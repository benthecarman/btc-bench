"""Prepare checked wallet targets and paired replay; reject evaluation fixtures."""
from collections import Counter, defaultdict
import hashlib
import json
from pathlib import Path
import random
import re
import sys
sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'scripts'))
from run_wallet_bench import request_body
from audit_human_catalog import shape


def rows(path):
    return list(map(json.loads, Path(path).read_text().splitlines()))


def digest(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def write(path, value):
    with Path(path).open('x') as f:
        json.dump(value, f, indent=2); f.write('\n')


def prepare_wallet(f, trace, tok, mode):
    assert f['split'] == 'training', 'Evaluation fixture cannot be exported'
    assert f['id'] == trace['task_id']
    body = request_body(f, 'unused', mode)
    opts = {'tools': body['tools']} if mode == 'submit' else {}
    prompt = tok.apply_chat_template(body['messages'], **opts, add_generation_prompt=True,
                                     enable_thinking=True, tokenize=False)
    assert prompt.endswith('<|im_start|>assistant\n')
    representation = ('Use BIP-388 key placeholders with /** for receive/change derivation.' if f['output_kind'] == 'template'
                      else 'Use the supplied derived public keys directly in the descriptor.')
    trace_text = representation + '\nAs a policy that is: ' + trace['policy']
    if trace['bodies']:
        trace_text += '\n' + trace['context'] + ':\n' + '\n'.join(trace['bodies'])
    else:
        trace_text += '\n' + trace['context'] + '.'
    if f['reference_template'].startswith('tr('):
        trace_text += '\nThe Taproot internal key supplies the unrestricted owner route; each leaf supplies an alternative route.' if trace['bodies'] else '\nUse the sole owner key as the Taproot internal key, with no script tree.'
    label = 'Descriptor template' if f['output_kind'] == 'template' else 'Concrete descriptor'
    chat_final = label + ':\n\n```text\n' + trace['answer'] + '\n```'
    final = chat_final if mode == 'chat' else '<tool_call>\n' + json.dumps({'name': 'submit_descriptor', 'arguments': {'descriptor': trace['answer']}}) + '\n</tool_call>'
    completion = '<think>\n' + trace_text + '\n</think>\n\n' + final + '<|im_end|>'
    return {'prompt': prompt, 'completion': completion}, chat_final


def main():
    from transformers import AutoTokenizer
    parent = 'runs/sft-broad-v1/merged-step528'
    tok = AutoTokenizer.from_pretrained(parent)
    root = Path('datasets/wallet-sft-v1-training')
    manifest = json.loads((root / 'manifest.json').read_text())
    assert manifest['evaluation_only'] is False
    assert digest(root / 'fixtures.jsonl') == manifest['fixtures_sha256']
    fixtures = rows(root / 'fixtures.jsonl')
    traces = json.loads((root / 'traces.json').read_text())
    assert len(fixtures) == len(traces) == 256
    dev = rows('datasets/wallet-policy-v1/fixtures.jsonl')
    reserved = rows('datasets/wallet-sft-v1-reserved/fixtures.jsonl')
    assert not {f['id'] for f in fixtures} & {f['id'] for f in dev + reserved}
    assert not {f['spec_en'] for f in fixtures} & {f['spec_en'] for f in dev + reserved}
    # Shape removes key identities, normalizes commutative operators and flattens
    # AND/OR. It preserves threshold counts and clock domains; not a proof of novelty.
    policy_shape = lambda p: shape(re.sub(r'@\d+', lambda m: ('02' + str(int(m[0][1:])+1).zfill(64)), p))
    heldout = {policy_shape(f['policy_template']) for f in reserved if f['family'] == 'new-composition'}
    assert not heldout & {policy_shape(f['policy_template']) for f in fixtures}
    output, targets = [], []
    for f, trace in zip(fixtures, traces):
        for mode in ['chat', 'submit']:
            pair, final = prepare_wallet(f, trace, tok, mode)
            output.append(dict(source='wallet', source_index=f['id'], interface=mode, kind=f['output_kind'], row=pair))
            if mode == 'chat':
                targets.append(dict(task_id=f['id'], text=final, finish_reason='reference'))
    prior = rows('datasets/sft-broad-v1-mixed.jsonl')
    provenance = json.loads(Path('training/broad-v1-mix.json').read_text())['row_provenance']
    assert len(prior) == len(provenance)
    pairs = defaultdict(dict)
    for row, item in zip(prior, provenance):
        pairs[(item['source'], item['source_index'], item['kind'])][item['interface']] = row
    rng = random.Random(202609071)
    chosen, rejected = [], Counter()
    for source, kind, count in [('broad', 'write', 64), ('broad', 'tree', 64),
                                 ('compound_replay', 'write', 64), ('original_replay', 'script', 32),
                                 ('original_replay', 'tree', 16), ('original_replay', 'identify', 16)]:
        candidates = [(key, pair) for key, pair in pairs.items() if key[0] == source and key[2] == kind]
        rng.shuffle(candidates)
        accepted = []
        for key, pair in candidates:
            assert set(pair) == {'chat', 'submit'}
            if kind != 'identify':
                match = re.search(r'^(?:As a policy that is|Written as a policy|That gives the policy|Its spending policy is): (.+)$', pair['submit']['completion'], re.M)
                assert match
                if shape(match[1]) in heldout:
                    rejected['reserved structure in replay'] += 1; continue
            accepted.append((key, pair))
            if len(accepted) == count: break
        assert len(accepted) == count, (source, kind, len(accepted))
        chosen.extend(accepted)
    for (source, index, kind), pair in chosen:
        for mode, row in pair.items():
            output.append(dict(source=source, source_index=index, kind=kind, interface=mode, row=row))
    assert len(output) == 1024
    rng.shuffle(output)
    lengths = []
    for item in output:
        row = item['row']
        all_ids = tok.encode(row['prompt'] + row['completion'], add_special_tokens=False)
        prompt_ids = tok.encode(row['prompt'], add_special_tokens=False)
        assert all_ids[:len(prompt_ids)] == prompt_ids
        assert tok.decode(all_ids[len(prompt_ids):]) == row['completion']
        assert len(all_ids) <= 4096, (item['source'], item['source_index'], len(all_ids))
        lengths.append(len(all_ids))
    dest = 'datasets/sft-wallet-v1-mixed.jsonl'
    with Path(dest).open('x') as stream:
        for item in output: stream.write(json.dumps(item['row']) + '\n')
    with Path('datasets/wallet-sft-v1-chat-targets.jsonl').open('x') as stream:
        for item in targets: stream.write(json.dumps(item) + '\n')
    report = dict(data=dest, data_sha256=digest(dest), rows=len(output), new_wallet_scenarios=128,
                  wallet_rows=512, replay_rows=512, max_tokens=max(lengths), mean_tokens=sum(lengths)/len(lengths),
                  counts=dict(Counter(x['source']+'/'+x['kind']+'/'+x['interface'] for x in output)),
                  seed=202609071, rejected=dict(rejected), reserved_new_composition_shape_matches=0,
                  split_limit='New-composition exclusion applies to this run, not all historical parent training. Familiar test structures intentionally overlap.',
                  row_provenance=[{k:v for k,v in item.items() if k!='row'} for item in output])
    write('training/wallet-sft-v1-mix.json', report)
    examples = []
    for family in ['native-single', 'recovery-delay', 'joint-councils', 'alternative-recovery']:
        f = next(f for f in fixtures if f['family'] == family and f['output_kind'] == 'template')
        t = next(t for t in traces if t['task_id'] == f['id'])
        examples += [f"**{f['id']}**\n\n{f['request']}\n\nPolicy: `{t['policy']}`\n\n{t['context']}:\n\n" + '\n'.join('`'+b+'`' for b in t['bodies']) + f"\n\nAnswer:\n\n```text\n{t['answer']}\n```\n"]
    with Path('training/wallet-sft-v1-examples.md').open('x') as f: f.write('\n'.join(examples))
    print(json.dumps({k:v for k,v in report.items() if k!='row_provenance'}, indent=2))


if __name__ == '__main__':
    main()
