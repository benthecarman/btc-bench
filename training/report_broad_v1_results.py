"""Report the completed frozen broad SFT comparison, with paired outcomes."""
from collections import Counter
from fractions import Fraction
import hashlib
import json
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / 'scripts'))
from compare_human_runs import correct
from report_compound_v2 import read_run


def paired(fixtures, old, new):
    result = {}
    for kind in sorted({f['task'] for f in fixtures}):
        groups = {k: [] for k in ['both', 'gained', 'lost', 'neither']}
        for fixture in fixtures:
            if fixture['task'] != kind:
                continue
            tid = fixture['id']
            a, b = correct(old.get(tid)), correct(new.get(tid))
            groups['both' if a and b else 'lost' if a else 'gained' if b else 'neither'].append(tid)
        result[kind] = groups
    return result


def main():
    settings = json.loads(Path('training/broad-v1-experiment.json').read_text())
    root = Path(settings['out'])
    status = json.loads((root / 'status.json').read_text())
    assert status['phase'] == 'Broad SFT training and evaluation complete'
    selection = json.loads((root / 'selection.json').read_text())
    assert selection['selected'] == status['selected']
    assert selection['plan_sha256'] == hashlib.sha256(Path('training/broad-v1-experiment.json').read_bytes()).hexdigest()
    assert selection['selected'] == max(['parent', 'step264', 'step528'],
                                        key=lambda label: Fraction(selection['scores'][label]))
    report = {'selection': selection, 'development': {}, 'reserved': {}, 'paired': {},
              'reserved_families': {}, 'finish_reasons': {}, 'restored_pid': status['restored_pid']}
    for label, suites in status['development'].items():
        report['development'][label] = {}
        for suite, modes in suites.items():
            report['development'][label][suite] = {}
            for mode, run in modes.items():
                data = read_run(suite, run['directory'], settings['fixture_hashes'][suite])
                assert data['summary'] == run['summary']
                report['development'][label][suite][mode] = data
                report['finish_reasons'][f'{label}/{suite}/{mode}'] = run['finish_reasons']
    fixtures = list(map(json.loads, Path('datasets/broad-v1-reserved/fixtures.jsonl').read_text().splitlines()))
    cases = json.loads(Path('training/broad-v1-reserved.json').read_text())['cases']
    families = {('t1-human-' if c['kind'] == 'write' else 't4-human-') + c['id']: c['family'] for c in cases}
    assert set(families) == {f['id'] for f in fixtures}
    for label, modes in status['reserved'].items():
        report['reserved'][label] = {}
        report['reserved_families'][label] = {}
        for mode, run in modes.items():
            data = read_run('broad-v1-reserved', run['directory'], settings['fixture_hashes']['broad-v1-reserved'])
            assert data['summary'] == run['summary']
            report['reserved'][label][mode] = data
            report['finish_reasons'][f'{label}/reserved/{mode}'] = run['finish_reasons']
            report['reserved_families'][label][mode] = {
                family: {kind: {'correct': sum(correct(data['results'].get(f['id'])) for f in fixtures
                                                if families[f['id']] == family and f['task'] == kind),
                                'total': sum(families[f['id']] == family and f['task'] == kind for f in fixtures)}
                         for kind in ['write', 'tree']}
                for family in dict.fromkeys(families.values())}
    chosen = status['selected']
    for suite in settings['development_suites'] + ['broad-v1-reserved']:
        report['paired'][suite] = {}
        suite_fixtures = list(map(json.loads, (Path('datasets') / suite / 'fixtures.jsonl').read_text().splitlines()))
        for mode in ['chat', 'submit']:
            runs = {label: report['reserved'][label][mode] if suite == 'broad-v1-reserved'
                    else report['development'][label][suite][mode] for label in ['parent', chosen]}
            report['paired'][suite][mode] = paired(suite_fixtures, runs['parent']['results'], runs[chosen]['results'])
            command = [sys.executable, 'scripts/compare_human_runs.py', '--dataset', 'datasets/' + suite,
                       '--run', 'parent=' + runs['parent']['directory'], '--run', 'selected=' + runs[chosen]['directory'],
                       '--out', f'runs/broad-v1-comparison-{suite}-{mode}']
            subprocess.run(command, check=True, capture_output=True, text=True)
    state = json.loads((root / 'checkpoint-528/trainer_state.json').read_text())
    audit = json.loads((root / 'loss-mask-audit.json').read_text())
    report['training'] = {'updates': state['global_step'], 'epochs': state['epoch'],
                          'audited_rows': len(audit),
                          'max_trainer_tokens': max(r['prompt_tokens'] + r['supervised_tokens'] for r in audit),
                          'history': state['log_history']}
    lines = ['# Broad SFT results', '', f'The frozen development rule selected **{chosen}**.', '',
             'Training used 2,112 chat/submit rows, two epochs, 528 updates, and a learning rate of 0.00002. '
             'Both checkpoints start from compound-v3 checkpoint 144. All actual trainer loss masks passed.', '',
             '## Checkpoint selection', '',
             'All suites in this table were observed before this experiment. They are development data. '
             'Selection uses the mean of eight task/interface accuracy cells. Ties prefer fewer updates, including the unchanged parent.', '',
             '| Model | Compound v2 chat / submit | Compound v3 chat / submit | Human write chat / submit | Human tree chat / submit | Mean |',
             '|---|---:|---:|---:|---:|---:|']
    for label, suites in report['development'].items():
        cells = []
        for suite, kind in [('compound-v2-validation', 'write'), ('compound-v3-fresh', 'write'),
                            ('human-v2', 'write'), ('human-v2', 'tree')]:
            cells.append(' / '.join(f"{suites[suite][mode]['summary'][kind]['correct']}/{suites[suite][mode]['summary'][kind]['total']}"
                                    for mode in ['chat', 'submit']))
        lines.append('| ' + label + ' | ' + ' | '.join(cells) + f" | {float(Fraction(selection['scores'][label])):.3f} |")
    lines += ['', '## Reserved compositions', '',
              'These 64 questions were first run after selection was saved. They contain 32 related script/descriptor pairs. '
              'They are synthetic composition checks with shared primitives and wording; they are not independent human conversations.', '',
              '| Model | Script chat | Script submit | Tree chat | Tree submit |', '|---|---:|---:|---:|---:|']
    for label, modes in report['reserved'].items():
        values = [f"{modes[mode]['summary'][kind]['correct']}/{modes[mode]['summary'][kind]['total']}"
                  for kind in ['write', 'tree'] for mode in ['chat', 'submit']]
        lines.append('| ' + label + ' | ' + ' | '.join(values) + ' |')
    lines += ['', '| Reserved family | Model | Script chat / submit | Tree chat / submit |', '|---|---|---:|---:|']
    for family in dict.fromkeys(families.values()):
        for label, modes in report['reserved_families'].items():
            values = [' / '.join(f"{modes[mode][family][kind]['correct']}/{modes[mode][family][kind]['total']}" for mode in ['chat', 'submit'])
                      for kind in ['write', 'tree']]
            lines.append('| ' + family + ' | ' + label + ' | ' + ' | '.join(values) + ' |')
    lines += ['', '## Paired changes', '', '| Suite | Interface | Task | Gained | Lost | Both correct | Neither |',
              '|---|---|---|---:|---:|---:|---:|']
    for suite, modes in report['paired'].items():
        for mode, kinds in modes.items():
            for kind, groups in kinds.items():
                lines.append('| ' + ' | '.join([suite, mode, kind] + [str(len(groups[k])) for k in ['gained', 'lost', 'both', 'neither']]) + ' |')
    finishes = Counter()
    for reasons in report['finish_reasons'].values():
        finishes.update(reasons)
    report['all_finish_reasons'] = dict(finishes)
    lines += ['', '## Coverage and limits', '',
              f"Recorded finish reasons across reported runs: {dict(finishes)}. The saved parent human-v2 submit run contains one context-limit failure. "
              'Every fixture stays in its denominator. Missing answers score zero. No extra output or time cap was added.', '',
              'Correct means semantic equivalence under the existing oracle. Tree correctness includes equivalent descriptors '
              'that do not improve reference weight; weight scores remain in the raw results. '
              'This comparison uses the same grader mode as the parent runs. Strict reference checks are separate.', '',
              'The test does not establish signed-transaction correctness. The authored tree set includes an explicit owner route; '
              'it does not cover committee-only Taproot outputs. A single seed does not establish repeatability.', '',
              'The original qwen3-4b-think server was restored. New checkpoints remain separate model directories.', '',
              'See [the fixed experiment plan](broad-v1-experiment.json). Per-question grades and paired IDs are saved in '
              '`runs/broad-v1-results.json`; prompts, responses, and logs remain in the individual run directories.', '']
    Path('runs/broad-v1-results.json').write_text(json.dumps(report, indent=2) + '\n')
    Path('training/broad-v1-results.md').write_text('\n'.join(lines))
    print('\n'.join(lines))


if __name__ == '__main__':
    main()
