"""Save reviewed successes and failures with their reference expressions."""
import json
from pathlib import Path
import sys

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'scripts'))
from compare_human_runs import correct
from check_compound_v3_traces import expression, rows


def main():
    report = json.loads(Path('runs/broad-v1-results.json').read_text())
    assert report['selection']['selected'] == 'step528'
    checks = {r['id']: r for r in rows('runs/broad-v1-trace-checks.jsonl')}
    examples = [
        ('broad-v1-reserved', 't1-human-composed-val-broad-v1-two-locked-departments-007-write',
         'A complete script success',
         'The selected model passes in both interfaces. Its stated policy and Miniscript also pass in both. '
         'It keeps the relative clock on the first council and the absolute clock on the second.'),
        ('broad-v1-reserved', 't4-human-composed-val-broad-v1-alternative-evidence-bundles-005-tree',
         'A valid descriptor with a different expression',
         'The selected descriptor passes in both interfaces. The chat answer uses or_i where the reference uses andor. '
         'The oracle accepts their equivalent spending behavior. The owner route remains separate from both evidence bundles.'),
        ('broad-v1-reserved', 't1-human-composed-val-broad-v1-three-conditioned-departments-006-write',
         'A correct final script with invalid stated Miniscript',
         'The final script and policy pass in both interfaces, but the stated Miniscript fails type checking in both. '
         'The c wrapper is applied to a B fragment. The correct final script does not establish that the displayed derivation is valid.'),
        ('broad-v1-reserved', 't1-human-composed-val-broad-v1-alternative-evidence-bundles-000-write',
         'A correct stated policy followed by a rejected script',
         'In chat, the policy is equivalent, but the stated Miniscript is invalid in Segwit v0 and the final script is rejected. '
         'The expression mixes a multi_a fragment with compressed keys and changes the arrangement of the required approvals.'),
        ('human-v2', 't4-human-tree-recovery',
         'The requested answer type is still missed in chat',
         'The request explicitly asks for a tr() descriptor. The selected chat answer returns raw Bitcoin script instead. '
         'The selected submit answer to the same question is a correct descriptor. This is a remaining instruction and output-format problem.'),
    ]
    lines = ['# Broad SFT spot checks', '',
             'These are saved evaluation answers from checkpoint 528 and its parent. The reserved examples were inspected after checkpoint selection. '
             'No answer was repaired. Public keys below are replaced with their request labels for readability; these labelled examples are not runnable. '
             'The exact prompts, answers, and references are saved in `runs/broad-v1-spot-check.json`.', '']
    saved = []
    for suite, tid, title, note in examples:
        fixture = next(f for f in rows(Path('datasets') / suite / 'fixtures.jsonl') if f['id'] == tid)
        def labelled(text):
            for key in fixture['keys']:
                text = text.replace(key['pubkey'], key['label'])
            return text
        sample = {'suite': suite, 'id': tid, 'note': note, 'fixture': fixture, 'answers': {}}
        lines += ['## ' + title, '', '`' + tid + '`', '', note, '', 'Request:', '', fixture['spec_en'], '',
                  'Reference policy:', '', '```text', labelled(fixture['reference_policy']), '```', '',
                  'Reference ' + ('Miniscript' if fixture['task'] == 'write' else 'descriptor') + ':', '', '```text',
                  labelled(fixture['reference_miniscript'] if fixture['task'] == 'write' else fixture['reference_descriptor']), '```', '',
                  '| Model | Interface | Final answer | Stated policy | Stated Miniscript |', '|---|---|---|---|---|']
        for label in ['parent', 'step528']:
            for mode in ['chat', 'submit']:
                data = report['reserved'][label][mode] if suite == 'broad-v1-reserved' else report['development'][label][suite][mode]
                directory = Path(data['directory'])
                filenames = ['chat-text.jsonl'] if mode == 'chat' else ['responses.jsonl', 'failures.jsonl']
                answer = next(r for filename in filenames if (directory / filename).exists()
                              for r in rows(directory / filename) if r['task_id'] == tid)
                grade = data['results'].get(tid)
                check = checks.get(f'{label}/{mode}/{tid}') if suite == 'broad-v1-reserved' else None
                key = label + '/' + mode
                sample['answers'][key] = {'record': answer, 'grade': grade, 'trace_check': check}
                states = [check[kind]['status'] if check else 'not checked' for kind in ['policy', 'miniscript']]
                final = 'correct' if correct(grade) else grade.get('failure', 'missing') if grade else 'missing'
                lines.append('| ' + ' | '.join([label, mode, final] + states) + ' |')
        answer = sample['answers']['step528/chat']['record']
        raw = answer.get('raw', '')
        for kind in ['policy', 'miniscript']:
            text = expression(raw, kind)
            if text:
                lines += ['', 'Selected chat ' + kind + ':', '', '```text', labelled(text), '```']
        lines += ['', 'Selected chat final answer:', '', labelled(answer.get('text', '')), '']
        if fixture['task'] == 'write':
            check = sample['answers']['step528/chat']['trace_check']
            if check['miniscript']['status'] != 'equivalent':
                lines += ['Miniscript check:', '', '```text', labelled(check['miniscript'].get('reason', 'missing')), '```', '']
        saved.append(sample)
    Path('runs/broad-v1-spot-check.json').write_text(json.dumps(saved, indent=2) + '\n')
    Path('training/broad-v1-spot-check.md').write_text('\n'.join(lines))
    print(f'Saved {len(saved)} reviewed examples')


if __name__ == '__main__':
    main()
