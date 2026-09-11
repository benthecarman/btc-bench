"""Render matched chat and submit training pairs, with equal original replay."""
import hashlib
import json
from pathlib import Path
import random
import re
import sys

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'scripts'))
from rl_common import extract_answer


def user_text(row, chat=False):
    matches = re.findall(r'<\|im_start\|>user\n(.*?)<\|im_end\|>', row['prompt'], re.S)
    assert len(matches) == 1
    text = matches[0]
    if chat:
        text = text.replace('Call the submit_identify tool with one of the following labels:',
                            'Reply with one of the following labels:')
    return text


def chat_pair(row, tok, new=False):
    answer = extract_answer(row['completion'], thinking=True)
    assert isinstance(answer, dict)
    field = {'script': 'script', 'descriptor': 'descriptor', 'identify': 'label'}[answer['task']]
    if field == 'label':
        final = answer[field]
    else:
        label = 'P2WSH witness script' if new else ('Taproot descriptor' if field == 'descriptor' else 'Bitcoin script')
        final = label + ':\n\n```text\n' + answer[field] + '\n```'
    think = row['completion'].split('</think>', 1)[0]
    if not think.startswith('<think>'):
        assert row['prompt'].endswith('<think>\n')
        think = '<think>\n' + think
    prompt = tok.apply_chat_template([{'role': 'system', 'content': 'You are a helpful assistant.'},
                                     {'role': 'user', 'content': user_text(row, chat=True)}],
                                    add_generation_prompt=True, enable_thinking=True, tokenize=False)
    assert prompt.endswith('<|im_start|>assistant\n')
    completion = think + '</think>\n\n' + final + '<|im_end|>'
    return dict(prompt=prompt, completion=completion), final


def main():
    from transformers import AutoTokenizer
    parent = 'runs/sft-qwen3-4b-think/merged'
    tok = AutoTokenizer.from_pretrained(parent)
    paths = [Path('datasets/sft-compound-v3-submit.jsonl'), Path('datasets/sft-train-think.jsonl')]
    new, old = [list(map(json.loads, p.read_text().splitlines())) for p in paths]
    fixtures = list(map(json.loads, Path('datasets/rl-compound-v3-train.jsonl').read_text().splitlines()))
    assert len(new) == len(fixtures) == 288
    rng = random.Random(20260914)
    selected = []
    for name, count in [('submit_script', 192), ('submit_descriptor', 72), ('submit_identify', 24)]:
        candidates = [i for i, row in enumerate(old) if f'"name": "{name}"' in row['completion']]
        rng.shuffle(candidates)
        seen, picked = set(), []
        for i in candidates:
            identity = json.dumps(old[i], sort_keys=True)
            text = user_text(old[i], chat=True)
            if 'submit_' in text or 'submit tool' in text.lower():
                continue
            chat, _ = chat_pair(old[i], tok)
            if max(len(tok.encode(r['prompt'] + r['completion'], add_special_tokens=False)) for r in [old[i], chat]) > 4096:
                continue
            if identity in seen:
                continue
            seen.add(identity)
            picked.append(i)
            if len(picked) == count:
                break
        assert len(picked) == count, (name, len(picked), count)
        selected.extend(picked)
    base = [dict(source='compound', index=i, row=r) for i, r in enumerate(new)]
    base += [dict(source='replay', index=i, row=old[i]) for i in selected]
    output, chat_records = [], []
    for r in base:
        chat, final = chat_pair(r['row'], tok, r['source'] == 'compound')
        for interface, row in [('submit', r['row']), ('chat', chat)]:
            output.append(dict(source=r['source'], index=r['index'], interface=interface, row=row))
        if r['source'] == 'compound':
            task = fixtures[r['index']]
            assert task['prompt'] == r['row']['prompt']
            chat_records.append(dict(task_id=task['task_id'], text=final, finish_reason='stop'))
    rng.shuffle(output)
    lengths = [len(tok.encode(r['row']['prompt'] + r['row']['completion'], add_special_tokens=False)) for r in output]
    assert len(output) == 1152 and max(lengths) <= 4096
    path = Path('datasets/sft-compound-v3-mixed.jsonl')
    with path.open('x') as f:
        for r in output:
            f.write(json.dumps(r['row']) + '\n')
    with Path('datasets/compound-v3-chat-targets.jsonl').open('x') as f:
        for r in chat_records:
            f.write(json.dumps(r) + '\n')
    metadata = dict(data=str(path), data_sha256=hashlib.sha256(path.read_bytes()).hexdigest(),
                    sources={str(p): hashlib.sha256(p.read_bytes()).hexdigest() for p in paths},
                    unique_compound=288, unique_replay=288, chat_rows=576, submit_rows=576, total_rows=1152,
                    max_tokens=max(lengths), mean_tokens=sum(lengths)/len(lengths), seed=20260914,
                    chat_system='You are a helpful assistant.',
                    chat_identification_instruction='Reply with one of the following labels:',
                    row_provenance=[{k: v for k, v in r.items() if k != 'row'} for r in output])
    with Path('training/compound-v3-mix.json').open('x') as f:
        json.dump(metadata, f, indent=2)
        f.write('\n')
    print(json.dumps({k: v for k, v in metadata.items() if k != 'row_provenance'}, indent=2))


if __name__ == '__main__':
    main()
