"""Report the fixed longer-RL comparison, including SFT cost and trace checks."""
from collections import Counter
import json
from pathlib import Path
import statistics

from report_wallet_rl_v1 import correct, digest, rows
from check_wallet_sft_v1_traces import check, expand, extract


ROOT = Path('runs/rl-wallet-v2')
LABELS = ['parent'] + [f'{arm}-step{step}' for step in [64, 128, 256] for arm in ['rl', 'mixed']]


def directory(label, suite, mode):
    return (Path('runs/sft-wallet-v1')/f'step128-{suite}-{mode}' if label == 'parent'
            else ROOT/f'{label}-{suite}-{mode}')


def main():
    plan = json.loads(Path('training/wallet-rl-v2-experiment.json').read_text())
    status = json.loads((ROOT/'status.json').read_text())
    assert status['phase'] == 'Longer RL comparison complete; original server available'
    assert not status.get('error')
    for path, sha in plan['hashes'].items():
        assert digest(path) == sha, path
    report = dict(plan_sha256=digest('training/wallet-rl-v2-experiment.json'),
                  training={}, evaluation={}, weights=status['weights'])
    for weight in status['weights'].values():
        assert digest(Path(weight['path'])/'model.safetensors') == weight['sha256']
    for arm in ['rl', 'mixed']:
        samples = rows(ROOT/arm/'rollouts.jsonl')
        assert len(samples) == 256
        segments = []
        for lo, hi in [(0, 64), (64, 128), (128, 192), (192, 256)]:
            segment = samples[lo:hi]
            rewards = [[r['shaped'] for r in row['rewards']] for row in segment]
            assert all(len(g) == 8 for g in rewards)
            segments.append(dict(start=lo+1, end=hi, groups=len(segment),
                mixed=sum(statistics.pstdev(g)>1e-9 for g in rewards),
                mean_reward=statistics.mean(v for g in rewards for v in g),
                truncated=sum(sum(r['truncated']) for r in segment),
                completion_tokens=sum(sum(r['completion_lengths']) for r in segment)))
        history = json.loads((ROOT/arm/'checkpoint-256/trainer_state.json').read_text())['log_history']
        report['training'][arm] = dict(segments=segments, wall_seconds=status['training_seconds'][arm],
            optimizer_history=history, question_counts=dict(Counter(t for row in samples for t in row['task_ids'])))
        if arm == 'mixed':
            replay = rows(ROOT/arm/'sft-replay.jsonl')
            assert [r['step'] for r in replay] == list(range(1, 257))
            report['training'][arm]['replay'] = dict(examples=len(replay),
                supervised_tokens=sum(r['supervised_tokens'] for r in replay),
                total_tokens=sum(r['supervised_tokens']+r['prompt_tokens'] for r in replay),
                seconds=sum(r['seconds'] for r in replay), mean_loss=statistics.mean(r['loss'] for r in replay))
    lines = ['The 256-update RL and RL plus SFT replay runs completed. Both used the same SFT parent and RL question pool. '
             'The mixed run added one supervised earlier-task example per update, with loss weight 0.1. '
             'Step 256 is the fixed final comparison. Checkpoints 64 and 128 describe the learning curve.\n',
             'All scores below count correct final answers. Tree weight scores are stored separately in the JSON report. '
             'All suites are observed development data; paired interfaces and representations are not independent trials.\n',
             '| Development set | Parent | RL 64 | Mixed 64 | RL 128 | Mixed 128 | RL 256 | Mixed 256 |',
             '|---|---:|---:|---:|---:|---:|---:|---:|']
    totals = {}
    traces, trace_meta = [], []
    finish = Counter()
    for suite in plan['evaluation_suites']:
        fixtures = rows(f'datasets/{suite}/fixtures.jsonl')
        ids = {f['id'] for f in fixtures}
        wallet = suite.startswith('wallet-')
        cells = {}
        for mode in ['chat', 'submit']:
            by_label = {}
            for label in LABELS:
                d = directory(label, suite, mode)
                grades = json.loads((d/'graded/results.json').read_text())
                graded = {r['task_id']:r for r in grades}
                assert len(graded) == len(grades) and set(graded) <= ids
                raw = rows(d/('responses.jsonl' if wallet or mode == 'submit' else 'chat-text.jsonl'))
                if not wallet and mode == 'submit' and (d/'failures.jsonl').exists():
                    raw += rows(d/'failures.jsonl')
                assert len(raw) == len(ids) and {r['task_id'] for r in raw} == ids
                by_label[label] = {tid:correct(graded.get(tid), wallet) for tid in ids}
                if label != 'parent':
                    finish.update(r['finish_reason'] for r in raw)
                cell = dict(correct=sum(by_label[label].values()), total=len(ids),
                    mean_score=sum(graded.get(tid, {}).get('score', 0) for tid in ids)/len(ids),
                    gains=sum(by_label[label][tid] and not by_label['parent'][tid] for tid in ids),
                    losses=sum(by_label['parent'][tid] and not by_label[label][tid] for tid in ids))
                cells.setdefault(mode, {})[label] = cell
                if wallet and label in ['parent', 'rl-step256', 'mixed-step256']:
                    fs = {f['id']:f for f in fixtures}
                    for row in raw:
                        f = fs[row['task_id']]
                        message = row['raw_response']['choices'][0]['message']
                        policy, bodies = extract(message.get('reasoning') or message.get('reasoning_content') or '')
                        ident = f'{label}/{suite}/{mode}/{f["id"]}'
                        traces.append(dict(id=ident, fixture=f, policy=expand(f, policy),
                            bodies=[expand(f, body) for body in bodies] if bodies else None))
                        trace_meta.append(dict(id=ident, label=label, correct=by_label[label][f['id']]))
        report['evaluation'][suite] = cells
        totals[suite] = {label:dict(correct=sum(cells[mode][label]['correct'] for mode in cells),
                                  total=2*len(ids)) for label in LABELS}
        lines.append('| '+suite+' | '+' | '.join(f"{totals[suite][label]['correct']}/{totals[suite][label]['total']}" for label in LABELS)+' |')
    report['totals'] = totals
    report['finish_reasons'] = dict(finish)
    lines += ['\nTraining by 64-update segment:\n', '| Arm | Updates | Groups with reward variation | Mean reward | Truncated answers |',
              '|---|---|---:|---:|---:|']
    for arm, train in report['training'].items():
        for seg in train['segments']:
            lines.append(f"| {arm} | {seg['start']}–{seg['end']} | {seg['mixed']}/{seg['groups']} | {seg['mean_reward']:.3f} | {seg['truncated']} |")
    replay = report['training']['mixed']['replay']
    lines += [f"\nTraining process wall time: RL {report['training']['rl']['wall_seconds']/60:.1f} minutes; mixed "
              f"{report['training']['mixed']['wall_seconds']/60:.1f} minutes. These include model initialization and saving. "
              f"SFT added {replay['examples']} examples and {replay['supervised_tokens']:,} supervised tokens. "
              'The arms are not matched by compute or data exposure.\n']
    checked = check(traces)
    assert len(checked) == len(trace_meta)
    trace_rows = [dict(**m, policy=r['policy'], bodies=r['bodies']) for m,r in zip(trace_meta, checked)]
    summaries = {}
    for label in ['parent', 'rl-step256', 'mixed-step256']:
        rs = [r for r in trace_rows if r['label'] == label]
        summaries[label] = dict(outputs=len(rs), correct=sum(r['correct'] for r in rs),
            policy_counts=dict(Counter(r['policy']['status'] for r in rs)),
            correct_with_bad_policy=sum(r['correct'] and r['policy']['status'] in ['invalid','not equivalent'] for r in rs),
            correct_with_bad_bodies=sum(r['correct'] and r['bodies']['status']=='invalid or not equivalent' for r in rs))
    report['trace_summary'] = summaries
    (ROOT/'trace-audit.json').write_text(json.dumps(dict(summary=summaries, rows=trace_rows,
        scope='Explicit marked wallet expressions only. No syntax repair. Body checks use the reference internal key. '
              'Missing expressions are unmeasured. This does not change final grades.'), indent=2)+'\n')
    lines += ['Final wallet trace diagnostics:\n', '| Model | Correct final with wrong/invalid policy | Correct final with wrong/invalid bodies |',
              '|---|---:|---:|']
    for label, value in summaries.items():
        lines.append(f"| {label} | {value['correct_with_bad_policy']} | {value['correct_with_bad_bodies']} |")
    lines += ['\nTrace checks cover explicit marked expressions only; missing text is unmeasured. Body checks use the reference internal key. '
              'They are diagnostics and do not affect final grades.\n',
              'No model was promoted. The original model server was restored. Raw generations, checkpoints, replay masks and training histories are preserved in `runs/rl-wallet-v2/`.']
    (ROOT/'results.json').write_text(json.dumps(report, indent=2)+'\n')
    Path('training/wallet-rl-v2-results.md').write_text('\n'.join(lines)+'\n')
    print('\n'.join(lines))


if __name__ == '__main__':
    main()
