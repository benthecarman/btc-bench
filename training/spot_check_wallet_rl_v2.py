"""Save fixed parent/final-checkpoint examples for human review."""
import json
from pathlib import Path

from report_wallet_rl_v2 import directory
from report_wallet_rl_v1 import rows


def main():
    output = ['Fixed cases chosen before the longer run completed. These are development examples, not a random estimate of accuracy.\n']
    cases = [('wallet-policy-v1', 'wp-native-single-0-template'),
             ('wallet-policy-v1', 'wp-joint-councils-0-template'),
             ('wallet-sft-v1-reserved', 'wr1-age-and-height-template'),
             ('wallet-sft-v1-reserved', 'wr1-quorum-or-joint-template'),
             ('human-v2', 't4-human-tree-recovery')]
    for suite, tid in cases:
        fixture = next(f for f in rows(f'datasets/{suite}/fixtures.jsonl') if f['id'] == tid)
        output.append(f"**{tid}**\n\n{fixture['request']}\n")
        for label in ['parent', 'rl-step256', 'mixed-step256']:
            for mode in ['chat', 'submit']:
                d = directory(label, suite, mode)
                wallet = suite.startswith('wallet-')
                records = rows(d/('responses.jsonl' if wallet or mode == 'submit' else 'chat-text.jsonl'))
                if not wallet and mode == 'submit' and (d/'failures.jsonl').exists():
                    records += rows(d/'failures.jsonl')
                record = next(r for r in records if r['task_id'] == tid)
                grade = next((r for r in json.loads((d/'graded/results.json').read_text()) if r['task_id'] == tid), None)
                answer = record.get('text', record.get('answer'))
                if wallet:
                    message = record['raw_response']['choices'][0]['message']
                    trace = message.get('reasoning') or message.get('reasoning_content') or ''
                else:
                    trace = record.get('raw', '')
                if isinstance(answer, dict):
                    answer = json.dumps(answer)
                output += [f"{label}, {mode}. Grade: `{json.dumps(grade)}`\n\n```text\n{answer or '(missing final answer)'}\n```\n",
                           'Trace excerpt (first 2,000 characters; full response is saved in the run directory):\n\n'
                           f'```text\n{trace[:2000]}\n```\n']
    Path('training/wallet-rl-v2-spot-check.md').write_text('\n'.join(output))
    print('Saved five fixed cases for parent, final RL and final mixed checkpoints, in both interfaces')


if __name__ == '__main__':
    main()
