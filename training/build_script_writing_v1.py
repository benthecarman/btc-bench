"""Build a writing-only chat curriculum and writing-only evaluation pools."""
import hashlib
import json
from pathlib import Path
import random
import re
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0,str(ROOT/'scripts'))
from audit_human_catalog import shape
from rl_replay import prepare_replay

NAME = 'script-writing-v1'
SEED = 20260912
PARENT = 'runs/sft-broad-v1/merged-step528'
CONTEXTS = {'legacy':'P2SH redeem script','segwitv0':'P2WSH witness script','tap':'tapscript leaf'}
FAMILIES = ['signature','both-signatures','quorum','mandatory-quorum','two-teams',
            'relative','absolute','both-clocks','relative-recovery','absolute-recovery',
            'hash-refund','delayed-choice']
RESERVED = ['guarded-hash-recovery','two-conditioned-teams','three-alternatives','shared-secret']


def contract(family, variant):
    delay = [1,16,17,127,128,255,256,512][variant]
    absolute = [930000,930127,1800000000,1800000512,940000,940128,1810000000,1810000512][variant]
    older = delay if variant%2==0 else (1<<22)+delay
    age = (f'{delay} blocks from confirmation' if variant%2==0 else
           f'{delay*512} seconds on the relative-lock clock ({delay} units of 512 seconds)')
    locktext = (f'block-height nLockTime of at least {absolute}' if absolute<500000000 else
                f'time-based nLockTime of at least Unix time {absolute}')
    pk = lambda n:f'pk(${n})'
    conjunction = lambda a,b:f'and({a},{b})'
    alternative = lambda a,b:f'or({a},{b})'
    verify = lambda a,b:f'and_v(v:{a},{b})'
    branch = lambda a,b:f'or_i({a},{b})'
    a,b,c,d = [pk(i) for i in range(4)]
    rel,ab = f'older({older})',f'after({absolute})'
    secret = 'sha256($sha256)'
    quorum = 'thresh(2,pk($0),pk($1),pk($2))'
    multi = 'multi(2,$0,$1,$2)'
    names = ['Alice','Bob','Carol','Dave','Erin','Frank','Grace']
    if family=='signature':
        count,policy,ms=1,a,a
        text='Alice alone authorizes a spend with her signature.'
        note='Push the supplied public key and check its signature.'
    elif family=='both-signatures':
        count,policy,ms=2,conjunction(a,b),verify(a,b)
        text='Both Alice and Bob must sign. Neither can spend alone.'
        note='Verify the first signature, then check the second. Both checks are mandatory.'
    elif family=='quorum':
        threshold,count=[(1,2),(2,3),(3,5),(4,6)][variant%4]
        policy=f'thresh({threshold},'+','.join(pk(i) for i in range(count))+')'
        ms=f'multi({threshold},'+','.join(f'${i}' for i in range(count))+')'
        text=f'Any {threshold} of '+', '.join(names[:count])+f' can authorize a spend. Fewer than {threshold} signatures must fail.'
        note=f'Count signatures from exactly these {count} keys and require {threshold}. Use the multisignature encoding for the requested context.'
    elif family=='mandatory-quorum':
        count=4; team='thresh(2,pk($1),pk($2),pk($3))'
        policy,ms=conjunction(a,team),verify(a,'multi(2,$1,$2,$3)')
        text='Alice must sign every withdrawal, together with at least two of Bob, Carol, and Dave. Those three cannot spend without Alice.'
        note='Verify the mandatory signer separately from the two-of-three quorum.'
    elif family=='two-teams':
        count=6; team='thresh(2,pk($3),pk($4),pk($5))'
        policy,ms=conjunction(quorum,team),verify(multi,'multi(2,$3,$4,$5)')
        text='Two of Alice, Bob, and Carol must sign, AND two of Dave, Erin, and Frank must sign. Each team must satisfy its own threshold.'
        note='Verify one team threshold and then check the other team threshold. Count the teams separately.'
    elif family in ['relative','absolute','both-clocks']:
        count=1
        locks = rel if family=='relative' else ab if family=='absolute' else conjunction(rel,ab)
        policy=conjunction(a,locks)
        ms=verify(rel,verify(ab,a)) if family=='both-clocks' else verify(locks,a)
        text='Alice must sign, and '+ (f'the output must be at least {age} old.' if family=='relative' else
             f'the spending transaction must have {locktext}.' if family=='absolute' else
             f'the output must be at least {age} old AND the spending transaction must have {locktext}. Both requirements apply.')
        note='Check each required lock before the signature. CSV checks input sequence; CLTV checks transaction locktime. Keep the requested units and values.'
    elif family in ['relative-recovery','absolute-recovery']:
        count=2; lock=rel if family=='relative-recovery' else ab
        policy,ms=alternative(a,conjunction(b,lock)),branch(a,verify(lock,b))
        condition=f'the output is at least {age} old' if family=='relative-recovery' else f'the spending transaction has {locktext}'
        text=f'Alice can spend immediately. Bob can spend independently when {condition}. Alice keeps her immediate route after recovery becomes available.'
        note='Put the recovery lock and recovery signature together in one branch. Leave the immediate branch unrestricted.'
    elif family=='hash-refund':
        count=2
        policy,ms=alternative(conjunction(a,secret),conjunction(b,ab)),branch(verify(a,secret),verify(ab,b))
        text=f'Alice can claim with her signature and a 32-byte preimage of SHA-256 $sha256. Bob can refund with his signature when the spending transaction has {locktext}. Alice can still claim after that.'
        note='The claim branch checks both the signature and the preimage. The refund branch checks the absolute lock and its own signature.'
    elif family=='delayed-choice':
        count=3
        policy,ms=conjunction(rel,alternative(conjunction(a,b),c)),verify(rel,branch(verify(a,b),c))
        text=f'Everyone must wait until the output is at least {age} old. After that, Alice and Bob together may spend, or Carol may spend alone.'
        note='Place the shared relative lock before the branch. One branch requires both cosigners; the other requires the recovery signer.'
    elif family=='guarded-hash-recovery':
        count=3
        policy=conjunction(a,alternative(conjunction(b,secret),conjunction(c,rel)))
        ms=verify(a,branch(verify(b,secret),verify(rel,c)))
        text=f'Alice must sign every spend. She also needs either Bob with a 32-byte preimage of SHA-256 $sha256, or Carol when the output is at least {age} old. Both routes still require Alice.'
        note='Keep the mandatory signer outside the two alternatives. Each alternative carries its own additional evidence.'
    elif family=='two-conditioned-teams':
        count=6; team='thresh(2,pk($3),pk($4),pk($5))'
        policy=alternative(conjunction(quorum,rel),conjunction(team,ab))
        ms=branch(verify(rel,multi),verify(ab,'multi(2,$3,$4,$5)'))
        text=f'Two of Alice, Bob, and Carol may spend when the output is at least {age} old. Separately, two of Dave, Erin, and Frank may spend when the transaction has {locktext}. Either complete route suffices.'
        note='Use one branch for each team and its own lock. Do not exchange signatures or clock requirements between branches.'
    elif family=='three-alternatives':
        count=5; team='thresh(2,pk($1),pk($2),pk($3))'
        policy=alternative(a,alternative(conjunction(team,rel),conjunction(pk(4),secret)))
        ms=branch(a,branch(verify(rel,'multi(2,$1,$2,$3)'),verify(pk(4),secret)))
        text=f'Alice may spend alone immediately. Or two of Bob, Carol, and Dave may spend when the output is at least {age} old. Or Erin may spend with her signature and a 32-byte preimage of SHA-256 $sha256. Each route is independent.'
        note='Represent each of the three complete alternatives as a branch. Preserve the separate lock and preimage requirements.'
    elif family=='shared-secret':
        count=4
        policy=conjunction(secret,alternative(conjunction(a,b),conjunction(conjunction(c,d),rel)))
        ms=verify(secret,branch(verify(a,b),verify(rel,verify(c,d))))
        text=f'Every spend needs a 32-byte preimage of SHA-256 $sha256. With that secret, Alice and Bob can sign immediately. Alternatively, Carol and Dave can both sign when the output is at least {age} old.'
        note='Check the common preimage before choosing a signing route. Only the second signing route requires the relative lock.'
    else:raise ValueError(family)
    return dict(keys=names[:count],policy=policy,miniscript=ms,text=text,note=note)


def rows(path):return [json.loads(l) for l in Path(path).read_text().splitlines()]


def digest(path):
    with Path(path).open('rb') as f:return hashlib.file_digest(f,'sha256').hexdigest()


def save(path,obj):
    with Path(path).open('x') as f:json.dump(obj,f,indent=2);f.write('\n')


def save_rows(path,data):
    with Path(path).open('x') as f:
        for row in data:f.write(json.dumps(row)+'\n')


def run(command,log,data=None):
    result=subprocess.run(command,input=data,text=True,capture_output=True)
    Path(log).write_text(result.stdout+result.stderr)
    result.check_returncode()
    return [json.loads(l) for l in result.stdout.splitlines()] if data is not None else None


def check_targets(fixtures,root,name,expressions=None):
    inputs=[dict(fixture=f,**({'miniscript':expressions[i]} if expressions else {})) for i,f in enumerate(fixtures)]
    targets=run(['target/release/examples/compile_script_targets'],root/(name+'-targets.log'),
                ''.join(json.dumps(r)+'\n' for r in inputs))
    assert len(targets)==len(fixtures)
    save_rows(root/(name+'-targets.jsonl'),targets)
    return targets


def write_pool(path,fixtures,groups,base_manifest,purpose):
    path.mkdir()
    save_rows(path/'fixtures.jsonl',fixtures)
    manifest=dict(base_manifest,suite=path.name,counts={'t1':len(fixtures)},purpose=purpose,
                  fixtures_sha256=digest(path/'fixtures.jsonl'))
    save(path/'manifest.json',manifest);save(path/'groups.json',groups)


def compile_catalog(root,split,families,variants):
    cases,notes,expressions=[],{},{}
    for family in families:
        for variant in range(variants):
            c=contract(family,variant)
            prefix='composed-train-' if split=='training' else 'composed-val-'
            group=f'{prefix}{NAME}-{family}-{variant}'
            for context,label in CONTEXTS.items():
                ident=group+'-'+context
                cases.append(dict(id=ident,group=group,kind='write',context=context,choose_context=False,
                    keys=c['keys'],tier='medium',policy=c['policy'],
                    prompt=f'Write a {label} for these spending requirements. '+c['text']+
                    ' Allow exactly these routes. Return the complete script in ASM.'))
                notes['t1-human-'+ident]=c['note']
                expressions['t1-human-'+ident]=c['miniscript'].replace('multi(','multi_a(') if context=='tap' else c['miniscript']
    source=root/(split+'-catalog.json')
    save(source,dict(generator='composed-training-v1' if split=='training' else 'compound-validation-v1',
        purpose='training' if split=='training' else 'validation',seed=SEED,suite=NAME+'-'+split,
        exclusions={},cases=cases,evaluation_role='Fixed writing evaluation; no checkpoint selection.'))
    base=root/(split+'-base')
    tool='compile_training_catalog' if split=='training' else 'compile_validation_catalog'
    run(['target/release/examples/'+tool,str(source),str(base)],root/(split+'-compile.log'))
    fixtures=rows(base/'fixtures.jsonl')
    expanded=[]
    for f in fixtures:
        values={str(i):k['pubkey'] for i,k in enumerate(f['keys'])}
        for algo in ['sha256','hash160']:
            found=re.search(algo+r'\(([0-9a-f]+)\)',f['reference_policy'])
            if found:values[algo]=found[1]
        expanded.append(re.sub(r'\$(sha256|hash160|\d+)',lambda m:values[m[1]],expressions[f['id']]))
    targets=check_targets(fixtures,root,split,expanded)
    for f,t in zip(fixtures,targets,strict=True):
        assert f['id']==t['id']
        f.update(reference_miniscript=t['miniscript'],reference_script_hex=t['hex'])
    groups=json.loads((base/'groups.json').read_text())
    pool=Path('datasets')/(NAME+'-'+split)
    write_pool(pool,fixtures,groups,json.loads((base/'manifest.json').read_text()),
               'Writing-only training' if split=='training' else 'Writing-only reserved evaluation')
    return fixtures,targets,notes,pool


def main():
    root=Path('runs')/(NAME+'-preparation');root.mkdir()
    from transformers import AutoTokenizer
    tokenizer=AutoTokenizer.from_pretrained(PARENT)
    new,targets,notes,train_pool=compile_catalog(root,'training',FAMILIES,8)
    reserved,_,_,reserved_pool=compile_catalog(root,'reserved',RESERVED,3)
    reserved_shapes={shape(f['reference_policy']) for f in reserved}
    assert not reserved_shapes & {shape(f['reference_policy']) for f in new}
    sources=['datasets/broad-v1-training/fixtures.jsonl','datasets/compound-v3-train/fixtures.jsonl']
    rng=random.Random(SEED)
    replay=[];provenance=[]
    for source,count in zip(sources,[112,112],strict=True):
        candidates=[f for f in rows(source) if f['task']=='write' and shape(f['reference_policy']) not in reserved_shapes]
        chosen=rng.sample(candidates,count)
        replay.extend(chosen)
        provenance.extend(dict(task_id=f['id'],source=source) for f in chosen)
    replay_targets=check_targets(replay,root,'replay')
    fixtures=new+replay; all_targets=targets+replay_targets
    assert len(fixtures)==512
    pairs=[]
    for f,t in zip(fixtures,all_targets,strict=True):
        assert f['task']=='write' and f['id']==t['id']
        prompt=tokenizer.apply_chat_template([dict(role='system',content='You are a helpful assistant.'),
            dict(role='user',content=f['request'])],add_generation_prompt=True,enable_thinking=True,tokenize=False)
        trace=f"Target: {t['context_label']}.\n"
        if f['id'] in notes:trace+=notes[f['id']]+'\n'
        trace+='As a policy that is: '+f['reference_policy']+'\nIn Miniscript: '+t['miniscript']+'\n'
        completion=('' if prompt.rstrip().endswith('<think>') else '<think>\n')+trace+'</think>\n\n'+t['final_chat']+'<|im_end|>'
        pairs.append(dict(prompt=prompt,completion=completion,split='training',task_id=f['id'],interface='chat',
                          task_kind='write',context=f['context'],source='new' if f['id'] in notes else 'replay'))
    rng.shuffle(pairs)
    masks=prepare_replay(pairs,tokenizer)
    data=Path('datasets')/('sft-'+NAME+'.jsonl');save_rows(data,pairs)
    save(root/'replay-provenance.json',provenance)
    # Reuse only the writing questions of human-v2, with original IDs and requests.
    human_source=Path('datasets/human-v2')
    human=[f for f in rows(human_source/'fixtures.jsonl') if f['task']=='write']
    groups=json.loads((human_source/'groups.json').read_text())
    human_pool=Path('datasets/human-v2-writing')
    if not human_pool.exists():
        manifest=json.loads((human_source/'manifest.json').read_text());manifest['evaluation_only']=True
        write_pool(human_pool,human,{f['id']:groups[f['id']] for f in human},manifest,'Observed human-v2 writing development subset')
    assert rows(human_pool/'fixtures.jsonl')==human and len(human)==120
    # New requests and IDs must differ from every earlier dataset.
    exclusions={}
    ids={f['id'] for f in new+reserved};requests={f['request'] for f in new+reserved}
    for path in sorted(Path('datasets').glob('*/fixtures.jsonl')):
        if path.parent in [train_pool,reserved_pool]:continue
        older=rows(path)
        assert not ids & {f['id'] for f in older}
        assert not requests & {f.get('request') for f in older}
        exclusions[str(path)]=digest(path)
    # Keep the complete training fixture set for later fit diagnostics.
    all_pool=Path('datasets')/(NAME+'-mixed')
    manifest=json.loads((train_pool/'manifest.json').read_text())
    all_groups=json.loads((train_pool/'groups.json').read_text())
    for source in sources:
        source_groups=json.loads((Path(source).parent/'groups.json').read_text())
        for f in replay:
            if f['id'] in source_groups:all_groups[f['id']]=source_groups[f['id']]
    assert set(all_groups)=={f['id'] for f in fixtures}
    write_pool(all_pool,fixtures,all_groups,manifest,'Writing-only SFT fixture provenance')
    report=dict(name=NAME,parent=PARENT,data=str(data),data_sha256=digest(data),rows=512,new_questions=288,
        new_scenario_groups=96,replay_questions=224,interfaces=['chat'],task_kinds=['write'],
        training_context_counts={c:sum(f['context']==c for f in fixtures) for c in CONTEXTS},
        reserved_questions=36,reserved_scenario_groups=12,reserved_families=RESERVED,
        reserved_shapes_absent_from_current_mix=True,historical_exclusions=exclusions,
        source_hashes={p:digest(p) for p in sources},max_tokens=max(len(r['input_ids']) for r in masks),
        supervised_tokens=sum(r['supervised_tokens'] for r in masks),
        fixture_hashes={str(p/'fixtures.jsonl'):digest(p/'fixtures.jsonl') for p in [train_pool,reserved_pool,all_pool,human_pool]},
        validation='All 512 training and 36 reserved targets passed strict context-specific equivalence, reference interpreter execution, ASM round trips and chat extraction. All 512 complete targets passed tokenization checks.',
        limits='Synthetic spending-policy writing tasks. Context variants share scenario groups. Reserved structures may occur in parent training. The verifier is limited to Miniscript; reference execution assumes valid signatures. No general signed-transaction or arbitrary-Script claim.')
    save(root/'verification.json',report)
    print(json.dumps({k:v for k,v in report.items() if k!='historical_exclusions'},indent=2))


if __name__=='__main__':main()
