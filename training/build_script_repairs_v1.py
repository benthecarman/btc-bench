"""Prepare raw Bitcoin Script examples for review, without training a model."""
import copy
import hashlib
import json
from pathlib import Path
import re
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT/'scripts'))
from rl_common import extract_answer
from rl_replay import prepare_replay
from sft_format import SUBMIT_SCRIPT, SYSTEM_PROMPT

CASES = [
    dict(name='both-signatures', keys=['Alice','Bob'],
         prompt='Can you write the Bitcoin script for our shared savings? Alice and Bob must both sign a withdrawal. Neither can spend alone, and there is no recovery route or waiting period.',
         policy='and(pk($0),pk($1))', miniscript='and_v(v:pk($0),pk($1))',
         bad_miniscript='or_i(pk($0),pk($1))',
         note='Both signatures are required. The first signature check must succeed before the second check runs; a branch that accepts either signature would permit an unauthorized withdrawal.'),
    dict(name='either-signature', keys=['Alice','Bob'],
         prompt='I want a Bitcoin script that lets either Alice or Bob spend the money on their own. They should not have to contact each other or wait. Nobody else can spend.',
         policy='or(pk($0),pk($1))', miniscript='or_i(pk($0),pk($1))',
         bad_miniscript='and_v(v:pk($0),pk($1))',
         note='Either signer is sufficient. The witness selects one branch and supplies the signature for that branch. Requiring both signatures would block the promised independent access.'),
    dict(name='delay-only-recovery', keys=['Alice','Bob'],
         prompt='Please write a Bitcoin script for my savings. Alice can spend at any time. If needed, Bob can recover the money once the output is at least 288 blocks old from confirmation, without Alice. Alice must still be able to spend after that. Those are the only options.',
         policy='or(pk($0),and(pk($1),older(288)))',
         miniscript='or_i(pk($0),and_v(v:older(288),pk($1)))',
         bad_miniscript='and_v(v:older(288),or_i(pk($0),pk($1)))',
         note='Only Bob must wait. Keep the relative lock inside the recovery branch so Alice remains unrestricted. For recovery, use a version-2-or-later transaction and a block-based input sequence of at least 288 with the relative-lock disable flag clear.'),
    dict(name='relative-not-absolute', keys=['Alice'],
         prompt='Write a Bitcoin script that lets Alice spend only after these coins have been confirmed for at least 144 blocks. The delay starts when this output confirms, not at a fixed block height. There are no other spending routes.',
         policy='and(pk($0),older(144))', miniscript='and_v(v:older(144),pk($0))',
         bad_miniscript='and_v(v:after(144),pk($0))',
         note='Use a relative lock, not an absolute block-height lock. CSV constrains this input sequence; CLTV checks transaction locktime. Use a version-2-or-later transaction and a block-based input sequence of at least 144 with the relative-lock disable flag clear.'),
    dict(name='signature-stack', keys=['Alice','Bob'],
         prompt='Could you write the Bitcoin script for a payment that needs both Alice and Bob to sign? Neither signature alone should work. There are no other conditions or spending paths.',
         policy='and(pk($0),pk($1))', miniscript='and_v(v:pk($0),pk($1))',
         replace_from='OP_CHECKSIGVERIFY', replace_to='OP_CHECKSIG',
         note='The first check must consume its Boolean result. CHECKSIGVERIFY does that and aborts if the signature is invalid. Two consecutive CHECKSIG operations would leave the first Boolean where the next signature is needed.'),
    dict(name='unlisted-key', keys=['Alice','Bob','Carol'],
         prompt='Write the Bitcoin script for a club reserve. Any two of Alice, Bob, and Carol can authorize a withdrawal. One signature is never enough, and there must be no spending path for any other key.',
         policy='thresh(2,pk($0),pk($1),pk($2))', miniscript='multi(2,$0,$1,$2)',
         bad_miniscript='or_i(multi(2,$0,$1,$2),pk($extra))',
         note='Remove the alternate branch for the unlisted key. The remaining script requires two of the three club signatures. Its CHECKMULTISIG witness includes the required empty dummy element before the signatures.'),
]


def rows(path):
    return list(map(json.loads, Path(path).read_text().splitlines()))


def write_json(path, obj):
    with Path(path).open('x') as f:
        json.dump(obj, f, indent=2); f.write('\n')


def write_rows(path, records):
    with Path(path).open('x') as f:
        for row in records:
            f.write(json.dumps(row)+'\n')


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def main():
    from transformers import AutoTokenizer
    base, pool = Path('datasets/script-repairs-v1-base'), Path('datasets/script-repairs-v1')
    out = Path('runs/script-repairs-v1')
    assert not any(p.exists() for p in [base,pool,out]), 'Preserve earlier review artifacts'
    assert Path('runs').resolve().is_relative_to('/mnt/llm-models')
    out.mkdir()
    catalog = dict(generator='composed-training-v1',purpose='training',suite='script-repairs-v1',seed=20260909,
        exclusions={},cases=[dict(id='composed-train-script-repairs-v1-'+c['name'],
            group='composed-train-script-repairs-v1-'+c['name'],kind='write',context='segwitv0',choose_context=True,
            keys=c['keys'],tier='medium',policy=c['policy'],prompt=c['prompt']) for c in CASES])
    catalog_path = Path('training/script-repairs-v1-catalog.json')
    write_json(catalog_path,catalog)
    with (out/'compile.log').open('x') as log:
        subprocess.run(['target/release/examples/compile_training_catalog',str(catalog_path),str(base)],
                       stdout=log,stderr=subprocess.STDOUT,check=True)
    originals = rows(base/'fixtures.jsonl')
    assert len(originals)==6
    by_name = {f['id'].removeprefix('t1-human-composed-train-script-repairs-v1-'):f for f in originals}
    extra = by_name['both-signatures']['keys'][0]['pubkey']
    inputs = []
    for c in CASES:
        fixture = by_name[c['name']]
        keymap = {str(i):k['pubkey'] for i,k in enumerate(fixture['keys'])}
        keymap['extra'] = extra
        if c['name']=='unlisted-key':
            assert extra not in [k['pubkey'] for k in fixture['keys']]
        expand = lambda s: re.sub(r'\$(extra|\d+)',lambda m:keymap[m[1]],s)
        row = dict(fixture=fixture,miniscript=expand(c['miniscript']),note=c['note'])
        if 'bad_miniscript' in c:
            row['bad_miniscript'] = expand(c['bad_miniscript'])
        else:
            row.update(replace_from=c['replace_from'],replace_to=c['replace_to'])
        inputs.append(row)
    write_rows(out/'teaching-inputs.jsonl',inputs)
    process = subprocess.run(['target/release/examples/compile_script_repairs'],
        input=''.join(json.dumps(r)+'\n' for r in inputs),text=True,capture_output=True)
    (out/'teaching-compile.log').write_text(process.stderr)
    process.check_returncode()
    targets = list(map(json.loads,process.stdout.splitlines()))
    assert len(targets)==6
    write_rows(out/'teaching-targets.jsonl',targets)
    trace_inputs = [dict(id=f['id'],reference_policy=f['reference_policy'],policy=f['reference_policy'],
                         miniscript=t['miniscript']) for f,t in zip([i['fixture'] for i in inputs],targets,strict=True)]
    checked = subprocess.run(['target/release/examples/check_trace_parts'],
        input=''.join(json.dumps(r)+'\n' for r in trace_inputs),text=True,capture_output=True,check=True)
    trace_results = list(map(json.loads,checked.stdout.splitlines()))
    assert len(trace_results)==6 and all(r[k]['status']=='equivalent' for r in trace_results for k in ['policy','miniscript'])
    write_json(out/'trace-checks.json',trace_results)
    tokenizer = AutoTokenizer.from_pretrained('runs/sft-broad-v1/merged-step528')
    fixtures, output, review, positive, negative, groups = [], [], [], [], [], {}
    for c,raw,target in zip(CASES,inputs,targets,strict=True):
        f = raw['fixture']
        assert target['id']==f['id']
        group = 'composed-train-script-repairs-v1-'+c['name']
        for task_kind in ['write','repair']:
            fixture = copy.deepcopy(f)
            fixture['id'] += '-'+task_kind
            fixture['reference_miniscript'] = target['miniscript']
            fixture['reference_script_hex'] = target['hex']
            if task_kind=='repair':
                fixture['request'] += '\nI tried the script below. Please fix it so it follows those rules:\n\n```text\n'+target['draft']+'\n```'
            fixtures.append(fixture); groups[fixture['id']] = group
            positive.append(dict(task_id=fixture['id'],answer=dict(task='script',script=target['asm'])))
            negative.append(dict(task_id=fixture['id'],answer=dict(task='script',script=target['draft'])))
            explanation = 'I choose a P2WSH witness script.\n'+c['note']+'\nAs a policy that is: '+f['reference_policy']+'\nIn Miniscript: '+target['miniscript']+'\n'
            for mode in ['chat','submit']:
                messages = [dict(role='system',content='You are a helpful assistant.' if mode=='chat' else SYSTEM_PROMPT),
                            dict(role='user',content=fixture['request'])]
                opts = dict(tools=[SUBMIT_SCRIPT]) if mode=='submit' else {}
                prompt = tokenizer.apply_chat_template(messages,**opts,add_generation_prompt=True,enable_thinking=True,tokenize=False)
                final = target['final_chat'] if mode=='chat' else '<tool_call>\n'+json.dumps(dict(name='submit_script',arguments=dict(script=target['asm'])))+'\n</tool_call>'
                completion = '<think>\n'+explanation+'</think>\n\n'+final+'<|im_end|>'
                if mode=='submit':
                    assert extract_answer(completion.removesuffix('<|im_end|>'),thinking=True)==dict(task='script',script=target['asm'])
                assert 'submit_descriptor' not in prompt+completion
                assert not re.search(r'\b(?:wsh|wpkh|tr)\(',completion)
                output.append(dict(prompt=prompt,completion=completion,split='training',task_id=fixture['id'],
                                   interface=mode,task_kind=task_kind,group=group))
            review.append(dict(id=fixture['id'],group=group,task_kind=task_kind,request=fixture['request'],
                keys=fixture['keys'],policy=f['reference_policy'],miniscript=target['miniscript'],script_asm=target['asm'],
                script_hex=target['hex'],draft=target['draft'] if task_kind=='repair' else None,
                explanation=c['note'],draft_failure=target['draft_reason']))
    seen_ids, seen_requests, seen_keys, compared = set(),set(),set(),[]
    for path in sorted(Path('datasets').glob('*/fixtures.jsonl')):
        if path.parent in [base,pool]:
            continue
        compared.append(str(path))
        for row in rows(path):
            seen_ids.add(row['id']); seen_requests.add(row.get('request'))
            seen_keys.update(k['pubkey'] for k in row.get('keys',[]) if 'pubkey' in k)
    assert not {f['id'] for f in fixtures} & seen_ids
    assert not {f['request'] for f in fixtures} & seen_requests
    assert not {k['pubkey'] for f in fixtures for k in f['keys']} & seen_keys
    pool.mkdir()
    write_rows(pool/'fixtures.jsonl',fixtures)
    manifest = json.loads((base/'manifest.json').read_text())
    manifest.pop('canary',None)
    manifest.update(fixtures_sha256=sha(pool/'fixtures.jsonl'),counts={'t1':len(fixtures)},
                    purpose='Raw-script training examples for human review')
    write_json(pool/'manifest.json',manifest)
    write_json(pool/'groups.json',groups)
    write_json(pool/'source.json',catalog)
    for name,responses in [('positive',positive),('negative',negative)]:
        path = out/(name+'.jsonl'); write_rows(path,responses)
        with (out/(name+'-grade.log')).open('x') as log:
            subprocess.run(['target/release/btc-bench','grade','--dataset',str(pool),'--responses',str(path),
                '--out',str(out/(name+'-graded')),'--standard-mode'],stdout=log,stderr=subprocess.STDOUT,check=True)
        grades = json.loads((out/(name+'-graded')/'results.json').read_text())
        assert len(grades)==12 and all(r['score']==(1 if name=='positive' else 0) for r in grades)
    prepared = prepare_replay(output,tokenizer)
    write_json(out/'loss-mask-audit.json',prepared)
    data = Path('datasets/sft-script-repairs-v1.jsonl'); write_rows(data,output)
    write_json('training/script-repairs-v1-review.json',review)
    report = dict(scenarios=6,questions=12,sft_rows=24,positive_grades=12,bad_drafts_rejected=12,
        independent_trace_checks=6,asm_hex_round_trips=6,execution_cross_checks=6,runner_chat_extraction_checks=6,
        max_tokens=max(len(r['input_ids']) for r in prepared),data=str(data),data_sha256=sha(data),
        fixture_sha256=sha(pool/'fixtures.jsonl'),exact_overlap=0,compared_pools=compared,
        status='Prepared for review; no model training',
        scope='All tasks request raw Bitcoin Script. Context is left open in requests; teaching targets choose Segwit v0/P2WSH. '
              'No claim of transfer to other contexts. Familiar structures recur deliberately. Keep paired write/repair and chat/submit forms in one split. '
              'Execution checks use the Miniscript interpreter with assumed-valid signatures, not signed transactions in Bitcoin Core.')
    write_json('training/script-repairs-v1-verification.json',report)
    sections = ['Six raw Bitcoin Script scenarios, each as a normal request and a repair. All have chat and submit_script targets. '
                'The policy and Miniscript form the checked explanation; the final answer is raw script ASM, with matching hex saved in the review JSON.\n',
                'Prompts say Bitcoin script and let the model choose its context. These targets choose P2WSH. All paired variants belong to the same training group. '
                'The earlier wallet descriptor repair examples are a separate, shelved experiment and are not included.\n',
                '**Reading the previews:** A, B and C below stand for the supplied compressed public keys, in prompt order. '
                'They are display abbreviations only; every actual training prompt, draft and answer contains full public-key hex.\n']
    for c,target in zip(CASES,targets,strict=True):
        r = next(r for r in review if r['task_kind']=='repair' and r['id'].endswith(c['name']+'-repair'))
        def short(text):
            for i,key in enumerate(r['keys']):
                text = text.replace(key['pubkey'],chr(ord('A')+i))
            return text.replace(extra,'UNLISTED_KEY') if c['name']=='unlisted-key' else text
        sections += [f"**{c['name']}**\n\n{c['prompt']}\n", 'Keys: '+', '.join(chr(ord('A')+i)+' = '+k['label'] for i,k in enumerate(r['keys']))+'.\n',
            'Faulty draft:\n\n```text\n'+short(target['draft'])+'\n```\n',
            c['note']+'\n', 'Policy:\n\n```text\n'+short(r['policy'])+'\n```\n',
            'Miniscript:\n\n```text\n'+short(r['miniscript'])+'\n```\n',
            'Correct raw script (ASM):\n\n```text\n'+short(target['asm'])+'\n```\n']
    sections += ['Verification: 12 correct answers pass strict grading; all 12 faulty drafts fail. Six independent policy/body checks, '
                 'six ASM/hex round trips and six execution cross-checks pass. All 24 SFT completion masks and tokenizer round trips pass.\n',
                 'These are authored training examples, not an independent benchmark. The interpreter checks assume valid signatures; '
                 'they do not establish full signed-transaction correctness.\n',
                 'Relative lock behavior follows [BIP-112](https://github.com/bitcoin/bips/blob/master/bip-0112.mediawiki). '
                 'The full keys, prompts, drafts, policies, Miniscript, ASM and hex are in `script-repairs-v1-review.json`.\n']
    with Path('training/script-repairs-v1-examples.md').open('x') as stream:
        stream.write('\n'.join(sections))
    print(json.dumps({k:v for k,v in report.items() if k!='compared_pools'},indent=2))


if __name__=='__main__':
    main()
