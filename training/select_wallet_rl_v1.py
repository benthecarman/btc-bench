"""Select a balanced pool using only the frozen training probe."""
from collections import Counter,defaultdict
import json
from pathlib import Path
import random
import statistics
import sys
sys.path.insert(0,str(Path(__file__).resolve().parents[1]/'scripts'))
from rl_probe import score,answer_from_sample


def main():
    root=Path('runs/rl-wallet-v1')
    records=list(map(json.loads,(root/'probe.jsonl').read_text().splitlines()))
    assert len(records)==64
    groups=[];eligible=defaultdict(list)
    for record in records:
        row=record['row'];task=json.loads(row['task_json'])
        assert not record.get('transport_error') and len(record['completions'])==8
        results=score([{'task':task,'answer':answer_from_sample(c,True,task)} for c in record['completions']], 'http://127.0.0.1:9902/reward/batch')
        rewards=[r['shaped'] for r in results]
        mixed=statistics.pstdev(rewards)>1e-9
        group=dict(task_id=row['task_id'],source_group=row['source_group'],kind=row['kind'],rewards=rewards,
                   mean=statistics.mean(rewards),mixed=mixed,
                   equivalent=sum(r['components']['equivalent'] for r in results),
                   finish_reasons=dict(Counter(c['finish_reason'] for c in record['completions'])))
        groups.append(group)
        if mixed:eligible[row['source_group']].append(row)
    # Choose equal numbers from each source group. Minimum four each was
    # frozen before inference; insufficient variation means no training.
    n=min(16,len(eligible['wallet']),len(eligible['earlier']))
    selected=[]
    if n>=4:
        rng=random.Random(7)
        for source in ['wallet','earlier']:
            selected += rng.sample(eligible[source],n)
        rng.shuffle(selected)
        with Path('datasets/rl-wallet-v1-selected.jsonl').open('x') as f:
            for row in selected:f.write(json.dumps(row)+'\n')
    summary=dict(groups=groups,eligible={k:len(v) for k,v in eligible.items()},train=n>=4,
                 selected_ids=[r['task_id'] for r in selected],selected_per_source=n if n>=4 else 0,
                 source_summary={source:dict(groups=sum(g['source_group']==source for g in groups),
                    mixed=sum(g['source_group']==source and g['mixed'] for g in groups),
                    mean_reward=statistics.mean(g['mean'] for g in groups if g['source_group']==source),
                    equivalent=sum(g['equivalent'] for g in groups if g['source_group']==source)) for source in ['wallet','earlier']})
    with (root/'probe-summary.json').open('x') as f:json.dump(summary,f,indent=2);f.write('\n')
    print(json.dumps({k:v for k,v in summary.items() if k!='groups'},indent=2))


if __name__=='__main__':main()
