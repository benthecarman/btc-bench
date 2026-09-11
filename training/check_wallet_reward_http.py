"""Exercise rendered submit completions through the real HTTP reward endpoint."""
import argparse
import json
from pathlib import Path
import sys
sys.path.insert(0,str(Path(__file__).resolve().parents[1]/'scripts'))
import rl_train


def main():
    ap=argparse.ArgumentParser();ap.add_argument('--url',required=True);args=ap.parse_args()
    rl_train.reward_url=args.url
    rows=list(map(json.loads,Path('datasets/rl-wallet-v1-probe.jsonl').read_text().splitlines()))
    gold={r['task_id']:r['row'] for r in map(json.loads,Path('datasets/rl-wallet-v1-gold.jsonl').read_text().splitlines())}
    completions=[gold[r['task_id']]['completion'] for r in rows]
    assert rl_train.oracle_reward(completions,[r['task_json'] for r in rows],thinking=[True]*len(rows))==[1.0]*len(rows)
    wallet=next(r for r in rows if r['source_group']=='wallet')
    good=gold[wallet['task_id']]['completion']
    duplicate=good.replace('<|im_end|>','')+good.split('</think>',1)[1]
    bad=['<think>work</think>OP_0','<think>work</think><tool_call>{"name":"submit_script","arguments":{"script":"OP_1"}}</tool_call>',duplicate]
    assert rl_train.oracle_reward(bad,[wallet['task_json']]*len(bad),thinking=[True]*len(bad))==[0.0]*len(bad)
    print('HTTP reward verified: 64 rendered gold completions pass; wrong output and duplicate wallet calls receive zero')


if __name__=='__main__':main()
