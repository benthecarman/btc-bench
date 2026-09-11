"""Save readable training examples and freeze the validated data inputs."""
from datetime import datetime, timezone
import hashlib
import json
from pathlib import Path


def digest(path):
    with Path(path).open('rb') as f:
        return hashlib.file_digest(f,'sha256').hexdigest()


def main():
    sources = json.loads(Path('training/broad-v1-training.json').read_text())['cases']
    fixtures = {r['id']:r for r in map(json.loads,Path('datasets/broad-v1-training/fixtures.jsonl').read_text().splitlines())}
    mix = json.loads(Path('training/broad-v1-mix.json').read_text())
    verify = json.loads(Path('training/broad-v1-verification.json').read_text())
    results = json.loads(Path('runs/broad-v1-chat-reference-check/graded/results.json').read_text())
    assert len(results)==512 and all(r['score']==1 and not r.get('failure') for r in results)
    assert not Path('runs/broad-v1-chat-reference-check/failures.jsonl').read_text().strip()
    lines = ['# Broader curriculum: training examples','',
             'These eight examples come from training. Reserved questions are not shown here. '
             'Requests below omit their public-key appendix; the actual model prompt includes it. '
             'Public keys in reference expressions are replaced with request labels for review. '
             'These labelled expressions are explanatory, not runnable scripts.','']
    for family in dict.fromkeys(c['family'] for c in sources):
        kind = 'tree' if family in ['team-recovery','staged-quorums','global-secret'] else 'write'
        case = next(c for c in sources if c['family']==family and c['kind']==kind)
        tid = ('t1-human-' if kind=='write' else 't4-human-')+case['id']
        r = fixtures[tid]
        def labelled(s):
            for k in r['keys']:
                s=s.replace(k['pubkey'],k['label'])
            return s
        lines += ['## '+family+' ('+kind+')','','`'+tid+'`','',r['spec_en'],'',
                  'Reference policy:','','```text',labelled(r['reference_policy']),'```','',
                  'Reference '+('Miniscript' if kind=='write' else 'descriptor')+':','','```text',
                  labelled(r['reference_miniscript'] if kind=='write' else r['reference_descriptor']),'```','']
    Path('training/broad-v1-examples.md').write_text('\n'.join(lines))
    paths = ['training/broad-v1-training.json','training/broad-v1-reserved.json','training/broad-v1-mix.json',
             'datasets/broad-v1-training/fixtures.jsonl','datasets/broad-v1-reserved/fixtures.jsonl',mix['data'],
             'training/build_broad_v1.py','training/prepare_broad_v1.py','training/verify_broad_v1.py',
             'crates/bench-gen/src/human.rs','crates/bench-cli/examples/compile_training_catalog.rs','Cargo.lock']
    freeze = dict(created_utc=datetime.now(timezone.utc).isoformat(),status='data ready; no model training or reserved-set inference in this preparation',
                  intended_parent='runs/sft-compound-v3/merged-step144',
                  parent_weight_sha256=digest('runs/sft-compound-v3/merged-step144/model.safetensors'),
                  hashes={p:digest(p) for p in paths},rows=mix['rows'],max_tokens=mix['max_tokens'],
                  reference_checks=verify,chat_reference_passes=len(results),
                  reserved_use='Only reference verification so far. Keep out of training and checkpoint selection; evaluate after the model and settings are fixed.',
                  supervision='All target token boundaries and complete-target round trips checked; the actual trainer loss-mask audit must run before the next training starts.')
    with Path('training/broad-v1-freeze.json').open('x') as f:
        json.dump(freeze,f,indent=2);f.write('\n')
    print(json.dumps({k:v for k,v in freeze.items() if k!='hashes'},indent=2))


if __name__=='__main__':
    main()
