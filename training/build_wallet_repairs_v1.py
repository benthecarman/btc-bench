"""Build a review set of fresh requests and single-fault wallet repairs.

Uses new test keys and authored requests. This is training material based on
observed failure categories, not an independent evaluation set.
"""
import copy
import hashlib
import json
from pathlib import Path
import re
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / 'scripts'))
from rl_common import extract_task_answer
from rl_replay import prepare_replay
from run_wallet_bench import request_body
from check_wallet_sft_v1_traces import check, expand


CASES = [
    dict(name='one-derivation', keys=['Nina'],
         request='I am setting up a native SegWit account for Nina. She alone can spend, with no delay or recovery route.',
         policy='pk(@0)', template='wpkh(@0/**)', bodies=[],
         bad='wpkh(@0/**/**)',
         note='A template key needs one receive/change derivation suffix. The shorthand /** already means /<0;1>/*. Do not append a second suffix.',
         concrete_note='These public keys are already derived. Use the supplied key directly, without a wildcard suffix.'),
    dict(name='no-comment-suffix', keys=['Omar'],
         request='Make a Taproot account for Omar. His key is the only spending route, and he can spend at any time.',
         policy='pk(@0)', template='tr(@0/**)', bodies=[],
         bad='tr(@0/**)#Taproot',
         note='A # suffix is not a free-form label. Return the descriptor without the invented #Taproot suffix; no script tree is needed.'),
    dict(name='single-leaf', keys=['Pia', 'Quinn', 'Ravi', 'Suri'],
         request='I need a Taproot reserve. Ravi can spend alone whenever needed. Alternatively, any two of Pia, Quinn, and Suri can spend together without Ravi. Those are the only routes.',
         policy='or(pk(@2),thresh(2,pk(@0),pk(@1),pk(@3)))',
         template='tr(@2/**,multi_a(2,@0/**,@1/**,@3/**))',
         bodies=['multi_a(2,@0/**,@1/**,@3/**)'],
         bad='tr(@2/**,{multi_a(2,@0/**,@1/**,@3/**)})',
         note='The team is one script leaf. Place it directly after the internal key. Braces join two tree branches; they do not wrap a single leaf.'),
    dict(name='every-template-key', keys=['Tess', 'Uma'],
         request='For my Taproot savings account, Uma can spend at any time. Tess can recover the money once the output has aged at least 720 blocks since confirmation, without Uma. Keep both routes available after that.',
         policy='or(pk(@1),and(pk(@0),older(720)))',
         template='tr(@1/**,and_v(v:pk(@0/**),older(720)))',
         bodies=['and_v(v:pk(@0/**),older(720))'],
         bad='tr(@1/**,and_v(v:pk(@0),older(720)))',
         note='The recovery key needs its receive/change derivation suffix too. The internal key and the leaf key are both template keys. The 720-block delay applies only to Tess.',
         concrete_note='Use the supplied public keys throughout the concrete descriptor. A template placeholder cannot stand in for the recovery public key.'),
    dict(name='and-v-type', keys=['Vera', 'Will'],
         request='Create a native SegWit script account that requires both Vera and Will to sign. Neither can spend alone. There is no time restriction or alternate route.',
         policy='and(pk(@0),pk(@1))', template='wsh(multi(2,@0/**,@1/**))',
         solution='wsh(and_v(v:pk(@0/**),pk(@1/**)))',
         bodies=['and_v(v:pk(@0/**),pk(@1/**))'],
         bad='wsh(and_v(pk(@0/**),pk(@1/**)))',
         note='and_v requires a V-type first argument. The v: wrapper verifies the first signature result; the second pk remains the final Boolean result.'),
    dict(name='or-b-type', keys=['Xena', 'Yuri'],
         request='I want a native SegWit script account where either Xena or Yuri can spend alone. There is no waiting period and neither needs the other signature.',
         policy='or(pk(@0),pk(@1))', template='wsh(multi(1,@0/**,@1/**))',
         solution='wsh(or_b(pk(@0/**),s:pk(@1/**)))', bodies=['or_b(pk(@0/**),s:pk(@1/**))'],
         bad='wsh(or_b(pk(@0/**),pk(@1/**)))',
         note='or_b requires B and W arguments. The s: wrapper turns the second pk into the required W form. Either signature is still sufficient.'),
    dict(name='separate-councils', keys=['Asha', 'Bea', 'Chen', 'Dale', 'Enid', 'Faye'],
         request='Set up a Taproot reserve with Faye as the owner. Faye can spend alone at any time. Without Faye, a withdrawal needs at least one of Asha, Bea, and Chen AND both Dale and Enid. Extra signatures from the first group cannot replace Dale or Enid. Allow exactly these routes.',
         policy='or(pk(@5),and(thresh(1,pk(@0),pk(@1),pk(@2)),and(pk(@3),pk(@4))))',
         template='tr(@5/**,and_v(v:multi_a(1,@0/**,@1/**,@2/**),multi_a(2,@3/**,@4/**)))',
         bodies=['and_v(v:multi_a(1,@0/**,@1/**,@2/**),multi_a(2,@3/**,@4/**))'],
         bad='tr(@5/**,multi_a(3,@0/**,@1/**,@2/**,@3/**,@4/**))',
         note='Keep the two group requirements separate. A single three-of-five threshold would let Asha, Bea, and Chen spend without Dale or Enid.'),
    dict(name='independent-routes', keys=['Gita', 'Hale', 'Imani', 'Jules', 'Kian'],
         request='Build a Taproot wallet with three ways to spend. Gita can spend alone immediately. Any two of Hale, Imani, and Jules can also spend immediately without Gita. Separately, Kian can recover the money once the output is at least 576 blocks old from confirmation, without anyone else. Each route is independent; permit no others.',
         policy='or(pk(@0),or(thresh(2,pk(@1),pk(@2),pk(@3)),and(pk(@4),older(576))))',
         template='tr(@0/**,{multi_a(2,@1/**,@2/**,@3/**),and_v(v:pk(@4/**),older(576))})',
         bodies=['multi_a(2,@1/**,@2/**,@3/**)', 'and_v(v:pk(@4/**),older(576))'],
         bad='tr(@0/**,and_v(v:multi_a(2,@1/**,@2/**,@3/**),and_v(v:pk(@4/**),older(576))))',
         note='Use two alternative script leaves. Joining the team and recovery conditions with and_v would require both, so neither promised independent route would work.'),
]


def write_json(path, value):
    with Path(path).open('x') as stream:
        json.dump(value, stream, indent=2)
        stream.write('\n')


def write_rows(path, rows):
    with Path(path).open('x') as stream:
        for row in rows:
            stream.write(json.dumps(row)+'\n')


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def run(command, log):
    with log.open('x') as stream:
        subprocess.run(command, stdout=stream, stderr=subprocess.STDOUT, check=True)


def main():
    from transformers import AutoTokenizer
    pool = Path('datasets/wallet-repairs-v1')
    base = Path('datasets/wallet-repairs-v1-base')
    out = Path('runs/wallet-repairs-v1')
    assert not any(p.exists() for p in [pool, base, out]), 'Preserve earlier review artifacts'
    assert Path('runs').resolve().is_relative_to('/mnt/llm-models')
    out.mkdir()
    catalog = dict(suite='wallet-repairs-v1', evaluation_only=False, cases=[
        dict(id='wrp1-'+c['name'], family=c['name'], group='wrp1-'+c['name'],
             request=c['request'], keys=c['keys'], template=c['template'], policy=c['policy']) for c in CASES])
    catalog_path = Path('training/wallet-repairs-v1-catalog.json')
    write_json(catalog_path, catalog)
    run(['target/release/btc-wallet-bench', 'build-training', '--catalog', str(catalog_path), '--out', str(base)], out/'build.log')
    originals = list(map(json.loads, (base/'fixtures.jsonl').read_text().splitlines()))
    assert len(originals) == 16
    tokenizer = AutoTokenizer.from_pretrained('runs/sft-wallet-v1/merged-step128')
    fixtures, rows, review, checks = [], [], [], []
    positives, negatives, chats = [], [], []
    for original in originals:
        case = next(c for c in CASES if c['name'] == original['family'])
        template = original['output_kind'] == 'template'
        solution = case.get('solution', case['template'])
        bad = case['bad']
        note = case['note']
        if not template:
            solution = expand(original, solution)
            bad = expand(original, bad)
            if case['name'] == 'every-template-key':
                # Concrete-key error is the reverse of the template suffix error.
                bad = solution.replace(expand(original, '@0'), '@0/**')
            note = case.get('concrete_note', note)
        policy = case['policy'] if template else expand(original, case['policy'])
        bodies = case['bodies'] if template else [expand(original, b) for b in case['bodies']]
        context = 'Taproot leaf Miniscript' if solution.startswith('tr(') else 'P2WSH descriptor body'
        for task_kind in ['write', 'repair']:
            fixture = copy.deepcopy(original)
            fixture['id'] += '-'+task_kind
            if task_kind == 'repair':
                fixture['request'] += '\nI tried this, but it may be wrong. Please fix it to match the requirements above:\n\n```text\n'+bad+'\n```'
            fixtures.append(fixture)
            explanation = (note+'\n' if task_kind == 'repair' else '')
            explanation += 'As a policy that is: '+policy+'\n'
            if bodies:
                explanation += context+':\n'+'\n'.join(bodies)+'\n'
            else:
                explanation += ('Use P2WPKH for the single signature; there is no separate Miniscript witness script.\n'
                                if solution.startswith('wpkh(') else 'Use the sole owner as the Taproot internal key; there are no script leaves.\n')
            # Audit the stated expressions at every checked derivation point.
            for pos in range(4 if template else 1):
                audit_fixture = copy.deepcopy(fixture)
                audit_fixture['derivations'] = original['derivations'][pos:]+original['derivations'][:pos]
                checks.append(dict(id=fixture['id']+f'/derivation-{pos}', fixture=audit_fixture,
                    policy=expand(audit_fixture, policy), bodies=[expand(audit_fixture,b) for b in bodies]))
            label = 'Descriptor template' if template else 'Concrete descriptor'
            # Put the brief repair explanation outside the code fence too.
            final = (note+'\n\n' if task_kind == 'repair' else '')+label+':\n\n```text\n'+solution+'\n```'
            positives.append(dict(task_id=fixture['id'], answer=solution))
            negatives.append(dict(task_id=fixture['id'], answer=bad))
            chats.append(dict(task_id=fixture['id'], text=final))
            for interface in ['chat','submit']:
                body = request_body(fixture, 'unused', interface)
                opts = {'tools':body['tools']} if interface == 'submit' else {}
                prompt = tokenizer.apply_chat_template(body['messages'], **opts, add_generation_prompt=True,
                                                       enable_thinking=True, tokenize=False)
                answer = final if interface == 'chat' else '<tool_call>\n'+json.dumps(dict(name='submit_descriptor',arguments=dict(descriptor=solution)))+'\n</tool_call>'
                completion = '<think>\n'+explanation+'</think>\n\n'+answer+'<|im_end|>'
                if interface == 'submit':
                    parsed = extract_task_answer(completion.removesuffix('<|im_end|>'),
                        {'task':'wallet','fixture':fixture}, thinking=True)
                    assert parsed == dict(task='descriptor',descriptor=solution)
                rows.append(dict(prompt=prompt, completion=completion, split='training', task_id=fixture['id'],
                                 interface=interface, group=fixture['group'], output_kind=fixture['output_kind'], task_kind=task_kind))
            review.append(dict(id=fixture['id'], group=fixture['group'], request=fixture['request'],
                task_kind=task_kind, output_kind=fixture['output_kind'], draft=bad if task_kind=='repair' else None,
                policy=policy, miniscript=bodies, solution=solution, explanation=note))
    # Exact separation checks do not assert structural novelty for repair data.
    old_ids, old_requests, old_keys = set(), set(), set()
    for path in [Path('datasets/wallet-policy-v1/fixtures.jsonl'), Path('datasets/wallet-sft-v1-training/fixtures.jsonl'),
                 Path('datasets/wallet-sft-v1-reserved/fixtures.jsonl')]:
        for row in map(json.loads, path.read_text().splitlines()):
            old_ids.add(row['id']); old_requests.add(row['request'])
            old_keys.update(k['key_info'] for k in row['keys'])
    assert not {f['id'] for f in fixtures} & old_ids
    assert not {f['request'] for f in fixtures} & old_requests
    assert not {k['key_info'] for f in fixtures for k in f['keys']} & old_keys
    pool.mkdir()
    write_rows(pool/'fixtures.jsonl', fixtures)
    manifest = json.loads((base/'manifest.json').read_text())
    manifest.update(questions=len(fixtures), fixtures_sha256=sha(pool/'fixtures.jsonl'),
                    purpose='Training review set: paired fresh requests and repairs, template and concrete output')
    write_json(pool/'manifest.json', manifest)
    write_json(pool/'groups.json', {c['group']:[f['id'] for f in fixtures if f['group']==c['group']] for c in catalog['cases']})
    write_json(pool/'source.json', catalog)
    for name, responses in [('positive',positives), ('negative',negatives), ('chat',chats)]:
        path = out/(name+'.jsonl')
        write_rows(path, responses)
        run(['target/release/btc-wallet-bench','grade-training','--dataset',str(pool),'--responses',str(path),
             '--out',str(out/(name+'-graded'))], out/(name+'-grade.log'))
        grades = json.loads((out/(name+'-graded')/'results.json').read_text())
        assert len(grades)==32 and all(r['score']==(0 if name=='negative' else 1) for r in grades), (name,grades)
    checked = check(checks)
    assert all(r['policy']['status']=='equivalent' and r['bodies']['status'] in
               ['equivalent with reference internal key','not applicable'] for r in checked)
    write_json(out/'trace-checks.json', checked)
    prepared = prepare_replay(rows, tokenizer)
    write_json(out/'loss-mask-audit.json', prepared)
    data = Path('datasets/sft-wallet-repairs-v1.jsonl')
    write_rows(data, rows)
    write_json('training/wallet-repairs-v1-review.json', review)
    report = dict(scenarios=8, questions=32, sft_rows=64, reference_passes=32, chat_passes=32,
                  bad_drafts_rejected=32, trace_checks=len(checked), max_tokens=max(len(r['input_ids']) for r in prepared),
                  data=str(data), data_sha256=sha(data), fixture_sha256=sha(pool/'fixtures.jsonl'),
                  status='Prepared for human spot check; no training run started',
                  scope='Training material based on observed failure categories. New exact requests and keys; '
                        'familiar policy structures intentionally recur. Keep all four questions per scenario in one split.',
                  sources=['https://github.com/bitcoin/bips/blob/master/bip-0388.mediawiki',
                           'https://bitcoin.sipa.be/miniscript/'])
    write_json('training/wallet-repairs-v1-verification.json', report)
    sections = ['Eight authored training scenarios, each paired as a fresh request and a repair, with template and concrete-key output. '
                'Both chat and submit SFT rows are prepared. No training has started.\n',
                'All correct targets and final chat responses pass the wallet verifier. Every bad draft earns zero. '
                'Explicit policies and Miniscript bodies are checked separately. Human review must still check that the requests express the intended contract.\n',
                'These are targeted training examples, not a new transfer benchmark. All derivatives of each scenario share a group.\n']
    for r in review:
        if r['output_kind']!='template' or r['task_kind']!='repair':
            continue
        sections += [f"**{r['id']}**\n\n{r['request']}\n", f"Correction: {r['explanation']}\n",
                     f"Spending policy:\n\n```text\n{r['policy']}\n```\n",
                     'Miniscript'+(' (no script body is needed for this output).' if not r['miniscript'] else ':\n\n```text\n'+'\n'.join(r['miniscript'])+'\n```')+'\n',
                     f"Correct descriptor:\n\n```text\n{r['solution']}\n```\n"]
    sections += ['Reference syntax: [BIP-388](https://github.com/bitcoin/bips/blob/master/bip-0388.mediawiki) and '
                 '[Miniscript](https://bitcoin.sipa.be/miniscript/). Full template/concrete requests are in `wallet-repairs-v1-review.json`.\n']
    with Path('training/wallet-repairs-v1-examples.md').open('x') as stream:
        stream.write('\n'.join(sections))
    print(json.dumps(report,indent=2))


if __name__=='__main__':
    main()
