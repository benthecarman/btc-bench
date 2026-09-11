"""Report the frozen wallet pass with paired results and complete denominators."""
from collections import Counter
import hashlib
import json
from pathlib import Path


def digest(path):
    with Path(path).open('rb') as f: return hashlib.file_digest(f,'sha256').hexdigest()


def rows(path):
    return list(map(json.loads, Path(path).read_text().splitlines()))


def main():
    root = Path('runs/sft-wallet-v1')
    plan = json.loads(Path('training/wallet-sft-v1-experiment.json').read_text())
    status = json.loads((root/'status.json').read_text())
    assert status['phase']=='Wallet SFT training and evaluation complete; original server restored'
    assert 'error' not in status
    for path,expected in plan['hashes'].items(): assert digest(path)==expected,path
    assert digest(Path(plan['parent'])/'model.safetensors')==plan['parent_weight_sha256']
    assert digest(root/'merged-step128/model.safetensors')==status['trained_weight_sha256']
    audit = json.loads((root/'loss-mask-audit.json').read_text()); assert len(audit)==1024
    source = rows(plan['data'])
    changes=Counter()
    for original,prepared in zip(source,audit):
        target=prepared['supervised_text']
        if target==original['completion']:changes['unchanged']+=1
        elif target==original['completion']+'<|im_end|>':changes['added_eos_only']+=1
        else: raise AssertionError('Unexpected trainer target change')
    report = {'plan_sha256':digest('training/wallet-sft-v1-experiment.json'),
              'trained_model':str(root/'merged-step128'),'weight_sha256':status['trained_weight_sha256'],
              'loss_mask_rows':len(audit),'trainer_target_changes':dict(changes),
              'max_training_tokens':max(x['prompt_tokens']+x['supervised_tokens'] for x in audit),
              'wallet':{},'retention':{},'finish_reasons':Counter()}
    lines=['One fixed wallet SFT pass completed from broad checkpoint 528: 1,024 rows and 128 updates. No checkpoint was selected from these results. All targets passed actual trainer loss-mask checks. The original serving model was restored.\n',
           '| Wallet set | Interface | Output | Parent | After SFT | Gained / lost |',
           '|---|---|---|---:|---:|---:|']
    for suite in ['wallet-policy-v1','wallet-sft-v1-reserved']:
        fixtures = rows(f'datasets/{suite}/fixtures.jsonl')
        byid={f['id']:f for f in fixtures}
        report['wallet'][suite]={}
        for mode in ['chat','submit']:
            data={}
            for label in ['parent','step128']:
                d=Path(f'runs/wallet-policy-v1-baseline/{mode}') if label=='parent' and suite=='wallet-policy-v1' else root/f'{label}-{suite}-{mode}'
                raw=rows(d/'responses.jsonl')
                assert len(raw)==len(fixtures) and {r['task_id'] for r in raw}==set(byid)
                grades=json.loads((d/'graded/results.json').read_text())
                assert len(grades)==len(fixtures) and {r['task_id'] for r in grades}==set(byid)
                data[label]={r['task_id']:r['score']==1 for r in grades}
                if label=='step128' or suite=='wallet-sft-v1-reserved': report['finish_reasons'].update(r['finish_reason'] for r in raw)
            cells={}
            for kind in ['template','concrete']:
                ids=[f['id'] for f in fixtures if f['output_kind']==kind]
                before,after=data['parent'],data['step128']
                cell=dict(total=len(ids),parent=sum(before[i] for i in ids),trained=sum(after[i] for i in ids),
                          gains=sum(after[i] and not before[i] for i in ids),losses=sum(before[i] and not after[i] for i in ids))
                cells[kind]=cell
                lines.append(f"| {suite} | {mode} | {kind} | {cell['parent']}/{cell['total']} | {cell['trained']}/{cell['total']} | {cell['gains']} / {cell['losses']} |")
            if suite=='wallet-sft-v1-reserved':
                cells['by_family']={family:{label:sum(values[f['id']] for f in fixtures if f['family']==family) for label,values in data.items()} | {'total':sum(f['family']==family for f in fixtures)} for family in ['familiar','new-composition']}
            report['wallet'][suite][mode]=cells
    lines += ['\nReserved groups, combining both output forms (related questions):\n','| Group | Interface | Parent | After SFT |','|---|---|---:|---:|']
    for mode,data in report['wallet']['wallet-sft-v1-reserved'].items():
        for family,cell in data['by_family'].items():lines.append(f"| {family} | {mode} | {cell['parent']}/{cell['total']} | {cell['step128']}/{cell['total']} |")
    lines += ['\nRetention on previously observed development sets:\n','| Set | Task | Interface | Parent | After SFT | Gained / lost |','|---|---|---|---:|---:|---:|']
    for suite in plan['retention_suites']:
        fixtures=rows(f'datasets/{suite}/fixtures.jsonl')
        report['retention'][suite]={}
        for mode in ['chat','submit']:
            before=status['development']['parent'][suite][mode]
            after=status['development']['step128'][suite][mode]
            report['finish_reasons'].update(after['finish_reasons'])
            corrects=[]
            for run in [before,after]:
                grades={r['task_id']:r for r in json.loads(Path(run['directory'],'graded/results.json').read_text())}
                corrects.append({f['id']:f['id'] in grades and grades[f['id']].get('failure') in [None,'unimproved'] for f in fixtures})
            report['retention'][suite][mode]={}
            for kind in before['summary']:
                b,a=before['summary'][kind],after['summary'][kind]
                ids=[f['id'] for f in fixtures if f['task']==kind]
                gains=sum(corrects[1][i] and not corrects[0][i] for i in ids)
                losses=sum(corrects[0][i] and not corrects[1][i] for i in ids)
                report['retention'][suite][mode][kind]=dict(parent=b['correct'],trained=a['correct'],total=a['total'],gains=gains,losses=losses)
                lines.append(f"| {suite} | {kind} | {mode} | {b['correct']}/{b['total']} | {a['correct']}/{a['total']} | {gains} / {losses} |")
    report['finish_reasons']=dict(report['finish_reasons'])
    lines += ['\nNew generation finish reasons: `'+json.dumps(report['finish_reasons'])+'`. Missing answers and context-limit outputs remain in the denominator.',
              '\nThe reserved check contains 16 authored scenarios, not 64 independent examples. Half use familiar structures. The other half are absent from this new training mix under normalized comparison; absence from all parent training is not established. This is a small synthetic test, not a broad human benchmark.',
              '\nThe wallet inference contract and runner are unchanged from the wallet baseline. The legacy tests retain their own original runner. Results across those interfaces measure different contracts.',
              '\nReasoning checks are saved separately in `runs/sft-wallet-v1/trace-audit.json`. Correct final answers alone do not validate reasoning. The teaching traces were checked separately before evaluation.',
              '\nModel: `'+report['trained_model']+'`. Weight SHA-256: `'+report['weight_sha256']+'`.']
    with (root/'results.json').open('x') as f:json.dump(report,f,indent=2);f.write('\n')
    with Path('training/wallet-sft-v1-results.md').open('x') as f:f.write('\n'.join(lines)+'\n')
    examples=[]
    for suite,ident in [('wallet-policy-v1','wp-native-single-0-template'),('wallet-policy-v1','wp-recovery-delay-1-template'),
                        ('wallet-policy-v1','wp-joint-councils-0-concrete'),('wallet-sft-v1-reserved','wr1-owner-last-template'),
                        ('wallet-sft-v1-reserved','wr1-age-and-height-template'),('wallet-sft-v1-reserved','wr1-quorum-or-joint-template')]:
        f=next(f for f in rows(f'datasets/{suite}/fixtures.jsonl') if f['id']==ident)
        examples += [f"**{ident}**\n\n{f['request']}\n\nReference policy: `{f['policy_template']}`\n"]
        for mode in ['chat','submit']:
            d=root/f'step128-{suite}-{mode}'
            row=next(r for r in rows(d/'responses.jsonl') if r['task_id']==ident)
            grade=next(r for r in json.loads((d/'graded/results.json').read_text()) if r['task_id']==ident)
            message=row['raw_response']['choices'][0]['message']
            text=row.get('text') if mode=='chat' else row.get('answer')
            examples += [f"{mode}: score {grade['score']}; finish {row['finish_reason']}; reason {grade['reason']}\n\n```text\n{text or '(no final answer)'}\n```\n"]
            reasoning=message.get('reasoning') or message.get('reasoning_content') or ''
            examples += ['Reasoning (first 3,000 characters; full record in run directory):\n\n```text\n'+reasoning[:3000]+'\n```\n']
    with Path('training/wallet-sft-v1-spot-check.md').open('x') as f:f.write('\n'.join(examples))
    print('\n'.join(lines))


if __name__=='__main__':main()
