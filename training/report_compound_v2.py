"""Report the frozen expanded-curriculum experiment from completed runs."""
import hashlib
import json
from pathlib import Path
import sys

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'scripts'))
from compare_human_runs import check_completions, correct, summarize


def read_run(suite, directory, expected_hash):
    path = Path('datasets') / suite / 'fixtures.jsonl'
    assert hashlib.sha256(path.read_bytes()).hexdigest() == expected_hash
    fixtures = list(map(json.loads, path.read_text().splitlines()))
    ids = {f['id'] for f in fixtures}
    directory = Path(directory)
    metadata = json.loads((directory / 'run.json').read_text())
    assert metadata['dataset_manifest']['fixtures_sha256'] == expected_hash
    check_completions(directory, metadata, ids)
    rows = json.loads((directory / 'graded/results.json').read_text())
    results = {r['task_id']: r for r in rows}
    assert len(results) == len(rows) and set(results) <= ids
    return dict(directory=str(directory), results=results, summary=summarize(fixtures, results))


def main():
    settings = json.loads(Path('training/compound-v2-experiment.json').read_text())
    status = json.loads(Path('runs/compound-v2-status.json').read_text())
    assert status['phase'] == 'Expanded training and validation complete'
    catalog = json.loads(Path('training/compound-v2-validation.json').read_text())['cases']
    buckets = {bucket: {'t1-human-' + c['id'] for c in catalog if c['validation_bucket'] == bucket}
               for bucket in ['new arrangement', 'held-out gated teams']}
    report = dict(selected=status['selected'], candidates={}, original_skills={})
    lines = ['# Expanded compound-approval results', '',
             'Validation selects the checkpoint. The training-fit sample is diagnostic; neither set is a final test.', '',
             '| Model | New arrangements | Held-out gated teams | Validation total | Training-fit sample |',
             '|---|---:|---:|---:|---:|']
    for label in ['original', 'step96', 'step192']:
        validation = read_run('compound-v2-validation', f'runs/compound-v2-{label}-validation',
                              settings['fixture_hashes']['compound-v2-validation'])
        fit = read_run('compound-v2-fit-sample', f'runs/compound-v2-{label}-fit',
                       settings['fixture_hashes']['compound-v2-fit-sample'])
        scores = {b: sum(correct(validation['results'].get(tid)) for tid in ids) for b, ids in buckets.items()}
        total, fitted = validation['summary']['write']['correct'], fit['summary']['write']['correct']
        report['candidates'][label] = dict(validation=validation, fit=fit, buckets=scores)
        lines.append(f"| {label} | {scores['new arrangement']}/24 | {scores['held-out gated teams']}/24 | {total}/48 | {fitted}/24 |")
    chosen = max(['original', 'step96', 'step192'], key=lambda label: report['candidates'][label]['validation']['summary']['write']['correct'])
    assert chosen == status['selected']
    lines += ['', f'The frozen selection rule chooses **{chosen}**. Ties prefer fewer updates.', '',
              '## Existing human-style benchmark', '',
              '| Model | Write chat | Tree chat | Write submit | Tree submit |',
              '|---|---:|---:|---:|---:|']
    for label in ['original', 'selected']:
        report['original_skills'][label] = {}
        counts = []
        for mode in ['chat', 'submit']:
            original = 'runs/human-v2-sft' + ('-submit' if mode == 'submit' else '')
            directory = original if label == 'original' or chosen == 'original' else f'runs/human-v2-compound-v2-{mode}'
            data = read_run('human-v2', directory, settings['fixture_hashes']['human-v2'])
            report['original_skills'][label][mode] = data
            counts += [f"{data['summary'][kind]['correct']}/{data['summary'][kind]['total']}" for kind in ['write', 'tree']]
        lines.append('| ' + label + ' | ' + ' | '.join(counts) + ' |')
    lines += ['', 'Counts measure semantic correctness and retain every expected question in the denominator. '
              'Tree weight is recorded separately in the raw comparisons.', '',
              'The original server was restored after evaluation. The selected checkpoint remains a separate experiment. '
              'The existing-skill check covers human-v2 write and tree tasks; it does not measure every original task type.', '',
              'See [the experiment design](compound-v2.md) and [example requests and references](compound-v2-examples.md). '
              'Full per-question results are in `runs/compound-v2-results.json`.', '']
    Path('runs/compound-v2-results.json').write_text(json.dumps(report, indent=2) + '\n')
    Path('training/compound-v2-results.md').write_text('\n'.join(lines))
    print('\n'.join(lines))


if __name__ == '__main__':
    main()
