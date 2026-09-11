"""Produce saved reports when the detached experiment supervisor completes."""
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import time


def main():
    root = Path('runs/rl-wallet-v2')
    pid = json.loads((root/'supervisor-pid.json').read_text())['pid']
    while True:
        status = json.loads((root/'status.json').read_text())
        if status['phase'] == 'Longer RL comparison complete; original server available':
            break
        if status.get('error'):
            raise RuntimeError('Training supervisor reported an error: '+status['error'])
        if not Path(f'/proc/{pid}').exists():
            raise RuntimeError('Supervisor exited without a completion record')
        time.sleep(15)
    subprocess.run([sys.executable, 'training/report_wallet_rl_v2.py'], check=True)
    subprocess.run([sys.executable, 'training/spot_check_wallet_rl_v2.py'], check=True)
    (root/'analysis-complete.json').write_text(json.dumps(dict(completed_unix=time.time(),
        results_sha256=hashlib.sha256((root/'results.json').read_bytes()).hexdigest()), indent=2)+'\n')


if __name__ == '__main__':
    main()
