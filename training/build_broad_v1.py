"""Author broader training contracts and reserve separate composition families."""
from collections import Counter
import hashlib
import json
from pathlib import Path
import random
import re
import sys

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'scripts'))
from audit_human_catalog import shape

TRAIN_FAMILIES = ['guarded-choice', 'team-recovery', 'secret-delivery', 'joint-departments',
                  'three-routes', 'staged-quorums', 'two-clock-gate', 'global-secret']
TEST_FAMILIES = ['three-conditioned-departments', 'two-locked-departments',
                 'alternative-evidence-bundles', 'two-secrets-and-clock-vote']
NAMES = ['Ari', 'Bea', 'Cleo', 'Dara', 'Eli', 'Finn', 'Gia', 'Hugo', 'Inez', 'Jules', 'Kai', 'Luz']


def combine(op, *nodes):
    assert len(nodes) >= 2
    return nodes[0] if len(nodes) == 1 else f'{op}({nodes[0]},{combine(op, *nodes[1:]) if len(nodes)>2 else nodes[1]})'


class Contract:
    def __init__(self, rng):
        self.rng = rng
        self.names = rng.sample(NAMES, len(NAMES))
        self.keys = []
        self.rel_time = rng.choice([False, True])
        self.abs_time = rng.choice([False, True])

    def signer(self):
        n = len(self.keys)
        self.keys.append(self.names[n])
        return f'pk(${n})', self.keys[-1] + ' signs'

    def team(self, lo=2, hi=4):
        n = self.rng.randint(lo, hi)
        members = [self.signer() for _ in range(n)]
        k = self.rng.randint(2, n)
        labels = self.keys[-n:]
        names = ' and '.join(labels) if n == 2 else ', '.join(labels[:-1]) + ', and ' + labels[-1]
        policy = combine('and',*(x[0] for x in members)) if k == n else f'thresh({k},{",".join(x[0] for x in members)})'
        return policy, f'at least {k} of {names} sign'

    def condition(self, kind=None):
        kind = kind or self.rng.choice(['older', 'after', 'sha256', 'hash160'])
        if kind in ['sha256', 'hash160']:
            label = 'SHA-256' if kind == 'sha256' else 'HASH160'
            return f'{kind}(${kind})', f'the 32-byte secret matching {label} ${kind} is supplied'
        if kind == 'older':
            n = self.rng.choice([4, 12, 24, 48, 144, 288])
            if self.rel_time:
                return f'older({(1<<22)+n})', f'the output meets a relative time lock of {n*512} seconds, using {n} units of 512 seconds on Bitcoin\'s relative-lock clock'
            return f'older({n})', f'the output is at least {n} blocks old, measured from confirmation'
        n = self.rng.randint(1800000000, 1900000000) if self.abs_time else self.rng.randint(930000, 990000)
        unit = 'time-based locktime (Unix seconds)' if self.abs_time else 'block-height locktime'
        return f'after({n})', f'the spending transaction has {unit} of at least {n}'


def training_contract(c, family):
    if family == 'guarded-choice':
        guard, a, b, gate = c.signer(), c.team(2,3), c.team(2,3), c.condition()
        p = combine('and', guard[0], combine('or', a[0], b[0]), gate[0])
        text = f'{guard[1].capitalize()} for either group\'s route. In addition, either {a[1]}, or {b[1]}. Whichever group signs, {gate[1]}. The guard cannot replace either group.'
    elif family == 'team-recovery':
        a, b, x, y = c.team(), c.team(), c.condition(), c.condition()
        p = combine('or', combine('and',a[0],x[0]), combine('and',b[0],y[0]))
        text = f'This arrangement allows two spending routes. The first works when {a[1]}, provided that {x[1]}. The second works when {b[1]}, provided that {y[1]}. Either complete route is enough.'
    elif family == 'secret-delivery':
        a,b,h,r,t = c.team(),c.team(),c.condition(c.rng.choice(['sha256','hash160'])),c.condition('older'),c.condition('after')
        p = combine('or', combine('and',a[0],h[0]), combine('and',b[0],r[0],t[0]))
        text = f'Delivery can be authorized when {a[1]} and {h[1]}. A separate recovery route works when {b[1]}, but only when both of these clock requirements hold: {r[1]}, and {t[1]}. Recovery does not need the secret.'
    elif family == 'joint-departments':
        a,b,g = c.team(),c.team(),c.condition()
        p = combine('and',a[0],b[0],g[0])
        text = f'Both departments must approve: {a[1]}, and {b[1]}. Extra signatures from one department cannot replace approval from the other. We also require that {g[1]}.'
    elif family == 'three-routes':
        a,b,k,x,y = c.team(2,3),c.team(2,3),c.signer(),c.condition(),c.condition()
        p = combine('or',a[0],combine('and',b[0],x[0]),combine('and',k[0],y[0]))
        text = f'Within this arrangement, any one of three routes is enough: {a[1]} with no waiting or secret; {b[1]} together with the requirement that {x[1]}; or {k[1]} together with the requirement that {y[1]}. Conditions belong only to their stated route.'
    elif family == 'staged-quorums':
        a,b,d = c.team(2,3),c.team(2,3),c.team(2,3)
        x,y,z = c.condition('older'),c.condition('older'),c.condition(c.rng.choice(['after','sha256','hash160']))
        p = combine('or',a[0],combine('and',b[0],x[0]),combine('and',d[0],y[0],z[0]))
        text = f'The normal route is available whenever {a[1]}. A backup route works when {b[1]}, provided that {x[1]}. A third route works when {d[1]}, provided both that {y[1]} and that {z[1]}. Each route stays available once its conditions hold.'
    elif family == 'two-clock-gate':
        a,x,y = c.team(2,6),c.condition('older'),c.condition('after')
        p = combine('and',a[0],x[0],y[0])
        text = f'To use this arrangement, {a[1]}. Signing is not enough on its own: {x[1]}, and {y[1]}. Both clocks must pass; either one alone is insufficient.'
    elif family == 'global-secret':
        a,b,h,g = c.team(),c.team(),c.condition(c.rng.choice(['sha256','hash160'])),c.condition(c.rng.choice(['older','after']))
        p = combine('and',h[0],combine('or',a[0],combine('and',b[0],g[0])))
        text = f'Both of the following routes require that {h[1]}. With that secret, one route works when {a[1]}. The other works when {b[1]}, but only if {g[1]}. The second route still needs the secret.'
    else:
        raise ValueError(family)
    return p,text


def reserved_contract(c, family):
    # These composition families have their own request text and are never used for training.
    if family == 'three-conditioned-departments':
        teams = [c.team(2,3) for _ in range(3)]
        gates = [c.condition() for _ in range(3)]
        p = 'thresh(2,' + ','.join(combine('and',t[0],g[0]) for t,g in zip(teams,gates)) + ')'
        text = 'Our three departments each get one vote. Any two department votes release the funds. Their votes are defined below:\n' + '\n'.join(f'- Department {i+1}: {t[1]}, and {g[1]}.' for i,(t,g) in enumerate(zip(teams,gates))) + '\nSignatures inside a department do not count as extra votes.'
    elif family == 'two-locked-departments':
        a,b,r,t = c.team(),c.team(),c.condition('older'),c.condition('after')
        p = combine('and',combine('and',a[0],r[0]),combine('and',b[0],t[0]))
        text = f'I need approval from two separate councils. For the first council, {a[1]}, and {r[1]}. For the second, {b[1]}, and {t[1]}. Both council approvals are required for the same withdrawal. Neither council can waive the other council\'s clock requirement.'
    elif family == 'alternative-evidence-bundles':
        a,h1,h2,r,t = c.team(2,5),c.condition('sha256'),c.condition('hash160'),c.condition('older'),c.condition('after')
        witness1,witness2 = c.signer(),c.signer()
        p = combine('and',a[0],combine('or',combine('and',witness1[0],h1[0],r[0]),combine('and',witness2[0],h2[0],t[0])))
        text = f'The signers must satisfy this rule: {a[1]}. They must also supply one complete evidence bundle. Bundle one requires that {witness1[1]}, {h1[1]}, and {r[1]}. Bundle two requires that {witness2[1]}, {h2[1]}, and {t[1]}. Either complete bundle works; do not mix the signature or secret from one with the clock from the other.'
    elif family == 'two-secrets-and-clock-vote':
        a,b,k,h1,h2,r,t = c.team(2,3),c.team(2,3),c.signer(),c.condition('sha256'),c.condition('hash160'),c.condition('older'),c.condition('after')
        p = 'thresh(2,' + ','.join([combine('and',a[0],h1[0]),combine('and',b[0],h2[0]),combine('and',k[0],r[0],t[0])]) + ')'
        text = f'A withdrawal needs any two of these three approvals:\n- {a[1]}, and {h1[1]}.\n- {b[1]}, and {h2[1]}.\n- {k[1]}, but only when {r[1]} and {t[1]}.\nEach bullet counts once, regardless of how many people sign it.'
    else:
        raise ValueError(family)
    return p,text


def source_cases(path):
    obj = json.loads(path.read_text())
    return obj if isinstance(obj,list) else obj.get('cases',[])


def main():
    rng = random.Random(20260907)
    prior_paths = [p for p in list(Path('evals').glob('*.json')) + list(Path('training').glob('*.json'))
                   if not p.name.startswith('broad-v1')]
    prior_paths += [Path('datasets/composed-training-v1-source.json')]
    historical, eval_shapes, hashes = set(),set(),{}
    for p in prior_paths:
        if not p.exists():
            continue
        cases = [c for c in source_cases(p) if isinstance(c,dict) and 'policy' in c]
        if not cases:
            continue
        shapes = {shape(c['policy']) for c in cases}
        historical.update(shapes)
        if p.parent.name == 'evals' or any(x in p.name for x in ['validation','fresh','fit-sample']):
            eval_shapes.update(shapes)
        hashes[str(p)] = hashlib.sha256(p.read_bytes()).hexdigest()
    old = Path('datasets/sft-train-think.jsonl')
    found = 0
    for line in old.read_text().splitlines():
        row = json.loads(line)
        match = re.search(r'^(?:As a policy that is|Written as a policy|That gives the policy|Its spending policy is): (.+)$',row['completion'],re.M)
        if match:
            historical.add(shape(match[1])); found += 1
    hashes[str(old)] = hashlib.sha256(old.read_bytes()).hexdigest()
    seen = set()
    outputs = {}
    rejects = Counter()
    for split, families, count in [('training',TRAIN_FAMILIES,32),('reserved',TEST_FAMILIES,8)]:
        cases = []
        for family in families:
            accepted = 0
            for _ in range(20000):
                if accepted == count:
                    break
                c = Contract(rng)
                policy, prose = (training_contract if split=='training' else reserved_contract)(c,family)
                if len(c.keys)>7:
                    rejects['key budget'] += 1; continue
                owner,owner_text = c.signer()
                tree_policy = combine('or',owner,policy)
                shapes = {shape(policy),shape(tree_policy)}
                blocked = eval_shapes if split=='training' else historical
                if shapes & (blocked | seen):
                    rejects[split+' shape exclusion'] += 1; continue
                seen.update(shapes)
                tag = 'composed-train' if split=='training' else 'composed-val'
                group = f'{tag}-broad-v1-{family}-{accepted:03d}'
                for kind in ['write','tree']:
                    if kind=='write':
                        request = rng.choice(['Can you write the Bitcoin script for this reserve?','Please help me make a Bitcoin script for our shared funds.','I need a Bitcoin script with the following spending rules.'])
                        prompt = request+'\n\n'+prose+'\n\nAllow every spend that meets these rules and no other route.'
                        context, keys, p = 'segwitv0',c.keys[:-1],policy
                    else:
                        prompt = f'Please give me a Taproot tr() descriptor for our wallet. {owner_text.capitalize()} to spend immediately without any other approval, secret, or waiting period. The rules in the next paragraph apply only to the alternative spending arrangement, not to {c.keys[-1]}\'s route:\n\n'+prose+f'\n\nThe alternative arrangement does not require {c.keys[-1]} to sign. Allow all of these routes and no others. Choose the internal key and leaves to reduce the worst-case input weight.'
                        context,keys,p = 'tap',c.keys,tree_policy
                    cases.append(dict(id=group+'-'+kind,group=group,family=family,kind=kind,context=context,
                                      choose_context=kind=='write',keys=keys,tier='hard',policy=p,prompt=prompt))
                accepted += 1
            assert accepted == count,(split,family,accepted)
        outputs[split] = cases
    for split,cases in outputs.items():
        training = split=='training'
        data = dict(generator='composed-training-v1' if training else 'compound-validation-v1',
                    purpose='training' if training else 'validation',suite='broad-v1-'+split,
                    evaluation_role='Reserved composition families; excluded from training and checkpoint selection',
                    seed=20260907,exclusions=hashes,cases=cases)
        with Path(f'training/broad-v1-{split}.json').open('x') as f:
            json.dump(data,f,indent=2);f.write('\n')
    report = dict(training_cases=len(outputs['training']),reserved_cases=len(outputs['reserved']),
                  training_groups=len(outputs['training'])//2,reserved_groups=len(outputs['reserved'])//2,
                  training_families=TRAIN_FAMILIES,reserved_families=TEST_FAMILIES,
                  rejected=dict(rejects),historical_policy_rows=found,exclusions=hashes,
                  limits='Synthetic authored templates; operator separation is not proof of semantic novelty. Script/tree derivatives share a group. Tree compiler requires an immediate owner route.')
    Path('training/broad-v1-coverage.json').write_text(json.dumps(report,indent=2)+'\n')
    print(json.dumps({k:v for k,v in report.items() if k!='exclusions'},indent=2))


if __name__=='__main__':
    main()
