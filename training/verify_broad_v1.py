"""Check all compiled references with strict grading and enforce split separation."""
import hashlib
import json
from pathlib import Path
import subprocess
import sys

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'scripts'))
from audit_human_catalog import shape


def main():
    report = {}
    shape_sets = {}
    for split in ['training','reserved']:
        pool = Path(f'datasets/broad-v1-{split}')
        cases = json.loads(Path(f'training/broad-v1-{split}.json').read_text())['cases']
        fixtures = [json.loads(l) for l in (pool/'fixtures.jsonl').read_text().splitlines()]
        assert len(cases) == len(fixtures)
        shape_sets[split] = {shape(c['policy']) for c in cases}
        assert len(shape_sets[split]) == len(cases)
        out = Path(f'runs/broad-v1-{split}-reference-check')
        out.mkdir()
        with (out/'responses.jsonl').open('x') as f:
            for r in fixtures:
                answer = dict(task='script',script=r['reference_script_hex']) if r['task']=='write' else dict(task='descriptor',descriptor=r['reference_descriptor'])
                f.write(json.dumps(dict(task_id=r['id'],answer=answer))+'\n')
        with (out/'grade.log').open('x') as log:
            subprocess.run(['target/release/btc-bench','grade','--dataset',str(pool),'--responses',str(out/'responses.jsonl'),
                            '--out',str(out/'graded'),'--standard-mode'],stdout=log,stderr=subprocess.STDOUT,check=True)
        results = json.loads((out/'graded/results.json').read_text())
        assert {r['task_id'] for r in results} == {r['id'] for r in fixtures}
        failures = [r for r in results if r['score']!=1 or r.get('failure')]
        assert not failures, failures[:3]
        notes = json.loads((pool/'reference-notes.json').read_text())
        report[split] = dict(questions=len(fixtures),groups=len({c['group'] for c in cases}),
                             distinct_shapes=len(shape_sets[split]),strict_reference_passes=len(results),
                             fallback_references=sum(bool(v) for v in notes.values()),
                             fixture_sha256=hashlib.sha256((pool/'fixtures.jsonl').read_bytes()).hexdigest())
    assert not shape_sets['training'] & shape_sets['reserved']
    train = json.loads(Path('training/broad-v1-training.json').read_text())['cases']
    reserved = json.loads(Path('training/broad-v1-reserved.json').read_text())['cases']
    assert not {c['family'] for c in train} & {c['family'] for c in reserved}
    for command in [
        ['target/release/btc-bench','sft-export','--dataset','datasets/broad-v1-reserved','--out','runs/broad-v1-forbidden-export.jsonl'],
        [sys.executable,'scripts/rl_prepare.py','--pool','datasets/broad-v1-reserved','--out','runs/broad-v1-forbidden-rl.jsonl']
    ]:
        r = subprocess.run(command,capture_output=True,text=True)
        assert r.returncode != 0 and 'evaluat' in (r.stderr+r.stdout).lower(),r
    assert not Path('runs/broad-v1-forbidden-export.jsonl').exists()
    assert not Path('runs/broad-v1-forbidden-rl.jsonl').exists()
    report['evaluation_export_guards'] = 'SFT and RL both reject the reserved pool'
    Path('training/broad-v1-verification.json').write_text(json.dumps(report,indent=2)+'\n')
    print(json.dumps(report,indent=2))


if __name__=='__main__':
    main()
