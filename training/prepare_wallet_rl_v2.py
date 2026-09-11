"""Freeze an earlier-skill replay sequence using previously checked references."""
from collections import Counter
import hashlib
import json
from pathlib import Path
import random
import re
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / 'scripts'))
from rl_common import extract_task_answer
from rl_replay import prepare_replay


def rows(path):
    return list(map(json.loads, Path(path).read_text().splitlines()))


def digest(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def main():
    from transformers import AutoTokenizer
    root = Path('runs/rl-wallet-v2')
    root.mkdir(exist_ok=True)
    mix = json.loads(Path('training/wallet-sft-v1-mix.json').read_text())
    assert digest(mix['data']) == mix['data_sha256']
    fixtures = rows('datasets/broad-v1-training/fixtures.jsonl')
    manifest = json.loads(Path('datasets/broad-v1-training/manifest.json').read_text())
    assert manifest['evaluation_only'] is False
    assert digest('datasets/broad-v1-training/fixtures.jsonl') == manifest['fixtures_sha256']
    data = rows(mix['data'])
    output, targets, trace_inputs, chat_targets = [], [], [], {}
    for row, provenance in zip(data, mix['row_provenance'], strict=True):
        if provenance['source'] != 'broad':
            continue
        fixture = fixtures[provenance['source_index']]
        assert 'train-broad-v1' in fixture['id']
        text = row['completion']
        policy = re.search(r'^As a policy that is: (.+)$', text, re.M)
        assert policy and policy[1] == fixture['reference_policy']
        if fixture['task'] == 'write':
            ms = re.search(r'^In Miniscript: (.+)$', text, re.M)
            assert ms and ms[1] == fixture['reference_miniscript']
            trace_inputs.append(dict(id=fixture['id']+'/'+provenance['interface'],
                reference_policy=fixture['reference_policy'], policy=policy[1], miniscript=ms[1]))
        else:
            marker = 'A verified descriptor for those spending paths:\n'
            assert text.split(marker)[1].splitlines()[0] == fixture['reference_descriptor']
        if provenance['interface'] == 'submit':
            answer = extract_task_answer(text.removesuffix('<|im_end|>'), fixture, thinking=True)
            targets.append(dict(task_id=fixture['id'], answer=answer))
        else:
            blocks = re.findall(r'```(?:text)?\n(.*?)\n```', text, re.S)
            assert len(blocks) == 1
            chat_targets[fixture['id']] = blocks[0]
        output.append(dict(row, split='training', task_id=fixture['id'], interface=provenance['interface']))
    assert len(output) == 256 and len(targets) == 128
    for target in targets:
        answer = target['answer']
        assert chat_targets[target['task_id']] == answer.get('script', answer.get('descriptor'))
    random.Random(7).shuffle(output)
    checked = subprocess.run(['target/release/examples/check_trace_parts'],
        input=''.join(json.dumps(r)+'\n' for r in trace_inputs), text=True, capture_output=True, check=True)
    checks = list(map(json.loads, checked.stdout.splitlines()))
    assert len(checks) == 128
    assert all(r[k]['status'] == 'equivalent' for r in checks for k in ['policy', 'miniscript'])
    (root/'replay-trace-checks.json').write_text(json.dumps(checks, indent=2)+'\n')
    with (root/'replay-reference-responses.jsonl').open('x') as stream:
        for row in targets:
            stream.write(json.dumps(row)+'\n')
    with (root/'replay-reference-grade.log').open('x') as log:
        subprocess.run(['target/release/btc-bench', 'grade', '--dataset', 'datasets/broad-v1-training',
            '--responses', str(root/'replay-reference-responses.jsonl'), '--out', str(root/'replay-reference-graded'),
            '--standard-mode'], stdout=log, stderr=subprocess.STDOUT, check=True)
    grades = json.loads((root/'replay-reference-graded/results.json').read_text())
    assert len(grades) == 128 and all(r['score'] == 1 and not r.get('failure') for r in grades)
    tokenizer = AutoTokenizer.from_pretrained('runs/sft-wallet-v1/merged-step128')
    prepared = prepare_replay(output, tokenizer)
    dest = Path('datasets/sft-wallet-rl-v2-replay.jsonl')
    with dest.open('x') as stream:
        for row in output:
            stream.write(json.dumps(row)+'\n')
    report = dict(rows=len(output), questions=len(targets), data=str(dest), sha256=digest(dest),
        counts=dict(Counter(('write' if r['task_id'].startswith('t1-') else 'tree')+'/'+r['interface'] for r in output)),
        max_tokens=max(len(r['input_ids']) for r in prepared), seed=7,
        verification='Policies equal fixture references. Write Miniscript passes independent semantic checks. '
                     'Tree trace descriptors equal strictly graded final references. Both final interfaces checked.',
        provenance=[dict(task_id=r['task_id'], interface=r['interface']) for r in output])
    (root/'replay-data.json').write_text(json.dumps(report, indent=2)+'\n')
    print(json.dumps({k:v for k,v in report.items() if k!='provenance'}, indent=2))


if __name__ == '__main__':
    main()
