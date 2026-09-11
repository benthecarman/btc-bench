"""Add gated-team training; freeze new arrangements and multi-team compositions."""
from collections import Counter
import hashlib
import itertools
import json
from pathlib import Path
import random
import re
import sys

sys.path.insert(0, str(Path(__file__).resolve().parent))
from build_compound_v2 import author, TYPES, HELD_OUT, shape

SEED = 20260913


def main():
    rng = random.Random(SEED)
    excluded, provenance = set(), {}
    for path in [Path('evals/human-v2.json'), Path('evals/composition-transfer-v1.json'),
                 Path('training/compound-approvals-v1-samples.json'), Path('datasets/composed-training-v1-source.json'),
                 Path('training/compound-v2-train.json'), Path('training/compound-v2-validation.json')]:
        obj = json.loads(path.read_text())
        rows = obj if isinstance(obj, list) else obj['cases']
        excluded.update(shape(r['policy']) for r in rows)
        provenance[str(path)] = hashlib.sha256(path.read_bytes()).hexdigest()
    original = Path('datasets/sft-train-think.jsonl')
    for line in original.read_text().splitlines():
        row = json.loads(line)
        match = re.search(r'^(?:As a policy that is|Written as a policy|That gives the policy|Its spending policy is): (.+)$', row['completion'], re.M)
        if match:
            excluded.add(shape(match[1]))
    provenance[str(original)] = hashlib.sha256(original.read_bytes()).hexdigest()
    seen = set(excluded)
    proposals = []
    for width in [3, 4]:
        for gated in HELD_OUT:
            for others in itertools.combinations_with_replacement(TYPES, width - 1):
                if sum(t in ['delay', 'deadline', 'sha', 'hash160'] for t in others) > 1:
                    continue
                for k in range(2, width):
                    for seconds in [False, True]:
                        types = [gated, *others]
                        rng.shuffle(types)
                        case = author(types, k, seconds, rng)
                        if case:
                            proposals.append(case)
    rng.shuffle(proposals)
    accepted = []
    for c in proposals:
        s = shape(c['policy'])
        if s not in seen:
            accepted.append(c)
            seen.add(s)
    assert len(accepted) >= 120, len(accepted)
    new = accepted[:96]
    fresh = [author(c['approval_types'], c['outer_threshold'], c['relative_seconds'], rng, True)
             for c in accepted[96:120]]
    for c in fresh:
        c['validation_bucket'] = 'new single gated-team arrangement'
    double = []
    for width in [3, 4]:
        for gates in itertools.combinations_with_replacement(HELD_OUT, 2):
            for others in itertools.combinations_with_replacement(TYPES, width - 2):
                if any(t in ['delay', 'deadline', 'sha', 'hash160'] for t in others):
                    continue
                for k in range(2, width):
                    for seconds in [False, True]:
                        types = [*gates, *others]
                        rng.shuffle(types)
                        c = author(types, k, seconds, rng, True)
                        if c:
                            double.append(c)
    rng.shuffle(double)
    for c in double:
        s = shape(c['policy'])
        if s in seen:
            continue
        c['validation_bucket'] = 'two gated-team approvals'
        fresh.append(c)
        seen.add(s)
        if len(fresh) == 48:
            break
    assert len(fresh) == 48
    for i, c in enumerate(new):
        c['id'] = c['group'] = f'composed-train-compound-v3-{i:03d}'
    for i, c in enumerate(fresh):
        c['id'] = c['group'] = f'composed-val-compound-v3-{i:03d}'
    prior = json.loads(Path('training/compound-v2-train.json').read_text())['cases']
    train = prior + new
    ts, fs = {shape(c['policy']) for c in train}, {shape(c['policy']) for c in fresh}
    assert len(ts) == 288 and len(fs) == 48 and not ts & fs
    assert not fs & excluded
    assert all(sum(t in HELD_OUT for t in c['approval_types']) <= 1 for c in train)
    for split, cases in [('train', train), ('fresh', fresh)]:
        output = dict(generator='composed-training-v1' if split == 'train' else 'compound-validation-v1',
                      purpose='training' if split == 'train' else 'validation', suite='compound-v3-' + split,
                      seed=SEED, exclusions=provenance, cases=cases,
                      evaluation_role='training' if split == 'train' else 'fresh transfer check; excluded from checkpoint selection')
        with Path(f'training/compound-v3-{split}.json').open('x') as f:
            json.dump(output, f, indent=2)
            f.write('\n')
    coverage = dict(new_gated_training=96, retained_v2_training=192, total_training=288, fresh_questions=48,
                    training_shapes=288, fresh_shapes=48, overlap=0,
                    new_gated_types=dict(Counter(t for c in new for t in c['approval_types'] if t in HELD_OUT)),
                    fresh_buckets=dict(Counter(c['validation_bucket'] for c in fresh)),
                    note='Old v2 validation remains development data. Fresh questions are not used for checkpoint selection. Requests are synthetic; shape exclusion is not proof of semantic novelty.')
    Path('training/compound-v3-coverage.json').write_text(json.dumps(coverage, indent=2) + '\n')
    print(json.dumps(coverage, indent=2))


if __name__ == '__main__':
    main()
