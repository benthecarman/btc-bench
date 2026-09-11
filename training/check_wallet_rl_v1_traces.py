"""Compare explicitly stated wallet policies/bodies across parent and both arms."""
from collections import Counter
import json
from pathlib import Path
from check_wallet_sft_v1_traces import check,expand,extract,rows


def main():
    root=Path('runs/rl-wallet-v1')
    inputs=[];meta=[]
    for arm in ['parent','rl','sft']:
        for suite in ['wallet-policy-v1','wallet-sft-v1-reserved']:
            fixtures={f['id']:f for f in rows(f'datasets/{suite}/fixtures.jsonl')}
            for mode in ['chat','submit']:
                d=Path('runs/sft-wallet-v1')/f'step128-{suite}-{mode}' if arm=='parent' else root/f'{arm}-{suite}-{mode}'
                grades={r['task_id']:r for r in json.loads((d/'graded/results.json').read_text())}
                responses=rows(d/'responses.jsonl');assert len(responses)==len(fixtures)
                for r in responses:
                    f=fixtures[r['task_id']];m=r['raw_response']['choices'][0]['message']
                    p,b=extract(m.get('reasoning') or m.get('reasoning_content') or '')
                    ident=f'{arm}/{suite}/{mode}/{f["id"]}'
                    inputs.append(dict(id=ident,fixture=f,policy=expand(f,p),bodies=[expand(f,x) for x in b] if b else None))
                    meta.append(dict(id=ident,arm=arm,suite=suite,mode=mode,correct=grades[f['id']]['score']==1))
    checked=check(inputs)
    result=[dict(**m,policy=r['policy'],bodies=r['bodies']) for m,r in zip(meta,checked)]
    summary={}
    for arm in ['parent','rl','sft']:
        rs=[r for r in result if r['arm']==arm]
        summary[arm]=dict(outputs=len(rs),correct=sum(r['correct'] for r in rs),
                          policy_counts=dict(Counter(r['policy']['status'] for r in rs)),
                          correct_with_bad_policy=sum(r['correct'] and r['policy']['status'] in ['invalid','not equivalent'] for r in rs),
                          correct_with_bad_bodies=sum(r['correct'] and r['bodies']['status']=='invalid or not equivalent' for r in rs))
    with (root/'trace-audit.json').open('x') as f:json.dump(dict(summary=summary,rows=result,scope='Explicit single-line expressions only; no syntax repair. Body checks use the reference internal key. Missing expressions are unmeasured. This is diagnostic and does not affect final-answer grades.'),f,indent=2);f.write('\n')
    print(json.dumps(summary,indent=2))


if __name__=='__main__':main()
