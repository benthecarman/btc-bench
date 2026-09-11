#!/usr/bin/env python3
"""Run the evaluation-only wallet pilot without grader feedback or generation caps."""
import argparse
from concurrent.futures import ThreadPoolExecutor, as_completed
import hashlib
import json
from pathlib import Path
import time
import urllib.error
import urllib.request

SYSTEM = ('Solve the following Bitcoin Script task. Decide your answer, then submit it by calling the submit tool exactly '
          'once. You are in an automated pipeline: there is no one to ask, so do not ask questions.')
TOOL = {'type': 'function', 'function': {'name': 'submit_descriptor',
        'description': 'Submit the descriptor template or concrete descriptor requested by the user.',
        'parameters': {'type': 'object', 'properties': {'descriptor': {'type': 'string'}}, 'required': ['descriptor']}}}


def submit_answer(message):
    calls = message.get('tool_calls') or []
    if len(calls) != 1 or calls[0].get('function', {}).get('name') != 'submit_descriptor':
        raise ValueError('Expected exactly one submit_descriptor call')
    args = json.loads(calls[0]['function']['arguments'])
    if not isinstance(args, dict) or not isinstance(args.get('descriptor'), str):
        raise ValueError('Submit arguments must contain a descriptor string')
    return args['descriptor']


def request_body(fixture, model, mode):
    result = {'model': model, 'messages': [
        {'role': 'system', 'content': 'You are a helpful assistant.' if mode == 'chat' else SYSTEM},
        {'role': 'user', 'content': fixture['request']}],
        'temperature': 0.6, 'top_p': 0.95, 'top_k': 20, 'min_p': 0.0,
        'seed': 20260904, 'chat_template_kwargs': {'enable_thinking': True}, 'stream': False}
    if mode == 'submit':
        result.update(tools=[TOOL], tool_choice='auto')
    return result


def complete(fixture, args):
    body = request_body(fixture, args.model, args.mode)
    req = urllib.request.Request(args.base_url.rstrip('/') + '/chat/completions',
                                 data=json.dumps(body).encode(), headers={'Content-Type': 'application/json'})
    started = time.time()
    for attempt in range(5):
        try:
            # The server context is the only output bound. Do not add a socket
            # deadline that silently turns a long generation into a test failure.
            with urllib.request.urlopen(req) as response:
                raw = json.load(response)
            break
        except urllib.error.HTTPError as error:
            if error.code not in (429, 500, 502, 503, 504) or attempt == 4:
                raise
            time.sleep(min(2 ** attempt, 8))
    choice = raw['choices'][0]
    message = choice['message']
    row = {'task_id': fixture['id'], 'finish_reason': choice.get('finish_reason'),
           'output_tokens': raw.get('usage', {}).get('completion_tokens'),
           'elapsed_seconds': time.time() - started, 'raw_response': raw}
    if args.mode == 'chat':
        row['text'] = message.get('content') or ''
    else:
        try:
            row['answer'] = submit_answer(message)
        except (ValueError, KeyError, TypeError) as error:
            row['error'] = str(error)
    return row


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument('--dataset', type=Path, required=True)
    ap.add_argument('--out', type=Path, required=True)
    ap.add_argument('--model', required=True)
    ap.add_argument('--mode', choices=['chat', 'submit'], required=True)
    ap.add_argument('--base-url', default='http://127.0.0.1:8010/v1')
    args = ap.parse_args()
    if args.out.exists():
        ap.error('output exists; preserve the earlier run')
    data = (args.dataset / 'fixtures.jsonl').read_bytes()
    manifest = json.loads((args.dataset / 'manifest.json').read_text())
    assert manifest['evaluation_only'] is True and manifest['schema'] == 'btc-wallet-v1'
    digest = hashlib.sha256(data).hexdigest()
    assert manifest['fixtures_sha256'] == digest
    fixtures = list(map(json.loads, data.decode().splitlines()))
    assert len({f['id'] for f in fixtures}) == len(fixtures)
    with urllib.request.urlopen(args.base_url.rstrip('/') + '/models') as response:
        assert [m['id'] for m in json.load(response)['data']] == [args.model]
    args.out.mkdir(parents=True)
    metadata = {'dataset': str(args.dataset), 'fixture_sha256': digest, 'model': args.model, 'mode': args.mode,
                'concurrency': 4, 'generation_cap': None, 'time_cap': None, 'retries': 4,
                'settings': {k: v for k, v in request_body(fixtures[0], args.model, args.mode).items() if k != 'messages'},
                'transport': 'OpenAI-compatible JSON, non-streaming; no text-tool fallback',
                'started_at_unix': time.time()}
    (args.out / 'run.json').write_text(json.dumps(metadata, indent=2) + '\n')
    with (args.out / 'requests.jsonl').open('x') as stream:
        for f in fixtures:
            stream.write(json.dumps({'task_id': f['id'], 'request': request_body(f, args.model, args.mode)}) + '\n')
    with (args.out / 'responses.jsonl').open('x') as stream, ThreadPoolExecutor(max_workers=4) as pool:
        futures = {pool.submit(complete, f, args): f for f in fixtures}
        for i, future in enumerate(as_completed(futures), 1):
            row = future.result()
            stream.write(json.dumps(row) + '\n'); stream.flush()
            print(f"{i}/{len(fixtures)} {row['task_id']} {row['finish_reason']}", flush=True)
    (args.out / 'complete.json').write_text(json.dumps({'finished_at_unix': time.time(), 'answers': len(fixtures)}) + '\n')


if __name__ == '__main__':
    main()
