"""Prepare a balanced training-only probe and exact reference-control targets."""
from collections import defaultdict
import hashlib
import json
from pathlib import Path
import random
import sys
sys.path.insert(0,str(Path(__file__).resolve().parents[1]/'scripts'))
from rl_common import validate_rows
from run_wallet_bench import request_body


def rows(path):return list(map(json.loads,Path(path).read_text().splitlines()))
def digest(path):return hashlib.sha256(Path(path).read_bytes()).hexdigest()
def write_rows(path,data):
    with Path(path).open('x') as f:
        for row in data:f.write(json.dumps(row)+'\n')


def main():
    from transformers import AutoTokenizer
    parent='runs/sft-wallet-v1/merged-step128'
    tok=AutoTokenizer.from_pretrained(parent)
    fixture_path=Path('datasets/wallet-sft-v1-training/fixtures.jsonl')
    manifest=json.loads(fixture_path.with_name('manifest.json').read_text())
    assert manifest['evaluation_only'] is False and digest(fixture_path)==manifest['fixtures_sha256']
    fixtures=rows(fixture_path)
    assert all(f['split']=='training' for f in fixtures)
    sources=rows('datasets/sft-wallet-v1-mixed.jsonl')
    provenance=json.loads(Path('training/wallet-sft-v1-mix.json').read_text())['row_provenance']
    assert len(sources)==len(provenance)
    wallet_gold={p['source_index']:r for p,r in zip(provenance,sources) if p['source']=='wallet' and p['interface']=='submit'}
    rng=random.Random(202609072)
    families=defaultdict(list)
    for f in fixtures:
        if f['output_kind']=='template':families[f['family']].append(f['id'].removesuffix('-template'))
    picked={rng.choice(ids) for ids in families.values()}
    prepared=[];gold=[]
    for f in fixtures:
        scenario=f['id'].rsplit('-',1)[0]
        if scenario not in picked:continue
        body=request_body(f,'unused','submit')
        prompt=tok.apply_chat_template(body['messages'],tools=body['tools'],add_generation_prompt=True,enable_thinking=True,tokenize=False)
        assert wallet_gold[f['id']]['prompt']==prompt
        row=dict(prompt=prompt,task_json=json.dumps({'task':'wallet','fixture':f}),kind='wallet-'+f['output_kind'],
                 task_id=f['id'],thinking=True,messages=body['messages'],tools=body['tools'],source_group='wallet')
        prepared.append(row);gold.append(dict(task_id=f['id'],row=wallet_gold[f['id']]))
    assert len(prepared)==32
    broad=rows('datasets/rl-broad-v1-training.jsonl')
    broad_gold=rows('datasets/sft-broad-v1-submit.jsonl')
    assert len(broad)==len(broad_gold)==512
    for kind in ['write','tree']:
        eligible=[i for i,r in enumerate(broad) if r['kind']==kind]
        for i in rng.sample(eligible,16):
            row=broad[i];target=broad_gold[i]
            assert row['prompt']==target['prompt']
            assert 'train-broad-v1' in row['task_id'], 'Only training catalog IDs allowed'
            prepared.append(dict(row,source_group='earlier'))
            gold.append(dict(task_id=row['task_id'],row=target))
    validate_rows(prepared,True)
    assert len(prepared)==64 and len({r['task_id'] for r in prepared})==64
    for row in prepared:
        assert len(tok.encode(row['prompt'],add_special_tokens=False))+4096<=8192
    write_rows('datasets/rl-wallet-v1-probe.jsonl',prepared)
    write_rows('datasets/rl-wallet-v1-gold.jsonl',gold)
    print('Prepared 64 questions: 32 wallet, 16 earlier write, 16 earlier tree; eight samples per question planned')


if __name__=='__main__':main()
