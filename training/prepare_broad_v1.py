"""Create matched chat/submit rows with compound and original-skill replay."""
from collections import Counter
import hashlib
import json
from pathlib import Path
import random
import re
import sys

from prepare_compound_v3 import chat_pair, user_text
sys.path.insert(0,str(Path(__file__).resolve().parents[1]/'scripts'))
from audit_human_catalog import shape


def rows(path):
    return [json.loads(l) for l in Path(path).read_text().splitlines()]


def main():
    from transformers import AutoTokenizer
    model = 'runs/sft-compound-v3/merged-step144'
    tok = AutoTokenizer.from_pretrained(model)
    rng = random.Random(20260907)
    source_paths = ['datasets/sft-broad-v1-submit.jsonl','datasets/sft-compound-v3-submit.jsonl','datasets/sft-train-think.jsonl']
    new,compound,old = [rows(p) for p in source_paths]
    prepared = rows('datasets/rl-broad-v1-training.jsonl')
    compound_prepared = rows('datasets/rl-compound-v3-train.jsonl')
    assert len(new) == len(prepared) == 512
    reserved = {shape(c['policy']) for c in json.loads(Path('training/broad-v1-reserved.json').read_text())['cases']}
    selected = []
    for i,(row,task) in enumerate(zip(new,prepared)):
        assert row['prompt'] == task['prompt']
        selected.append(dict(source='broad',index=i,row=row,task_id=task['task_id'],kind=task['kind']))
    for version in ['v2','v3']:
        choices = [i for i,r in enumerate(compound_prepared) if f'compound-{version}-' in r['task_id']]
        for i in rng.sample(choices,64):
            selected.append(dict(source='compound_replay',index=i,row=compound[i],task_id=compound_prepared[i]['task_id'],kind='write'))
    rejected = Counter()
    for name,count in [('submit_script',256),('submit_descriptor',112),('submit_identify',48)]:
        candidates = [i for i,r in enumerate(old) if f'"name": "{name}"' in r['completion']]
        rng.shuffle(candidates)
        seen = set()
        accepted = 0
        for i in candidates:
            row = old[i]
            identity = json.dumps(row,sort_keys=True)
            if identity in seen:
                rejected['duplicate replay row'] += 1;continue
            text = user_text(row,chat=True)
            if 'submit_' in text or 'submit tool' in text.lower():
                rejected['tool instruction in chat replay'] += 1;continue
            chat,_ = chat_pair(row,tok)
            if max(len(tok.encode(x['prompt']+x['completion'],add_special_tokens=False)) for x in [row,chat])>4096:
                rejected['replay length'] += 1;continue
            selected.append(dict(source='original_replay',index=i,row=row,task_id=None,
                                 kind={'submit_script':'script','submit_descriptor':'tree','submit_identify':'identify'}[name]))
            seen.add(identity);accepted+=1
            if accepted==count:break
        assert accepted==count,(name,accepted)
    output = []
    chat_finals = []
    lengths = []
    source_counts = Counter()
    for item in selected:
        row = item['row']
        match = re.search(r'^(?:As a policy that is|Written as a policy|That gives the policy|Its spending policy is): (.+)$',row['completion'],re.M)
        if item['kind']!='identify':
            assert match, (item['source'],item['index'])
            assert shape(match[1]) not in reserved
        chat,final = chat_pair(row,tok,new=item['source']!='original_replay' and item['kind']=='write')
        for mode,pair in [('submit',row),('chat',chat)]:
            ids = tok.encode(pair['prompt']+pair['completion'],add_special_tokens=False)
            prompt_ids = tok.encode(pair['prompt'],add_special_tokens=False)
            assert ids[:len(prompt_ids)] == prompt_ids, 'Prompt boundary changes when target is appended'
            assert tok.decode(ids[len(prompt_ids):]) == pair['completion'], 'Target token round-trip changed'
            assert len(ids)<=4096,(item['source'],item['index'],mode,len(ids))
            lengths.append(len(ids))
            source_counts[item['source']+'/'+mode]+=1
            output.append(dict(source=item['source'],source_index=item['index'],task_id=item['task_id'],
                               kind=item['kind'],interface=mode,row=pair))
        if item['source']=='broad':
            chat_finals.append(dict(task_id=item['task_id'],text=final,finish_reason='stop'))
    rng.shuffle(output)
    path = Path('datasets/sft-broad-v1-mixed.jsonl')
    with path.open('x') as f:
        for r in output:f.write(json.dumps(r['row'])+'\n')
    with Path('datasets/broad-v1-chat-targets.jsonl').open('x') as f:
        for r in chat_finals:f.write(json.dumps(r)+'\n')
    report = dict(data=str(path),data_sha256=hashlib.sha256(path.read_bytes()).hexdigest(),
                  sources={p:hashlib.sha256(Path(p).read_bytes()).hexdigest() for p in source_paths},
                  tokenizer_model=model,seed=20260907,underlying_rows=len(selected),rows=len(output),
                  max_tokens=max(lengths),mean_tokens=sum(lengths)/len(lengths),counts=dict(source_counts),
                  rejected_replay=dict(rejected),reserved_shape_matches=0,
                  target_round_trip_checks=len(output),row_provenance=[{k:v for k,v in r.items() if k!='row'} for r in output])
    with Path('training/broad-v1-mix.json').open('x') as f:
        json.dump(report,f,indent=2);f.write('\n')
    print(json.dumps({k:v for k,v in report.items() if k!='row_provenance'},indent=2))


if __name__=='__main__':
    main()
