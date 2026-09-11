"""Author compound-approval training and separate structural validation sets."""
from collections import Counter
import hashlib
import itertools
import json
from pathlib import Path
import random
import re
import sys

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'scripts'))
from audit_human_catalog import shape

SEED = 20260911
NAMES = ['Nora', 'Eli', 'Sam', 'Tess', 'Mina', 'Owen', 'Pia', 'Ada', 'Ben',
         'Cleo', 'Uma', 'Vik', 'Wren', 'Alex', 'Blair', 'Casey']
TYPES = ['person', 'pair', 'either', 'committee', 'delay', 'deadline', 'sha', 'hash160']
HELD_OUT = ['delayed_pair', 'secret_pair', 'delayed_committee']


def author(types, k, seconds, rng, validation=False):
    names = rng.sample(NAMES, len(NAMES))
    keys = []

    def key():
        i = len(keys)
        keys.append(names[i])
        return f'pk(${i})', names[i]

    def delay():
        units = rng.choice([2, 4, 6, 12]) if seconds else rng.choice([48, 96, 144, 288, 432])
        if seconds:
            return f'older({(1 << 22) + units})', f'a relative time lock of at least {units * 512} seconds, using {units} units of 512 seconds on Bitcoin\'s relative-lock clock'
        return f'older({units})', f'the output to be at least {units} blocks old, measured from confirmation'

    policies, descriptions = [], []
    for kind in types:
        a, an = key()
        if kind == 'person':
            policy, prose = a, f'{an} can give one approval with a signature'
        elif kind in ['pair', 'either', 'delayed_pair', 'secret_pair']:
            b, bn = key()
            policy = f'{"or" if kind == "either" else "and"}({a},{b})'
            prose = (f'{an} and {bn} share one approval, which either person can give alone; both signing still counts once'
                     if kind == 'either' else f'{an} and {bn} share one approval and must both sign for it')
            if kind == 'delayed_pair':
                gate, text = delay()
                policy = f'and({policy},{gate})'
                prose += f'; their joint approval also needs {text}'
            elif kind == 'secret_pair':
                policy = f'and({policy},sha256($sha256))'
                prose += '; their joint approval also needs the 32-byte secret matching SHA-256 $sha256'
        elif kind in ['committee', 'delayed_committee']:
            b, bn = key()
            c, cn = key()
            policy = f'thresh(2,{a},{b},{c})'
            prose = f'{an}, {bn} and {cn} form a committee; any two must sign to give its one approval'
            if kind == 'delayed_committee':
                gate, text = delay()
                policy = f'and({policy},{gate})'
                prose += f', and that approval also needs {text}'
        elif kind == 'delay':
            gate, text = delay()
            policy, prose = f'and({a},{gate})', f'{an} gives one approval with a signature, but it can count only with {text}'
        elif kind == 'deadline':
            height = rng.randint(930000, 990000)
            policy = f'and({a},after({height}))'
            prose = f'{an} gives one approval with a signature, but it needs the spending transaction to have block-height locktime at least {height}'
        else:
            function = 'sha256' if kind == 'sha' else 'hash160'
            title = 'SHA-256' if kind == 'sha' else 'HASH160'
            policy = f'and({a},{function}(${function}))'
            prose = f'{an} gives one approval with a signature and the 32-byte secret matching {title} ${function}; neither item alone is enough'
        policies.append(policy)
        descriptions.append(prose)
    if len(keys) > 6:
        return None
    policy = f'thresh({k},{",".join(policies)})'
    purpose = rng.choice(['club reserve', 'co-op wallet', 'charity fund', 'business reserve', 'community fund'])
    if validation:
        opening = rng.choice([
            f'Can you write the Bitcoin script for our {purpose}? We need at least {k} approvals to release the money.',
            f'I am setting up a {purpose}. A withdrawal should work with any {k} of these approvals. Please write the Bitcoin script.',
        ])
        prompt = opening + '\n\n' + '\n'.join(f'- {s[0].upper() + s[1:]}.' for s in descriptions)
        prompt += '\n\nEach item above counts as one approval. Extra signatures within an item do not add approvals. Allow every combination that meets these rules and no other spending route.'
    else:
        opening = rng.choice([
            f'Our {purpose} needs at least {k} of the following {len(types)} approvals for a withdrawal.',
            f'Please set up this {purpose} so that any {k} approvals from this list can spend.',
            f'For our {purpose}, the spending rule is at least {k} approvals out of these {len(types)}.',
        ])
        prompt = opening + '\n\n' + '\n'.join(f'{i + 1}. {s[0].upper() + s[1:]}.' for i, s in enumerate(descriptions))
        prompt += '\n\nCount each listed approval once, even if it has several signatures or a secret. Extra approvals are fine. Do not add another way to spend. Write the Bitcoin script.'
    return dict(kind='write', context='segwitv0', choose_context=True, keys=keys,
                tier='hard' if len(keys) > 4 else 'medium', policy=policy, prompt=prompt,
                approval_types=list(types), relative_seconds=seconds, outer_threshold=k)


def main():
    rng = random.Random(SEED)
    excluded, hashes, coverage = set(), {}, {}
    for path in [Path('evals/human-v2.json'), Path('evals/composition-transfer-v1.json'),
                 Path('training/compound-approvals-v1-samples.json'), Path('datasets/composed-training-v1-source.json')]:
        doc = json.loads(path.read_text())
        rows = doc if isinstance(doc, list) else doc['cases']
        excluded.update(shape(r['policy']) for r in rows)
        hashes[str(path)] = hashlib.sha256(path.read_bytes()).hexdigest()
    original = Path('datasets/sft-train-think.jsonl')
    recognized = 0
    for line in original.read_text().splitlines():
        row = json.loads(line)
        match = re.search(r'^(?:As a policy that is|Written as a policy|That gives the policy|Its spending policy is): (.+)$', row['completion'], re.M)
        if match:
            excluded.add(shape(match[1]))
            recognized += 1
    hashes[str(original)] = hashlib.sha256(original.read_bytes()).hexdigest()
    coverage['recognized_original_policy_traces'] = recognized
    candidates = []
    for width in [3, 4]:
        for types in itertools.combinations_with_replacement(TYPES, width):
            if sum(t in ['delay', 'deadline', 'sha', 'hash160'] for t in types) > 2:
                continue
            if all(t == 'person' for t in types):
                continue
            for k in range(2, width):
                for seconds in [False, True]:
                    ordered = list(types)
                    rng.shuffle(ordered)
                    item = author(ordered, k, seconds, rng)
                    if item:
                        candidates.append(item)
    rng.shuffle(candidates)
    seen, accepted = set(excluded), []
    for item in candidates:
        s = shape(item['policy'])
        if s not in seen:
            accepted.append(item)
            seen.add(s)
    assert len(accepted) >= 216, len(accepted)
    train = accepted[:192]
    familiar = []
    for item in accepted[192:216]:
        familiar.append(author(item['approval_types'], item['outer_threshold'], item['relative_seconds'], rng, True))
    held_candidates = []
    for held in HELD_OUT:
        for rest in itertools.combinations_with_replacement(TYPES, 2):
            if sum(t in ['delay', 'deadline', 'sha', 'hash160'] for t in rest) > 1:
                continue
            for seconds in [False, True]:
                types = [held, *rest]
                rng.shuffle(types)
                item = author(types, 2, seconds, rng, True)
                if item:
                    held_candidates.append(item)
    rng.shuffle(held_candidates)
    held = []
    for item in held_candidates:
        s = shape(item['policy'])
        if s not in seen:
            held.append(item)
            seen.add(s)
        if len(held) == 24:
            break
    assert len(held) == 24
    validation = familiar + held
    for split, rows in [('train', train), ('validation', validation)]:
        for i, item in enumerate(rows):
            item['group'] = item['id'] = f'composed-{split if split == "train" else "val"}-compound-v2-{i:03d}'
            item['validation_bucket'] = ('held-out gated teams' if i >= 24 else 'new arrangement') if split == 'validation' else None
        result = dict(generator='composed-training-v1' if split == 'train' else 'compound-validation-v1',
                      purpose='training' if split == 'train' else 'validation', seed=SEED,
                      exclusions=hashes, coverage=coverage, cases=rows)
        with Path(f'training/compound-v2-{split}.json').open('x') as f:
            json.dump(result, f, indent=2)
            f.write('\n')
    train_shapes = {shape(c['policy']) for c in train}
    validation_shapes = {shape(c['policy']) for c in validation}
    assert len(train_shapes) == 192 and len(validation_shapes) == 48
    assert not train_shapes & validation_shapes
    assert not (train_shapes | validation_shapes) & excluded
    assert all(not set(c['approval_types']) & set(HELD_OUT) for c in train)
    report = dict(seed=SEED, train=192, validation=48, train_shapes=192, validation_shapes=48,
                  structural_overlap=0, excluded_shapes=len(excluded), coverage=coverage,
                  train_approval_types=dict(Counter(t for c in train for t in c['approval_types'])),
                  validation_buckets=dict(Counter(c['validation_bucket'] for c in validation)),
                  limit='Synthetic requests. Structural checks are not proofs of semantic novelty. Validation will select checkpoints, so it is development data.')
    Path('training/compound-v2-coverage.json').write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps(report, indent=2))


if __name__ == '__main__':
    main()
