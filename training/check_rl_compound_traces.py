"""Check transfer-answer intermediate expressions for the fixed pilot arms."""
from collections import Counter
import json
from pathlib import Path
import subprocess
from check_compound_v3_traces import expression, rows


def main():
    fixtures = {r['id']: r for r in rows('datasets/compound-v3-fresh/fixtures.jsonl')}
    inputs = [dict(id='reference/' + tid, reference_policy=f['reference_policy'],
                   policy=f['reference_policy'], miniscript=f['reference_miniscript'])
              for tid, f in fixtures.items()]
    final_results = {}
    for arm in ['parent', 'rl', 'sft']:
        for mode in ['chat', 'submit']:
            directory = Path(f'runs/compound-v3-selected-fresh-{mode}' if arm == 'parent'
                             else f'runs/rl-compound-v3-pilot-v1-{arm}-transfer-{mode}')
            files = ['chat-text.jsonl'] if mode == 'chat' else ['responses.jsonl', 'failures.jsonl']
            answers = {}
            for name in files:
                if name == 'failures.jsonl' and not (directory / name).exists():
                    continue
                for r in rows(directory / name):
                    assert r['task_id'] not in answers
                    answers[r['task_id']] = r
            assert set(answers) == set(fixtures)
            final_results[f'{arm}/{mode}'] = {r['task_id']: r for r in json.loads((directory / 'graded/results.json').read_text())}
            for tid, f in fixtures.items():
                raw = answers[tid].get('raw', '')
                inputs.append(dict(id=f'{arm}/{mode}/{tid}', reference_policy=f['reference_policy'],
                                   **{k: expression(raw, k) for k in ['policy', 'miniscript']}))
    serialized = ''.join(json.dumps(r) + '\n' for r in inputs)
    prefix = 'runs/rl-compound-v3-pilot-v1-trace'
    Path(prefix + '-input.jsonl').write_text(serialized)
    result = subprocess.run(['target/release/examples/check_trace_parts'], input=serialized,
                            text=True, capture_output=True, check=True).stdout
    Path(prefix + '-checks.jsonl').write_text(result)
    counts = {}
    for row in map(json.loads, result.splitlines()):
        if row['id'].startswith('reference/'):
            assert all(row[k]['status'] == 'equivalent' for k in ['policy', 'miniscript'])
            continue
        arm, mode, tid = row['id'].split('/')
        key = f'{arm}/{mode}'
        group = counts.setdefault(key, {k: Counter() for k in ['policy', 'miniscript', 'correct_final_miniscript']})
        for k in ['policy', 'miniscript']:
            group[k][row[k]['status']] += 1
        final = final_results[key].get(tid)
        if final is not None and final.get('failure') is None:
            group['correct_final_miniscript'][row['miniscript']['status']] += 1
    report = dict(reference_controls=48, counts=counts,
                  scope='Explicit marked expressions, no syntax repair, Segwit v0 context. Missing markers do not prove wrong semantics. Final-answer grades are unchanged.')
    Path(prefix + '-summary.json').write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps(report, indent=2))


if __name__ == '__main__':
    main()
