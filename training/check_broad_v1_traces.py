"""Check reserved script traces independently of final script grades."""
from collections import Counter
import json
from pathlib import Path
import subprocess
import sys

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'scripts'))
from compare_human_runs import correct
from check_compound_v3_traces import expression, rows


def main():
    status = json.loads(Path('runs/sft-broad-v1/status.json').read_text())
    assert status['phase'] == 'Broad SFT training and evaluation complete'
    fixtures = {f['id']: f for f in rows('datasets/broad-v1-reserved/fixtures.jsonl') if f['task'] == 'write'}
    assert len(fixtures) == 32
    inputs = [dict(id='reference/' + tid, reference_policy=f['reference_policy'],
                   policy=f['reference_policy'], miniscript=f['reference_miniscript']) for tid, f in fixtures.items()]
    # A dropped branch and a malformed expression must not pass the audit.
    first = next(iter(fixtures.values()))
    inputs.append(dict(id='control/wrong', reference_policy=first['reference_policy'],
                       policy='pk(' + first['keys'][0]['pubkey'] + ')',
                       miniscript='pk(' + first['keys'][0]['pubkey'] + ')'))
    inputs.append(dict(id='control/invalid', reference_policy=first['reference_policy'],
                       policy='broken(', miniscript='broken('))
    final_results = {}
    for label, modes in status['reserved'].items():
        for mode, run in modes.items():
            directory = Path(run['directory'])
            files = ['chat-text.jsonl'] if mode == 'chat' else ['responses.jsonl', 'failures.jsonl']
            answers = {}
            for name in files:
                if not (directory / name).exists() and name == 'failures.jsonl':
                    continue
                for row in rows(directory / name):
                    assert row['task_id'] not in answers
                    answers[row['task_id']] = row
            assert set(fixtures) <= set(answers)
            final_results[f'{label}/{mode}'] = {r['task_id']: r for r in json.loads((directory / 'graded/results.json').read_text())}
            for tid, fixture in fixtures.items():
                raw = answers[tid].get('raw', '')
                inputs.append(dict(id=f'{label}/{mode}/{tid}', reference_policy=fixture['reference_policy'],
                                   **{kind: expression(raw, kind) for kind in ['policy', 'miniscript']}))
    prefix = 'runs/broad-v1-trace'
    serialized = ''.join(json.dumps(r) + '\n' for r in inputs)
    Path(prefix + '-input.jsonl').write_text(serialized)
    checked = subprocess.run(['target/release/examples/check_trace_parts'], input=serialized,
                             text=True, capture_output=True, check=True).stdout
    Path(prefix + '-checks.jsonl').write_text(checked)
    counts = {}
    for row in map(json.loads, checked.splitlines()):
        if row['id'].startswith('reference/'):
            assert all(row[kind]['status'] == 'equivalent' for kind in ['policy', 'miniscript'])
            continue
        if row['id'].startswith('control/'):
            expected = 'invalid' if row['id'].endswith('invalid') else 'not equivalent'
            assert all(row[kind]['status'] == expected for kind in ['policy', 'miniscript'])
            continue
        label, mode, tid = row['id'].split('/')
        group = counts.setdefault(f'{label}/{mode}', {k: Counter() for k in ['policy', 'miniscript', 'correct_final_miniscript']})
        for kind in ['policy', 'miniscript']:
            group[kind][row[kind]['status']] += 1
        if correct(final_results[f'{label}/{mode}'].get(tid)):
            group['correct_final_miniscript'][row['miniscript']['status']] += 1
    report = {'reference_controls': 32, 'negative_controls': 2, 'counts': counts,
              'scope': 'Reserved write tasks only. Explicit marked expressions, no syntax repair; Segwit v0. '
                       'Missing markers do not prove wrong semantics. This does not check tree reasoning. Final-answer grades are unchanged.'}
    Path(prefix + '-summary.json').write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps(report, indent=2))


if __name__ == '__main__':
    main()
