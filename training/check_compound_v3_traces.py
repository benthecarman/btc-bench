"""Check explicit intermediate expressions without changing final-answer scores."""
import json
import re
import subprocess
from collections import Counter
from pathlib import Path


def rows(path):
    return [json.loads(line) for line in Path(path).read_text().splitlines()]


def expression(raw, kind):
    markers = (r'As a policy that is|Written as a policy|That gives the policy|Its spending policy is|The target is a threshold signature scheme'
               if kind == 'policy' else
               r'In Miniscript|The Miniscript for that policy|Compiling that to Miniscript for this context')
    found = re.findall(r'^(?:' + markers + r'): (.+)$', raw, re.M)
    # Use the last explicit expression, allowing a later correction. Do not repair syntax.
    return found[-1].strip() if found else None


def main():
    status = json.loads(Path('runs/compound-v3-status.json').read_text())
    assert status.get('selected'), 'Wait for checkpoint selection'
    fixtures = {r['id']: r for r in rows('datasets/compound-v3-fresh/fixtures.jsonl')}
    cases = json.loads(Path('training/compound-v3-fresh.json').read_text())['cases']
    buckets = {'t1-human-' + c['id']: c['validation_bucket'] for c in cases}
    inputs = [dict(id='reference/' + tid, reference_policy=f['reference_policy'],
                   policy=f['reference_policy'], miniscript=f['reference_miniscript'])
              for tid, f in fixtures.items()]
    finishes = {}
    for model in ['original', 'selected']:
        source = 'original' if model == 'original' or status['selected'] == 'original' else 'selected'
        for mode in ['chat', 'submit']:
            directory = Path(f'runs/compound-v3-{source}-fresh-{mode}')
            files = ['chat-text.jsonl'] if mode == 'chat' else ['responses.jsonl', 'failures.jsonl']
            answers = {}
            for file in files:
                if file == 'failures.jsonl' and not (directory / file).exists():
                    continue
                for row in rows(directory / file):
                    assert row['task_id'] not in answers
                    answers[row['task_id']] = row
            assert set(answers) == set(fixtures), f'Incomplete {model} {mode}'
            finishes[f'{model}/{mode}'] = dict(Counter(r.get('finish_reason', 'missing') for r in answers.values()))
            for tid, f in fixtures.items():
                raw = answers[tid].get('raw', '')
                inputs.append(dict(id=f'{model}/{mode}/{tid}', reference_policy=f['reference_policy'],
                                   **{kind: expression(raw, kind) for kind in ['policy', 'miniscript']}))
    serialized = ''.join(json.dumps(r) + '\n' for r in inputs)
    Path('runs/compound-v3-trace-input.jsonl').write_text(serialized)
    checked = subprocess.run(['target/release/examples/check_trace_parts'], input=serialized,
                             text=True, check=True, capture_output=True).stdout
    Path('runs/compound-v3-trace-checks.jsonl').write_text(checked)
    counts = {}
    for row in map(json.loads, checked.splitlines()):
        if row['id'].startswith('reference/'):
            assert all(row[k]['status'] == 'equivalent' for k in ['policy', 'miniscript'])
            continue
        model, mode, tid = row['id'].split('/')
        key = f'{model}/{mode}/{buckets[tid]}'
        group = counts.setdefault(key, {k: Counter() for k in ['policy', 'miniscript']})
        for kind in group:
            group[kind][row[kind]['status']] += 1
    report = dict(reference_controls=len(fixtures), counts=counts, finish_reasons=finishes,
                  scope='Explicit marked lines only; missing is not proof of incorrect semantics. '
                        'Miniscript is checked in Segwit v0 context. Final-script grades are unchanged.')
    Path('runs/compound-v3-trace-summary.json').write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps(report, indent=2))


if __name__ == '__main__':
    main()
