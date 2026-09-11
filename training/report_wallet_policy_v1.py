"""Report the fixed wallet pilot and save reference/model spot checks."""
from collections import Counter
import hashlib
import json
from pathlib import Path
from statistics import mean


def rows(path):
    return list(map(json.loads, Path(path).read_text().splitlines()))


def main():
    plan = json.loads(Path('evals/wallet-policy-v1-experiment.json').read_text())
    root = Path(plan['out'])
    status = json.loads((root / 'status.json').read_text())
    assert status['phase'] == 'Wallet evaluation complete; original server restored' and 'error' not in status
    for path, expected in plan['hashes'].items():
        assert hashlib.sha256(Path(path).read_bytes()).hexdigest() == expected, path
    fixtures = rows(Path(plan['dataset']) / 'fixtures.jsonl')
    by_id = {f['id']: f for f in fixtures}
    report = {'plan': plan, 'runs': {}, 'paired': {}, 'families': {}, 'restored_pid': status['restored_pid']}
    lines = ['# Wallet-policy v1 results', '',
             'Checkpoint 528 was tested before any wallet-policy training. All 40 questions are evaluation-only. '
             'Twenty scenarios each have a template and concrete descriptor request, grouped into ten families.', '',
             '| Interface | Template correct | Concrete correct |', '|---|---:|---:|']
    for mode in ['chat', 'submit']:
        directory = root / mode
        metadata = json.loads((directory / 'run.json').read_text())
        assert metadata['fixture_sha256'] == plan['hashes']['datasets/wallet-policy-v1/fixtures.jsonl']
        answers = rows(directory / 'responses.jsonl')
        assert len(answers) == 40 and {r['task_id'] for r in answers} == set(by_id)
        grades = json.loads((directory / 'graded/results.json').read_text())
        assert len(grades) == 40 and {g['task_id'] for g in grades} == set(by_id)
        grades = {g['task_id']: g for g in grades}
        summary = json.loads((directory / 'graded/summary.json').read_text())
        report['runs'][mode] = {'summary': summary, 'results': grades,
                                'finish_reasons': dict(Counter(r['finish_reason'] for r in answers)),
                                'answers': {r['task_id']: r for r in answers}}
        values = [f"{summary[k]['correct']}/{summary[k]['total']}" for k in ['template', 'concrete']]
        lines.append('| ' + mode + ' | ' + ' | '.join(values) + ' |')
        pairs = {k: [] for k in ['both', 'template_only', 'concrete_only', 'neither']}
        for f in fixtures:
            if f['output_kind'] != 'template':
                continue
            scenario = f['id'].removesuffix('-template')
            a = grades[scenario + '-template']['score'] == 1
            b = grades[scenario + '-concrete']['score'] == 1
            pairs['both' if a and b else 'template_only' if a else 'concrete_only' if b else 'neither'].append(scenario)
        report['paired'][mode] = pairs
        report['families'][mode] = {family: {kind: {
            'correct': sum(grades[f['id']]['score'] == 1 for f in fixtures if f['family'] == family and f['output_kind'] == kind),
            'total': sum(f['family'] == family and f['output_kind'] == kind for f in fixtures)}
            for kind in ['template', 'concrete']} for family in dict.fromkeys(f['family'] for f in fixtures)}
        report['runs'][mode]['tokens'] = {kind: {
            'mean_prompt_tokens': mean(r['raw_response']['usage']['prompt_tokens'] for r in answers if by_id[r['task_id']]['output_kind'] == kind),
            'mean_output_tokens': mean(r['output_tokens'] for r in answers if by_id[r['task_id']]['output_kind'] == kind)}
            for kind in ['template', 'concrete']}
    lines += ['', '| Interface | Both forms correct | Template only | Concrete only | Neither |', '|---|---:|---:|---:|---:|']
    for mode, pairs in report['paired'].items():
        lines.append('| ' + mode + ' | ' + ' | '.join(str(len(v)) for v in pairs.values()) + ' |')
    lines += ['', '## By family', '', '| Family | Chat template / concrete | Submit template / concrete |', '|---|---:|---:|']
    for family in report['families']['chat']:
        values = [' / '.join(str(report['families'][mode][family][kind]['correct']) + '/2' for kind in ['template', 'concrete']) for mode in ['chat', 'submit']]
        lines.append('| ' + family + ' | ' + ' | '.join(values) + ' |')
    lines += ['', '## Token counts', '', '| Interface | Form | Mean prompt tokens | Mean output tokens |', '|---|---|---:|---:|']
    for mode, run in report['runs'].items():
        for kind, values in run['tokens'].items():
            lines.append(f"| {mode} | {kind} | {values['mean_prompt_tokens']:.1f} | {values['mean_output_tokens']:.1f} |")
    lines += ['', '## Coverage and interpretation', '']
    for mode, run in report['runs'].items():
        lines.append(f"{mode}: 40/40 responses; finish reasons {run['finish_reasons']}.")
    if all(cell['correct'] == 0 for run in report['runs'].values() for cell in run['summary'].values()):
        lines += ['', 'Both representations are at zero accuracy in this baseline. This reveals an unmet output contract, '
                  'but it cannot establish whether placeholders improve construction or whether copying keys is the main problem. '
                  'A future training experiment needs separate wallet-format examples before this comparison can measure that difference.']
    lines += ['', 'All expected questions stay in the denominator. Every reference passed strict descriptor checks, '
              'semantic comparison with a separately authored policy, and canonical-English round trips before inference.', '',
              'The same scenarios and related variants appear in both forms and interfaces. These are familiar wallet patterns, '
              'not 80 independent transfer questions. Template prompts teach the placeholder/path notation; concrete prompts supply derived keys. '
              'Any difference combines representation, token load, and instruction effects. One seed does not isolate their causes.', '',
              'The local pilot uses a general descriptor submit tool and non-streaming JSON without textual tool fallback. '
              'Its scores should be compared within this pilot. No model parameters were updated.', '',
              'The original qwen3-4b-think model server was restored. '
              'See [the task contract](wallet-policy-v1.md), [the frozen plan](wallet-policy-v1-experiment.json), '
              'and [reference/model spot checks](wallet-policy-v1-examples.md).', '']
    (root / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
    Path('evals/wallet-policy-v1-results.md').write_text('\n'.join(lines))

    examples = ['wp-native-quorum-0', 'wp-recovery-delay-1', 'wp-joint-councils-0', 'wp-alternative-recovery-0']
    review = ['# Wallet-policy v1 spot checks', '',
              'Four fixed scenarios with their reference expressions and saved model answers. '
              'These examples are for review; they are not a random sample. The full model prompts also specify the output contract and key mapping.', '']
    for scenario in examples:
        f = by_id[scenario + '-template']
        review += ['## ' + scenario, '', f['spec_en'], '', 'Key mapping:', '',
                   ', '.join(f"@{i} = {k['label']}" for i, k in enumerate(f['keys'])), '',
                   'Separately authored spending policy:', '', '```text', f['policy_template'], '```', '',
                   'Reference descriptor template:', '', '```text', f['reference_template'], '```', '',
                   'Canonical BIP-388 English:', '', *['> ' + text for text in f['cleartext']], '',
                   '| Interface | Template | Concrete |', '|---|---|---|']
        for mode in ['chat', 'submit']:
            values = ['correct' if report['runs'][mode]['results'][scenario + '-' + k]['score'] == 1 else 'failed' for k in ['template', 'concrete']]
            review.append('| ' + mode + ' | ' + ' | '.join(values) + ' |')
        for kind in ['template', 'concrete']:
            tid = scenario + '-' + kind
            answer = report['runs']['chat']['answers'][tid]
            grade = report['runs']['chat']['results'][tid]
            review += ['', 'Saved chat ' + kind + ' answer:', '', answer.get('text', '(missing)'), '']
            if grade['reason']:
                review += ['Grader reason: `' + grade['reason'] + '`', '']
    Path('evals/wallet-policy-v1-examples.md').write_text('\n'.join(review))
    print('\n'.join(lines))


if __name__ == '__main__':
    main()
