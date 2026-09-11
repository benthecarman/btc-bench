"""Save fixed before/after cases for manual review, independent of scores."""
import json
from pathlib import Path


def rows(path):return list(map(json.loads,Path(path).read_text().splitlines()))


def main():
    root=Path('runs/rl-wallet-v1');output=[]
    cases=[('wallet-policy-v1','wp-native-single-0-template'),('wallet-policy-v1','wp-joint-councils-0-template'),
           ('wallet-sft-v1-reserved','wr1-age-and-height-template'),('wallet-sft-v1-reserved','wr1-quorum-or-joint-template'),
           ('human-v2','t4-human-tree-recovery')]
    for suite,ident in cases:
        f=next(f for f in rows(f'datasets/{suite}/fixtures.jsonl') if f['id']==ident)
        output += [f"**{ident}**\n\n{f['request']}\n"]
        for arm in ['parent','rl','sft']:
            for mode in ['chat','submit']:
                d=Path('runs/sft-wallet-v1')/f'step128-{suite}-{mode}' if arm=='parent' else root/f'{arm}-{suite}-{mode}'
                grades={r['task_id']:r for r in json.loads((d/'graded/results.json').read_text())}
                wallet=suite.startswith('wallet-')
                records=rows(d/('responses.jsonl' if wallet or mode=='submit' else 'chat-text.jsonl'))
                record=next((r for r in records if r['task_id']==ident),None)
                grade=grades.get(ident)
                if wallet:
                    answer=record.get('text') if mode=='chat' else record.get('answer')
                    message=record['raw_response']['choices'][0]['message']
                    trace=message.get('reasoning') or message.get('reasoning_content') or ''
                else:
                    answer=record.get('text',record.get('answer')) if record else None
                    if isinstance(answer,dict):answer=answer.get('descriptor',answer.get('script',str(answer)))
                    trace=record.get('raw','') if record else ''
                output += [f"{arm}, {mode}. Grade: `{json.dumps(grade)}`\n\n```text\n{answer or '(missing final answer)'}\n```\n",
                           'Reasoning/raw trace (first 2,000 characters; full response preserved in run directory):\n\n```text\n'+trace[:2000]+'\n```\n']
    with Path('training/wallet-rl-v1-spot-check.md').open('x') as f:f.write('\n'.join(output))
    print('Saved five fixed cases across parent, RL and SFT control, in both interfaces')


if __name__=='__main__':main()
