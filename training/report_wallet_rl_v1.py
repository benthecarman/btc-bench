"""Compare the fixed RL and SFT arms with their unchanged parent."""
from collections import Counter,defaultdict
import hashlib
import json
from pathlib import Path
import statistics


def digest(path):
    with Path(path).open('rb') as f:return hashlib.file_digest(f,'sha256').hexdigest()

def rows(path):return list(map(json.loads,Path(path).read_text().splitlines()))

def correct(row,wallet):
    return row is not None and (row['score']==1 if wallet else row.get('failure') in [None,'unimproved'])


def main():
    root=Path('runs/rl-wallet-v1')
    plan=json.loads(Path('training/wallet-rl-v1-experiment.json').read_text())
    status=json.loads((root/'status.json').read_text())
    assert status['phase']=='Wallet RL pilot complete; original server available' and not status.get('error')
    for p,expected in plan['hashes'].items():assert digest(p)==expected,p
    assert digest(Path(plan['parent'])/'model.safetensors')==plan['parent_weight_sha256']
    probe=json.loads((root/'probe-summary.json').read_text())
    report={'probe':probe['source_summary'],'training_eligible':probe['train'],'plan_sha256':digest('training/wallet-rl-v1-experiment.json')}
    if not probe['train']:
        with (root/'results.json').open('x') as f:json.dump(report,f,indent=2);f.write('\n')
        print('Probe complete; insufficient reward variation under the frozen selection rule.');return
    selected={r['task_id']:r for r in rows('datasets/rl-wallet-v1-selected.jsonl')}
    samples=rows(root/'rl/rollouts.jsonl');assert len(samples)==32
    training=defaultdict(list);counts=Counter();truncated=0;equivalent=0
    for r in samples:
        assert len(r['task_ids'])==len(r['completions'])==len(r['rewards'])==8
        assert len(set(r['task_ids']))==1
        source=selected[r['task_ids'][0]]['source_group']
        training[source].append([v['shaped'] for v in r['rewards']])
        counts.update(r['task_ids']);truncated+=sum(r['truncated']);equivalent+=sum(v['components']['equivalent'] for v in r['rewards'])
    report['training']={k:{'groups':len(v),'samples':sum(map(len,v)),'mixed':sum(statistics.pstdev(g)>1e-9 for g in v),
                         'mean_reward':statistics.mean(x for g in v for x in g)} for k,v in training.items()}
    report['training'].update(truncated=truncated,equivalent=equivalent,sample_counts=dict(counts))
    control=json.loads((root/'sft-control-data.json').read_text());assert control['counts']==dict(counts) and control['rows']==256
    assert digest('datasets/sft-wallet-rl-control-v1.jsonl')==control['sha256']
    audit=json.loads((root/'sft/loss-mask-audit.json').read_text());assert len(audit)==256
    source=rows('datasets/sft-wallet-rl-control-v1.jsonl')
    assert all(a['supervised_text'] in [r['completion'],r['completion']+'<|im_end|>'] for a,r in zip(audit,source))
    report['weights']={arm:{'path':str(root/arm/'merged'),'sha256':digest(root/arm/'merged/model.safetensors')} for arm in ['rl','sft']}
    assert all(report['weights'][arm]['sha256']==status['weights'][arm] for arm in ['rl','sft'])
    report['evaluation']={};finish=Counter()
    lines=['A fixed 32-update RL pilot and a matched 32-update reference SFT control completed from wallet SFT checkpoint 128. No evaluation result selected a checkpoint. The original serving model was restored.\n',
           'Training probe (eight completions per question):\n','| Source | Mixed groups | Mean reward | Semantically correct samples |','|---|---:|---:|---:|']
    for source,v in probe['source_summary'].items():lines.append(f"| {source} | {v['mixed']}/{v['groups']} | {v['mean_reward']:.3f} | {v['equivalent']}/{v['groups']*8} |")
    lines+=['\nThe selected pool has six wallet and six earlier tree questions. All earlier write groups had zero reward variation and were excluded by the frozen rule. Raw script performance is still tested.\n',
            'Actual RL sampling:\n','| Source | Groups | Mixed groups | Mean reward |','|---|---:|---:|---:|']
    for source in ['wallet','earlier']:
        v=report['training'][source];lines.append(f"| {source} | {v['groups']} | {v['mixed']} | {v['mean_reward']:.3f} |")
    lines += [f"\nRL sampled 256 completions; {truncated} reached the rollout limit. The SFT control used the same question counts and 256 verified reference completions, with all actual loss masks checked. This does not match token counts or GPU compute.\n",
              '| Development set | Interface | Task/output | Parent | RL | SFT control | RL gains/losses | SFT gains/losses |',
              '|---|---|---|---:|---:|---:|---:|---:|']
    for suite in plan['evaluation_suites']:
        fixtures=rows(f'datasets/{suite}/fixtures.jsonl');ids={f['id'] for f in fixtures};wallet=suite.startswith('wallet-')
        report['evaluation'][suite]={}
        for mode in ['chat','submit']:
            result={};graded={}
            for arm in ['parent','rl','sft']:
                d=Path('runs/sft-wallet-v1')/f'step128-{suite}-{mode}' if arm=='parent' else root/f'{arm}-{suite}-{mode}'
                raw=rows(d/('responses.jsonl' if wallet or mode=='submit' else 'chat-text.jsonl'))
                if not wallet and mode=='submit' and (d/'failures.jsonl').exists():raw+=rows(d/'failures.jsonl')
                assert len(raw)==len(ids) and {r['task_id'] for r in raw}==ids
                if arm!='parent':finish.update(r['finish_reason'] for r in raw)
                grades=json.loads((d/'graded/results.json').read_text());graded[arm]={r['task_id']:r for r in grades}
                assert len(graded[arm])==len(grades) and set(graded[arm])<=ids
                result[arm]={f['id']:correct(graded[arm].get(f['id']),wallet) for f in fixtures}
            kinds=sorted({f['output_kind'] if wallet else f['task'] for f in fixtures})
            cells={}
            for kind in kinds:
                group=[f['id'] for f in fixtures if (f['output_kind'] if wallet else f['task'])==kind]
                cell={'total':len(group)}
                for arm in result:
                    cell[arm]=dict(correct=sum(result[arm][i] for i in group),
                                   mean_score=sum(graded[arm].get(i,{}).get('score',0) for i in group)/len(group),
                                   gains=sum(result[arm][i] and not result['parent'][i] for i in group),
                                   losses=sum(result['parent'][i] and not result[arm][i] for i in group))
                cells[kind]=cell
                p,r,s=[cell[arm] for arm in ['parent','rl','sft']]
                lines.append(f"| {suite} | {mode} | {kind} | {p['correct']}/{len(group)} | {r['correct']}/{len(group)} | {s['correct']}/{len(group)} | {r['gains']}/{r['losses']} | {s['gains']}/{s['losses']} |")
            report['evaluation'][suite][mode]=cells
    lines += ['\nHuman-v2 tree mean benchmark score (includes the weight objective):\n',
              '| Interface | Parent | RL | SFT control |','|---|---:|---:|---:|']
    for mode in ['chat','submit']:
        cell=report['evaluation']['human-v2'][mode]['tree']
        lines.append('| '+mode+' | '+' | '.join(f"{cell[arm]['mean_score']:.3f}" for arm in ['parent','rl','sft'])+' |')
    report['finish_reasons']=dict(finish)
    lines += ['\nNew evaluation finish reasons: `'+json.dumps(report['finish_reasons'])+'`. All questions remain in the denominator, including missing answers and context-limit generations.',
              '\nAll five suites have been observed before this pilot, including the formerly reserved wallet set. These results are development measurements. Related template/concrete and chat/submit results are not independent samples. This is one seed and a small pool selected for reward variation.',
              '\nWallet/write rewards test final-answer correctness; tree reward also measures weight. None directly rewards the model reasoning. Separate trace checks are diagnostic and do not affect grades.',
              '\nFull rollout records, raw responses, model hashes and control counts are in `runs/rl-wallet-v1/`.']
    with (root/'results.json').open('x') as f:json.dump(report,f,indent=2);f.write('\n')
    with Path('training/wallet-rl-v1-results.md').open('x') as f:f.write('\n'.join(lines)+'\n')
    print('\n'.join(lines))


if __name__=='__main__':main()
