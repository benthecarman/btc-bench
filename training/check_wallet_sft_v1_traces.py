"""Audit explicit model trace expressions, independently of final-answer grades."""
import argparse
from collections import Counter
import hashlib
import json
from pathlib import Path
import re
import subprocess

ROOT = Path(__file__).resolve().parents[1]


def rows(path):
    return list(map(json.loads, Path(path).read_text().splitlines()))


def expand(fixture, text):
    if text is None: return None
    symbolic = re.findall(r'pk\((@\d+)\)', fixture['policy_template'])
    actual = re.findall(r'pk\(([^()]+)\)', fixture['derivations'][0]['policy'])
    assert len(symbolic) == len(actual)
    mapping = dict(zip(symbolic, actual))
    return re.sub(r'@\d+(?:/\*\*)?', lambda m: mapping.get(m[0].removesuffix('/**'), m[0]), text)


def extract(reasoning):
    policies = re.findall(r'^(?:As a policy that is|Written as a policy|That gives the policy|Its spending policy is):\s*([^\n]+)', reasoning, re.M)
    policy = policies[0].strip() if len(policies) == 1 else None
    marks = list(re.finditer(r'^(?:Taproot leaf Miniscript|P2WSH descriptor body|In Miniscript|Miniscript):[ \t]*([^\n]*)', reasoning, re.M))
    bodies = None
    if len(marks) == 1:
        mark = marks[0]
        lines = ([mark[1]] if mark[1].strip() else []) + reasoning[mark.end():].splitlines()
        body_lines = []
        started = False
        for line in lines:
            line = line.strip()
            if not line and not started: continue
            if not line: break
            if started and not re.match(r'[A-Za-z_][A-Za-z0-9_:]*\(', line): break
            started = True
            body_lines.append(line)
        bodies = body_lines or None
    return policy, bodies


def check(inputs):
    data = ''.join(json.dumps(row) + '\n' for row in inputs)
    result = subprocess.run(['target/release/examples/check_wallet_trace'], input=data, text=True, capture_output=True, check=True)
    checked = list(map(json.loads, result.stdout.splitlines()))
    assert len(checked) == len(inputs)
    return checked


def main():
    ap = argparse.ArgumentParser(); ap.add_argument('--check-targets', action='store_true'); args = ap.parse_args()
    if args.check_targets:
        fixtures = rows('datasets/wallet-sft-v1-training/fixtures.jsonl')
        traces = json.loads(Path('datasets/wallet-sft-v1-training/traces.json').read_text())
        inputs = [dict(id=f['id'], fixture=f, policy=expand(f,t['policy']), bodies=[expand(f,b) for b in t['bodies']]) for f,t in zip(fixtures,traces)]
        result = check(inputs)
        assert all(r['policy']['status']=='equivalent' and r['bodies']['status'] in ['equivalent with reference internal key','not applicable'] for r in result)
        # A wrong reasoning policy must fail even when the fixture/final target is correct.
        original = next(r for r in inputs if r['bodies'])
        bad = dict(original, policy='UNSATISFIABLE', bodies=['c:older(1)'])
        negative = check([bad])[0]
        assert negative['policy']['status'] != 'equivalent'
        assert negative['bodies']['status'] == 'invalid or not equivalent'
        print(f'Checked {len(result)} teaching policies/body sets and a wrong-trace control')
        return
    runs = []
    for mode in ['chat','submit']:
        runs.append(('parent','wallet-policy-v1',mode,Path(f'runs/wallet-policy-v1-baseline/{mode}')))
        runs.append(('step128','wallet-policy-v1',mode,Path(f'runs/sft-wallet-v1/step128-wallet-policy-v1-{mode}')))
        for label in ['parent','step128']:
            runs.append((label,'wallet-sft-v1-reserved',mode,Path(f'runs/sft-wallet-v1/{label}-wallet-sft-v1-reserved-{mode}')))
    inputs, metadata = [], []
    for label,suite,mode,directory in runs:
        fixtures = {f['id']:f for f in rows(f'datasets/{suite}/fixtures.jsonl')}
        grades = {r['task_id']:r for r in json.loads((directory/'graded/results.json').read_text())}
        for row in rows(directory/'responses.jsonl'):
            f = fixtures[row['task_id']]
            message = row['raw_response']['choices'][0]['message']
            reasoning = message.get('reasoning') or message.get('reasoning_content') or ''
            policy,bodies = extract(reasoning)
            ident = f'{label}/{suite}/{mode}/{f["id"]}'
            inputs.append(dict(id=ident,fixture=f,policy=expand(f,policy),bodies=[expand(f,b) for b in bodies] if bodies else None))
            metadata.append(dict(id=ident,label=label,suite=suite,mode=mode,output_kind=f['output_kind'],correct=grades[f['id']]['score']==1))
    result = check(inputs)
    combined = [dict(**m,policy=r['policy'],bodies=r['bodies']) for m,r in zip(metadata,result)]
    report = {'scope':'Only explicitly marked expressions; no syntax repair. Body equivalence uses the reference internal key and does not validate the model choice of key path. Missing markers are unmeasured, not proof of wrong reasoning.',
              'rows':combined,'counts':dict(Counter(r['label']+'/'+r['suite']+'/'+r['policy']['status'] for r in combined)),
              'checker_sha256':hashlib.sha256(Path('crates/bench-wallet/examples/check_wallet_trace.rs').read_bytes()).hexdigest()}
    out=Path('runs/sft-wallet-v1/trace-audit.json')
    with out.open('x') as f: json.dump(report,f,indent=2);f.write('\n')
    print(json.dumps(report['counts'],indent=2))


if __name__=='__main__': main()
