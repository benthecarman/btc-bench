"""Prepare a fixed construction/repair SFT pass and separate composition checks."""
import argparse
import copy
import hashlib
import json
from pathlib import Path
import random
import re
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / 'scripts'))
from audit_human_catalog import shape
from rl_replay import prepare_replay
from sft_format import SUBMIT_SCRIPT, SYSTEM_PROMPT

NAME = 'script-construction-v1'
PARENT = 'runs/sft-broad-v1/merged-step528'
SEED = 20260911


def case(family, n):
    delay = [18, 36, 72, 144, 216, 288, 432, 576][n]
    height = 820000 + 17 * n
    k = lambda i: f'pk(${i})'
    andp = lambda a, b: f'and({a},{b})'
    orp = lambda a, b: f'or({a},{b})'
    andm = lambda a, b: f'and_v(v:{a},{b})'
    orm = lambda a, b: f'or_i({a},{b})'
    a, b, c = [k(i) for i in range(3)]
    lock, absolute = f'older({delay})', f'after({height})'
    quorum = 'thresh(2,pk($0),pk($1),pk($2))'
    multi = 'multi(2,$0,$1,$2)'
    secret = 'sha256($sha256)'
    names = ['Alice', 'Bob', 'Carol', 'Dave', 'Erin', 'Frank']
    if family == 'both-signatures':
        count, policy, ms, bad = 2, andp(a,b), andm(a,b), orm(a,b)
        text = 'Alice and Bob must both sign. Neither may spend alone.'
        note = 'Verify the first signature before checking the second. CHECKSIGVERIFY consumes the first result; two plain CHECKSIG operations do not form an AND.'
    elif family == 'either-signature':
        count, policy, ms, bad = 2, orp(a,b), orm(a,b), andm(a,b)
        text = 'Alice or Bob may spend independently. One signature is enough.'
        note = 'Use two alternative branches. The witness supplies a signature below the branch selector. Requiring both signatures would remove independent access.'
    elif family == 'recovery':
        count, policy, ms = 2, orp(a,andp(b,lock)), orm(a,andm(lock,b))
        bad = andm(lock,orm(a,b))
        text = f'Alice may spend at any time. Bob may spend without Alice once the output is at least {delay} blocks old from confirmation. Alice keeps access after that.'
        note = 'Place CSV only in the recovery branch. The delay must not restrict the immediate signer. The recovery transaction needs version 2 or later and a compatible block-based input sequence.'
    elif family == 'relative-lock':
        count, policy, ms, bad = 1, andp(a,lock), andm(lock,a), andm(f'after({delay})',a)
        text = f'Alice must sign, and the output must be at least {delay} blocks old from confirmation. This is an age requirement, not a fixed block height.'
        note = 'Use CSV for the input age. Use a block-based sequence with the disable flag clear and transaction version 2 or later. CLTV checks a different transaction field.'
    elif family == 'quorum':
        count, policy, ms, bad = 3, quorum, multi, 'multi(1,$0,$1,$2)'
        text = 'Any two of Alice, Bob, and Carol must sign. One signature is insufficient.'
        note = 'The threshold is two of three. In P2WSH, CHECKMULTISIG consumes an empty dummy item followed by signatures in public-key order.'
    elif family == 'hash-lock':
        count, policy, ms, bad = 1, andp(a,secret), andm(a,secret), a
        text = 'Alice must sign and supply a 32-byte preimage of SHA-256 $sha256.'
        note = 'Both the signature and preimage checks are required. The hash fragment checks the preimage length as well as its SHA-256 digest. A signature alone must fail.'
    elif family == 'hash-refund':
        count, policy, ms = 2, orp(andp(a,secret),andp(b,lock)), orm(andm(a,secret),andm(lock,b))
        bad = orm(a,andm(lock,b))
        text = f'Alice may claim with her signature and a 32-byte preimage of SHA-256 $sha256. Alternatively, Bob may sign a refund once the output is at least {delay} blocks old. The refund needs no preimage.'
        note = 'Keep the signature and hash check together in the claim branch. Put the delay and refund signature in the other branch. The branch selector chooses a complete route.'
    elif family == 'separate-teams':
        count = 6
        other = 'thresh(2,pk($3),pk($4),pk($5))'
        policy, ms, bad = andp(quorum,other), andm(multi,'multi(2,$3,$4,$5)'), 'multi(4,$0,$1,$2,$3,$4,$5)'
        text = 'Require two of Alice, Bob, and Carol, plus two of Dave, Erin, and Frank. Extra signatures from one team cannot replace approval from the other.'
        note = 'Use two separate signature thresholds. Verify the first threshold before evaluating the second. A single four-of-six threshold permits the wrong team split.'
    elif family == 'guarded-choice':
        count, policy, ms, bad = 3, andp(a,orp(b,c)), andm(a,orm(b,c)), 'multi(2,$0,$1,$2)'
        text = 'Alice must sign every withdrawal. In addition, either Bob or Carol must sign. Bob and Carol together cannot replace Alice.'
        note = 'Verify the mandatory signer first, then choose one alternative signer. A two-of-three threshold would allow a spend without the mandatory signer.'
    elif family == 'global-delay':
        count, policy, ms, bad = 2, andp(lock,orp(a,b)), andm(lock,orm(a,b)), orm(a,andm(lock,b))
        text = f'Either Alice or Bob may sign, but both must wait until the output is at least {delay} blocks old. There is no immediate route.'
        note = 'Place the relative lock before the branch so that it restricts both routes. Verify its result before processing the branch selector.'
    elif family == 'delayed-quorum':
        count, policy, ms, bad = 3, andp(quorum,lock), andm(lock,multi), multi
        text = f'Require any two of Alice, Bob, and Carol and an output age of at least {delay} blocks. The signatures do not waive the delay.'
        note = 'Verify the relative lock and then count signatures. The empty CHECKMULTISIG dummy belongs below the signatures; the script supplies the lock operand.'
    elif family == 'absolute-lock':
        count, policy, ms, bad = 1, andp(a,absolute), andm(absolute,a), a
        text = f'Alice must sign. The spending transaction must have block-height nLockTime of at least {height}, with a non-final input sequence.'
        note = 'Use CLTV for the absolute block-height requirement, then verify the signature. Set a compatible nLockTime and non-final sequence in the spending transaction.'
    elif family == 'guarded-recovery':
        count = 3
        policy, ms = andp(a,orp(b,andp(c,lock))), andm(a,orm(b,andm(lock,c)))
        bad = orm(andm(a,b),andm(lock,c))
        text = f'Alice must sign every withdrawal. She also needs Bob, or Carol after the output is at least {delay} blocks old. The delayed route still requires Alice.'
        note = 'Keep the mandatory signature outside the choice. Only the backup cosigner route carries the relative lock; that route does not bypass the mandatory signer.'
    elif family == 'two-delayed-routes':
        count = 2
        second = f'older({delay+24})'
        policy, ms = orp(andp(a,lock),andp(b,second)), orm(andm(lock,a),andm(second,b))
        bad = andm(lock,orm(a,b))
        text = f'Alice may sign once the output is at least {delay} blocks old. Bob may sign independently once it is at least {delay+24} blocks old. Neither has an earlier route.'
        note = 'Each branch has its own relative lock. Sharing the shorter delay across both branches would release the second route too early.'
    elif family == 'quorum-two-clocks':
        count = 3
        policy, ms = andp(quorum,andp(lock,absolute)), andm(lock,andm(absolute,multi))
        bad = andm(lock,multi)
        text = f'Any two of Alice, Bob, and Carol must sign. The output must be at least {delay} blocks old, AND transaction nLockTime must be at least block height {height}. Use a non-final block-based sequence. Both clocks are mandatory.'
        note = 'Check the relative and absolute locks separately before the quorum. Satisfying the age requirement does not satisfy the absolute-height requirement.'
    elif family == 'team-hash-refund':
        count = 4
        policy, ms = orp(andp(quorum,secret),andp(k(3),lock)), orm(andm(multi,secret),andm(lock,k(3)))
        bad = orm(multi,andm(lock,k(3)))
        text = f'Two of Alice, Bob, and Carol can claim with a 32-byte preimage of SHA-256 $sha256. Dave has a separate signature-only refund route once the output is at least {delay} blocks old.'
        note = 'The team claim requires both a quorum and the preimage. The delayed refund is a separate branch and needs neither the team nor the preimage.'
    else:
        raise ValueError(family)
    return dict(family=family, keys=names[:count], policy=policy, miniscript=ms,
                bad_miniscript=bad, prompt='Write Bitcoin Script for this arrangement. '+text+
                ' Allow exactly these spending routes. Return raw script ASM and state its context.', note=note)


TRAIN = ['both-signatures','either-signature','recovery','relative-lock','quorum',
         'hash-lock','hash-refund','separate-teams','guarded-choice','global-delay',
         'delayed-quorum','absolute-lock']
RESERVED = ['guarded-recovery','two-delayed-routes','quorum-two-clocks','team-hash-refund']


def rows(path):
    return [json.loads(line) for line in Path(path).read_text().splitlines()]


def digest(path):
    with Path(path).open('rb') as f:
        return hashlib.file_digest(f, 'sha256').hexdigest()


def write_json(path, obj):
    with Path(path).open('x') as f:
        json.dump(obj, f, indent=2); f.write('\n')


def write_rows(path, values):
    with Path(path).open('x') as f:
        for value in values:
            f.write(json.dumps(value)+'\n')


def command(args, log, data=None):
    result = subprocess.run(args, input=data, text=True, capture_output=True)
    Path(log).write_text(result.stderr + result.stdout)
    result.check_returncode()
    return result.stdout


def prepare_pool(root, split, families, variants, tokenizer):
    catalog, teaching = [], []
    prefix = 'composed-train-' if split == 'training' else 'composed-val-'
    for family in families:
        for n in range(variants):
            c = case(family,n)
            ident = f'{prefix}{NAME}-{family}-{n}'
            teaching.append(c)
            catalog.append(dict(id=ident, group=ident, kind='write', context='segwitv0',
                choose_context=True, keys=c['keys'], tier='medium', policy=c['policy'], prompt=c['prompt']))
    source = root/f'{split}-catalog.json'
    write_json(source, dict(generator='composed-training-v1' if split=='training' else 'compound-validation-v1',
        purpose='training' if split=='training' else 'validation', suite=f'{NAME}-{split}', seed=SEED,
        evaluation_role='Reserved until the fixed final checkpoint is saved; never used for selection.',
        exclusions={}, cases=catalog))
    base = root/f'{split}-base'
    tool = 'compile_training_catalog' if split=='training' else 'compile_validation_catalog'
    command([f'target/release/examples/{tool}',str(source),str(base)],root/f'{split}-catalog.log')
    originals = rows(base/'fixtures.jsonl')
    inputs = []
    for fixture,c in zip(originals,teaching,strict=True):
        substitutions = {str(i):k['pubkey'] for i,k in enumerate(fixture['keys'])}
        for token,algo in [('sha256','sha256'),('hash160','hash160')]:
            found = re.search(rf'{algo}\(([0-9a-f]+)\)',fixture['reference_policy'])
            if found: substitutions[token] = found[1]
        def expand(value):
            return re.sub(r'\$(sha256|hash160|\d+)',lambda m:substitutions[m[1]],value)
        inputs.append(dict(fixture=fixture,miniscript=expand(c['miniscript']),
            bad_miniscript=expand(c['bad_miniscript']),note=c['note']))
    raw = command(['target/release/examples/compile_script_repairs'],root/f'{split}-targets.log',
                  ''.join(json.dumps(r)+'\n' for r in inputs))
    targets = [json.loads(line) for line in raw.splitlines()]
    write_rows(root/f'{split}-targets.jsonl',targets)
    fixtures, pairs, groups, provenance = [], [], {}, []
    positive, negative = [], []
    for f,c,t,source_case in zip(originals,teaching,targets,catalog,strict=True):
        assert f['id']==t['id']
        for kind in ['write','repair']:
            fixture = copy.deepcopy(f)
            fixture['id'] += '-'+kind
            fixture['reference_miniscript'] = t['miniscript']
            fixture['reference_script_hex'] = t['hex']
            if kind=='repair':
                fixture['request'] += '\nRepair this draft to meet those conditions:\n```text\n'+t['draft']+'\n```'
            fixtures.append(fixture)
            groups[fixture['id']] = source_case['group']
            provenance.append(dict(task_id=fixture['id'],family=c['family'],task_kind=kind,
                                   group=source_case['group'],split=split))
            positive.append(dict(task_id=fixture['id'],answer=dict(task='script',script=t['asm'])))
            negative.append(dict(task_id=fixture['id'],answer=dict(task='script',script=t['draft'])))
            # Only training requests are converted to supervised rows.
            if split!='training': continue
            for mode in ['chat','submit']:
                messages = [dict(role='system',content='You are a helpful assistant.' if mode=='chat' else SYSTEM_PROMPT),
                            dict(role='user',content=fixture['request'])]
                options = dict(tools=[SUBMIT_SCRIPT]) if mode=='submit' else {}
                prompt = tokenizer.apply_chat_template(messages,**options,add_generation_prompt=True,
                                                       enable_thinking=True,tokenize=False)
                trace = 'I choose a P2WSH witness script.\n'+c['note']+'\nAs a policy that is: '+f['reference_policy']+'\nIn Miniscript: '+t['miniscript']+'\n'
                final = t['final_chat'] if mode=='chat' else '<tool_call>\n'+json.dumps(dict(name='submit_script',arguments=dict(script=t['asm'])))+'\n</tool_call>'
                completion = ('' if prompt.rstrip().endswith('<think>') else '<think>\n')+trace+'</think>\n\n'+final+'<|im_end|>'
                pairs.append(dict(prompt=prompt,completion=completion,split=split,interface=mode,
                                  task_id=fixture['id'],source='construction',group=source_case['group']))
    pool = Path('datasets')/f'{NAME}-{split}'
    pool.mkdir()
    write_rows(pool/'fixtures.jsonl',fixtures)
    manifest = json.loads((base/'manifest.json').read_text())
    manifest.update(fixtures_sha256=digest(pool/'fixtures.jsonl'),counts={'t1':len(fixtures)})
    write_json(pool/'manifest.json',manifest)
    write_json(pool/'groups.json',groups)
    write_json(pool/'provenance.json',provenance)
    for name,records,score in [('positive',positive,1),('negative',negative,0)]:
        path = root/f'{split}-{name}.jsonl'; write_rows(path,records)
        out = root/f'{split}-{name}-graded'
        command(['target/release/btc-bench','grade','--dataset',str(pool),'--responses',str(path),
                 '--out',str(out),'--standard-mode'],root/f'{split}-{name}.log')
        grades = json.loads((out/'results.json').read_text())
        assert len(grades)==len(fixtures) and all(r['score']==score for r in grades)
    return pool, fixtures, pairs


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument('--out', type=Path, default=Path('runs')/f'{NAME}-preparation')
    args = ap.parse_args()
    root = args.out
    root.mkdir()
    from transformers import AutoTokenizer
    tokenizer = AutoTokenizer.from_pretrained(PARENT)
    reserved_shapes = {shape(case(f,n)['policy']) for f in RESERVED for n in range(4)}
    training_shapes = {shape(case(f,n)['policy']) for f in TRAIN for n in range(8)}
    assert not reserved_shapes & training_shapes
    train_pool, train, pairs = prepare_pool(root,'training',TRAIN,8,tokenizer)
    reserved_pool, reserved, _ = prepare_pool(root,'reserved',RESERVED,4,tokenizer)
    old_path = Path('datasets/sft-broad-v1-mixed.jsonl')
    old = rows(old_path)
    provenance = json.loads(Path('training/broad-v1-mix.json').read_text())
    assert digest(old_path)==provenance['data_sha256']
    candidates = {}
    for index,(row,p) in enumerate(zip(old,provenance['row_provenance'],strict=True)):
        policy = re.search(r'^(?:As a policy that is|Written as a policy|That gives the policy|Its spending policy is): (.+)$',row['completion'],re.M)
        if policy and shape(policy[1]) in reserved_shapes: continue
        key = (p['source'],p['source_index'])
        candidates.setdefault(key,{})[p['interface']] = (index,row,p)
    rng = random.Random(SEED)
    replay_provenance = []
    # 128 script, 48 tree, 16 identify questions, in both interfaces.
    for kind,count in [('script',128),('tree',48),('identify',16)]:
        options = [v for v in candidates.values() if set(v)=={'chat','submit'} and
                   ('script' if v['chat'][2]['kind'] in ['write','script'] else v['chat'][2]['kind'])==kind]
        for pair in rng.sample(options,count):
            for mode,(index,row,p) in pair.items():
                ident = p.get('task_id') or f"replay-{p['source']}-{p['source_index']}"
                pairs.append(dict(row,split='training',interface=mode,task_id=ident,source='replay',group=ident))
                replay_provenance.append(dict(index=index,**p))
    assert len(pairs)==768
    rng.shuffle(pairs)
    masks = prepare_replay(pairs,tokenizer)
    data = Path('datasets')/f'sft-{NAME}.jsonl'
    write_rows(data,pairs)
    write_json(root/'tokenization.json',dict(rows=len(masks),max_tokens=max(len(r['input_ids']) for r in masks),
                                          supervised_tokens=sum(r['supervised_tokens'] for r in masks)))
    write_json(root/'replay-provenance.json',replay_provenance)
    # Compare exact IDs, requests and script bytes against historical datasets.
    overlap = {}
    new_ids = {f['id'] for f in train+reserved}
    new_requests = {f['request'] for f in train+reserved}
    new_hex = {f['reference_script_hex'] for f in train+reserved}
    for path in sorted(Path('datasets').glob('*/fixtures.jsonl')):
        if path.parent in [train_pool,reserved_pool]: continue
        previous = rows(path)
        counts = dict(ids=len(new_ids & {f['id'] for f in previous}),
                      requests=len(new_requests & {f.get('request') for f in previous}),
                      scripts=len(new_hex & {f.get('reference_script_hex') for f in previous}))
        assert not any(counts.values()), (str(path),counts)
        overlap[str(path)] = counts
    report = dict(name=NAME,parent=PARENT,seed=SEED,data=str(data),data_sha256=digest(data),
        rows=len(pairs),new_scenarios=96,new_questions=len(train),new_rows=384,replay_rows=384,
        reserved_scenarios=16,reserved_questions=len(reserved),train_families=TRAIN,reserved_families=RESERVED,
        reserved_shape_overlap_with_new_training_and_replay=0,exact_historical_overlap=overlap,
        source_hashes={str(old_path):digest(old_path),'training/broad-v1-mix.json':digest('training/broad-v1-mix.json')},
        fixture_hashes={str(p/'fixtures.jsonl'):digest(p/'fixtures.jsonl') for p in [train_pool,reserved_pool]},
        checks='All positive targets and faulty drafts graded. Compiler checks ASM round trip, reference interpreter execution, and chat extraction. Tokenization preserves complete targets. Trainer must audit actual loss masks.',
        limits='Synthetic P2WSH teaching targets. Miniscript verifier; signatures assumed in reference execution. No signed-transaction or arbitrary-Script claim. Reserved shapes are absent from this new mix; parent training may contain them. Related task/interface variants are not independent samples.')
    write_json(root/'verification.json',report)
    print(json.dumps({k:v for k,v in report.items() if k!='exact_historical_overlap'},indent=2))


if __name__=='__main__':
    main()
