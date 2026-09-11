"""Compare both interfaces on development, fresh compositions, and human-v2."""
import json
from pathlib import Path
from report_compound_v2 import read_run, correct


def main():
    settings = json.loads(Path('training/compound-v3-experiment.json').read_text())
    status = json.loads(Path('runs/compound-v3-status.json').read_text())
    assert status['phase'] == 'Gated-team and chat experiment complete'
    report = dict(selected=status['selected'], development={}, fit={}, fresh={}, human={})
    lines = ['# Gated-team and chat training results', '',
             'The old development questions select the checkpoint. Fresh compositions and human-v2 do not select it.', '',
             '## Development and training fit', '',
             '| Model | Development chat | Development submit | Fit chat | Fit submit |',
             '|---|---:|---:|---:|---:|']
    totals = {}
    for label in ['original', 'step144', 'step288']:
        values = []
        for part, suite in [('development', 'compound-v2-validation'), ('fit', 'compound-v3-fit-sample')]:
            report[part][label] = {}
            for mode in ['chat', 'submit']:
                data = read_run(suite, settings['runs'][label][part][mode], settings['fixture_hashes'][suite])
                report[part][label][mode] = data
                summary = data['summary']['write']
                values.append(f"{summary['correct']}/{summary['total']}")
        totals[label] = sum(report['development'][label][mode]['summary']['write']['correct'] for mode in ['chat', 'submit'])
        lines.append('| ' + label + ' | ' + ' | '.join(values) + ' |')
    chosen = max(['original', 'step144', 'step288'], key=lambda label: totals[label])
    assert chosen == status['selected']
    lines += ['', f'The frozen rule selects **{chosen}**. Each development question appears in two interfaces; the 96 answers are not 96 independent questions.', '',
              '## Fresh composition check', '',
              '| Model and interface | New single-team arrangements | Two conditioned teams | Total |',
              '|---|---:|---:|---:|']
    catalog = json.loads(Path('training/compound-v3-fresh.json').read_text())['cases']
    buckets = {name: {'t1-human-' + c['id'] for c in catalog if c['validation_bucket'] == name}
               for name in ['new single gated-team arrangement', 'two gated-team approvals']}
    for label in ['original', 'selected']:
        report['fresh'][label] = {}
        for mode in ['chat', 'submit']:
            directory = settings['runs']['original']['fresh'][mode] if label == 'original' or chosen == 'original' else f'runs/compound-v3-selected-fresh-{mode}'
            data = read_run('compound-v3-fresh', directory, settings['fixture_hashes']['compound-v3-fresh'])
            data['buckets'] = {b: sum(correct(data['results'].get(tid)) for tid in ids) for b, ids in buckets.items()}
            report['fresh'][label][mode] = data
            values = [f'{n}/24' for n in data['buckets'].values()]
            values.append(f"{data['summary']['write']['correct']}/48")
            lines.append('| ' + label + ' ' + mode + ' | ' + ' | '.join(values) + ' |')
    lines += ['', 'These synthetic questions have normalized structures absent from the checked training sources. '
              'Two conditioned-team approvals never occur together in this training curriculum. '
              'The templates and vocabulary remain related; this is not an independently authored human test.', '',
              '## Existing human-style benchmark', '',
              '| Model | Write chat | Tree chat | Write submit | Tree submit |',
              '|---|---:|---:|---:|---:|']
    for label in ['original', 'selected']:
        report['human'][label] = {}
        values = []
        for mode in ['chat', 'submit']:
            original = 'runs/human-v2-sft' + ('-submit' if mode == 'submit' else '')
            directory = original if label == 'original' or chosen == 'original' else f'runs/compound-v3-selected-human-{mode}'
            data = read_run('human-v2', directory, settings['fixture_hashes']['human-v2'])
            report['human'][label][mode] = data
            values += [f"{data['summary'][kind]['correct']}/{data['summary'][kind]['total']}" for kind in ['write', 'tree']]
        lines.append('| ' + label + ' | ' + ' | '.join(values) + ' |')
    lines += ['', 'All expected questions remain in the denominator, including unextractable answers. Counts measure semantic correctness; tree weight is separate. '
              'The original model server is restored. This run changes the curriculum, interface mix, replay sample, and learning rate, so it cannot isolate one cause of any gain.', '',
              'See [the design](compound-v3.md), [error analysis and trace checks](compound-v3-analysis.md), '
              'and [three spot checks](compound-v3-spot-check.md). Full per-question results are in `runs/compound-v3-results.json`.', '']
    Path('runs/compound-v3-results.json').write_text(json.dumps(report, indent=2) + '\n')
    Path('training/compound-v3-results.md').write_text('\n'.join(lines))
    print('\n'.join(lines))


if __name__ == '__main__':
    main()
