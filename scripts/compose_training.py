#!/usr/bin/env python3
"""Sample a separate recursive training grammar; reject human-eval shapes.

No evaluation policy or prompt is used as a generation template. Evaluation
catalogs are read only to build the exclusion set. Compilation is a separate
all-or-nothing step so unsupported proposals cannot silently disappear.
"""
import argparse
from collections import Counter
import hashlib
import json
from pathlib import Path
import random

from audit_human_catalog import shape


def source(seed, write, tree, excluded):
    rng = random.Random(seed)
    cases, seen, rejected = [], set(), Counter()
    for kind, count in [('write', write), ('tree', tree)]:
        accepted = 0
        for attempt in range(max(1, count) * 1000):
            if accepted == count:
                break
            keys = []
            relative_seconds = rng.random() < 0.3
            absolute_seconds = rng.random() < 0.2

            def key():
                i = len(keys)
                keys.append(f'Key_{i + 1}')
                return f'pk(${i})', f'{keys[-1]} signs'

            def join(op, left, right):
                phrases = {'and': ['both', 'all of'], 'or': ['at least one of', 'either of']}
                return f'{op}({left[0]},{right[0]})', f'{rng.choice(phrases[op])} [{left[1]}; {right[1]}]'

            def gate(node):
                which = rng.randrange(5)
                if which in (0, 2) and not relative_seconds:
                    n = rng.randint(1, 40) * 32
                    atom = f'older({n})', f'a relative block lock of {n} blocks since output confirmation holds'
                elif which == 1:
                    n = rng.randint(1800000001, 1900000000) if absolute_seconds else rng.randint(910001, 999999)
                    unit = 'time-based locktime (Unix seconds)' if absolute_seconds else 'block-height locktime'
                    atom = f'after({n})', f'the transaction {unit} is at least {n}'
                elif which in (0, 2):
                    n = rng.randint(1, 24)
                    atom = f'older({(1 << 22) + n})', f'a relative time lock of {n * 512} seconds holds, using {n} units of 512 seconds on the Bitcoin lock clock'
                elif which == 3:
                    atom = 'sha256($sha256)', 'the 32-byte preimage of SHA-256 $sha256 is supplied'
                else:
                    atom = 'hash160($hash160)', 'the 32-byte preimage of HASH160 $hash160 is supplied'
                return join('and', node, atom)

            def node(depth):
                if depth == 0 or rng.random() < 0.40:
                    n = rng.randint(1, 4)
                    children = [key() for _ in range(n)]
                    k = rng.randint(1, n)
                    if n == 1:
                        result = children[0]
                    else:
                        result = f'thresh({k},{",".join(c[0] for c in children)})', f'at least {k} of these {n} approvals [{"; ".join(c[1] for c in children)}]'
                else:
                    result = join(rng.choice(['and', 'or']), node(depth - 1), node(depth - 1))
                return gate(result) if rng.random() < 0.42 else result

            if kind == 'tree':
                owner = key()
                policy, prose = join('or', owner, node(rng.randint(1, 2)))
                context, choose_context = 'tap', False
                request = 'Please give a tr() descriptor for exactly this spending rule: '
                suffix = ' Choose the internal key and leaves to reduce the worst-case input weight. Return the descriptor.'
            else:
                policy, prose = node(rng.randint(1, 3))
                context = rng.choice(['segwitv0', 'tap'])
                choose_context = context == 'segwitv0'
                request = rng.choice(['Please implement this rule in ', 'I need ', 'Construct ']) + ('a Bitcoin script' if choose_context else 'a tapscript leaf') + ': '
                suffix = ' Return the script in hex or Bitcoin Core assembly.'
            if len(keys) > 8:
                rejected['key budget'] += 1
                continue
            policy_shape = shape(policy)
            if policy_shape in excluded:
                rejected['evaluation shape'] += 1
                continue
            # One question per operator shape, across both task types.
            if policy_shape in seen:
                rejected['duplicate training shape'] += 1
                continue
            seen.add(policy_shape)
            group = f'composed-train-{seed}-{kind}-{accepted:04d}'
            cases.append(dict(id=group, group=group, kind=kind, context=context,
                              choose_context=choose_context, keys=keys,
                              tier='hard' if len(keys) > 4 else 'medium', policy=policy,
                              prompt=request + prose + '. No additional spending routes are allowed.' + suffix))
            accepted += 1
        if accepted != count:
            raise ValueError(f'Only found {accepted}/{count} distinct {kind} policies')
    return cases, dict(rejected)


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument('--seed', type=int, default=20260906)
    ap.add_argument('--write', type=int, default=256)
    ap.add_argument('--tree', type=int, default=128)
    ap.add_argument('--exclude', type=Path, nargs='+', default=[Path('evals/human-v2.json'), Path('evals/composition-transfer-v1.json')])
    ap.add_argument('--out', type=Path, default=Path('datasets/composed-training-v1-source.json'))
    args = ap.parse_args()
    if min(args.write, args.tree) < 0 or args.write + args.tree == 0:
        ap.error('counts must be nonnegative and total must be positive')
    exclusions, excluded = {}, set()
    for path in args.exclude:
        rows = json.loads(path.read_text())
        excluded.update(shape(r['policy']) for r in rows)
        exclusions[str(path)] = hashlib.sha256(path.read_bytes()).hexdigest()
    cases, rejections = source(args.seed, args.write, args.tree, excluded)
    result = dict(generator='composed-training-v1', purpose='training', seed=args.seed,
                  exclusions=exclusions, excluded_shapes=len(excluded), rejections=rejections, cases=cases)
    args.out.parent.mkdir(parents=True, exist_ok=True)
    with args.out.open('x') as f:
        json.dump(result, f, indent=2)
        f.write('\n')
    print(f'{len(cases)} unique training operator shapes; {len(excluded)} evaluation shapes excluded; {args.out}')


if __name__ == '__main__':
    main()
