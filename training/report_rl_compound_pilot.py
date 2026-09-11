"""Report the fixed final RL/SFT comparison with every expected answer counted."""
import json
from pathlib import Path
from report_compound_v2 import read_run, correct


def main():
    status = json.loads(Path('runs/rl-compound-v3-pilot-v1-status.json').read_text())
    assert status['phase'] == 'RL and SFT pilot complete' and 'error' not in status
    hashes = json.loads(Path('training/compound-v3-experiment.json').read_text())['fixture_hashes']
    parent = dict(development='runs/compound-v3-step144-development-',
                  transfer='runs/compound-v3-selected-fresh-', human='runs/compound-v3-selected-human-')
    report = {}
    lines = ['# RLVR and additional-SFT pilot results', '',
             'Both methods start from compound-v3 checkpoint 144 and receive 32 optimizer updates. '
             'These are fixed final checkpoints. All evaluation questions have been observed in earlier experiments.', '',
             '| Model | Development chat | Development submit | Transfer chat | Transfer submit | Human write chat | Human write submit | Human tree chat | Human tree submit |',
             '|---|---:|---:|---:|---:|---:|---:|---:|---:|']
    for arm in ['parent', 'rl', 'sft']:
        report[arm] = {}
        values = []
        for part, suite in [('development', 'compound-v2-validation'), ('transfer', 'compound-v3-fresh'), ('human', 'human-v2')]:
            report[arm][part] = {}
            for mode in ['chat', 'submit']:
                directory = parent[part] + mode if arm == 'parent' else f'runs/rl-compound-v3-pilot-v1-{arm}-{part}-{mode}'
                data = read_run(suite, directory, hashes[suite])
                report[arm][part][mode] = data
            kinds = ['write', 'tree'] if part == 'human' else ['write']
            for kind in kinds:
                for mode in ['chat', 'submit']:
                    s = report[arm][part][mode]['summary'][kind]
                    values.append(f"{s['correct']}/{s['total']}")
        lines.append('| ' + arm + ' | ' + ' | '.join(values) + ' |')
    lines += ['', '## Paired outcomes', '', '| Arm | Suite | Interface | Task | Gained | Lost |', '|---|---|---|---|---:|---:|']
    for arm in ['rl', 'sft']:
        for part in report[arm]:
            for mode, data in report[arm][part].items():
                baseline = report['parent'][part][mode]['results']
                suite = {'development':'compound-v2-validation', 'transfer':'compound-v3-fresh', 'human':'human-v2'}[part]
                fixtures = [json.loads(l) for l in Path(f'datasets/{suite}/fixtures.jsonl').read_text().splitlines()]
                data['paired'] = {}
                for kind in data['summary']:
                    outcomes = {k: [] for k in ['both', 'gained', 'lost', 'neither']}
                    for task in fixtures:
                        if task['task'] != kind:
                            continue
                        tid = task['id']
                        old, new = correct(baseline.get(tid)), correct(data['results'].get(tid))
                        key = 'both' if old and new else 'lost' if old else 'gained' if new else 'neither'
                        outcomes[key].append(tid)
                    data['paired'][kind] = outcomes
                    lines.append(f"| {arm} | {part} | {mode} | {kind} | {len(outcomes['gained'])} | {len(outcomes['lost'])} |")
    lines += ['', 'Every expected question remains in the denominator. Missing or unextractable answers fail. '
              'Counts measure semantic correctness; tree weight remains separate. Chat and submit share questions and are not independent trials.', '',
              'See [the analysis](rl-compound-v3-pilot-v1-analysis.md), [spot checks](rl-compound-v3-pilot-v1-spot-check.md), '
              'and [design](rl-compound-v3-pilot-v1.md). Detailed results are in `runs/rl-compound-v3-pilot-v1-results.json`. '
              'The original model server is restored.', '']
    Path('runs/rl-compound-v3-pilot-v1-results.json').write_text(json.dumps(report, indent=2) + '\n')
    Path('training/rl-compound-v3-pilot-v1-results.md').write_text('\n'.join(lines))
    print('\n'.join(lines))


if __name__ == '__main__':
    main()
