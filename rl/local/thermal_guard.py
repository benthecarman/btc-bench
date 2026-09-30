#!/usr/bin/env python3
"""Watch GPU and system temperatures during the local RL loop and pause
the hot machine's worker until it cools down.

Every --interval seconds it samples the RTX 5090 (local nvidia-smi) and
the DGX Spark (nvidia-smi and ACPI thermal zones over ssh) into a CSV.
A machine that reaches its pause temperature, or reports hardware
thermal slowdown, has its worker container frozen with `docker pause`
(the rollout vLLM on the 5090, the trainer on the Spark). It is resumed
once it is at or below the resume temperature and at least
--min-pause seconds (default 5 min) have passed. A paused worker resumes exactly where
it stopped: rollout requests and the eval runner wait without a read
timeout, so a pause only adds time.

Standard library only; runs on the rollout host.

    python3 rl/local/thermal_guard.py --log runs/rl27/thermal.csv
"""

import argparse
import csv
import json
import os
import subprocess
import time

SPARK = "ben@spark"
GPU_QUERY = "temperature.gpu,power.draw,clocks.sm,clocks_throttle_reasons.hw_thermal_slowdown"


def log(**kw):
    print(json.dumps({"time": time.strftime("%Y-%m-%dT%H:%M:%S"), **kw}), flush=True)


def parse_gpu(line):
    temp, power, clock, hw = [x.strip() for x in line.split(",")]
    return {"gpu_c": float(temp), "power_w": float(power.split()[0]) if power[0].isdigit() else None,
            "sm_mhz": float(clock.split()[0]) if clock[0].isdigit() else None, "hw_slowdown": hw == "Active"}


def local_5090():
    out = subprocess.run(["nvidia-smi", "-i", "0", f"--query-gpu={GPU_QUERY}", "--format=csv,noheader"],
                         capture_output=True, text=True, timeout=30, check=True).stdout
    return parse_gpu(out.strip().splitlines()[0])


def spark():
    cmd = (f"nvidia-smi --query-gpu={GPU_QUERY} --format=csv,noheader; "
           "cat /sys/class/thermal/thermal_zone*/temp")
    out = subprocess.run(["ssh", "-o", "ConnectTimeout=10", SPARK, cmd], capture_output=True, text=True,
                         timeout=60, check=True).stdout.strip().splitlines()
    reading = parse_gpu(out[0])
    reading["system_c"] = max(int(x) for x in out[1:] if x.strip().isdigit()) / 1000
    return reading


def docker(host, *args):
    cmd = ["docker", *args] if host is None else ["ssh", "-o", "ConnectTimeout=10", host, "docker " + " ".join(args)]
    return subprocess.run(cmd, capture_output=True, text=True, timeout=60)


def is_paused(host, container):
    r = docker(host, "inspect", "-f", "{{.State.Paused}}", container)
    return r.returncode == 0 and r.stdout.strip() == "true"


class Guard:
    """Pause/resume one machine's worker on temperature."""

    def __init__(self, name, host, container, pause_c, resume_c, system_pause_c, min_pause):
        self.name, self.host, self.container = name, host, container
        self.pause_c, self.resume_c, self.system_pause_c = pause_c, resume_c, system_pause_c
        self.min_pause = min_pause
        self.paused_at = None
        if is_paused(host, container):  # e.g. after a guard restart mid-pause
            self.paused_at = time.time()

    def hot(self, r):
        return (r["gpu_c"] >= self.pause_c or r["hw_slowdown"]
                or (self.system_pause_c and r.get("system_c", 0) >= self.system_pause_c))

    def cool(self, r):
        return r["gpu_c"] <= self.resume_c and not r["hw_slowdown"] and (
            not self.system_pause_c or r.get("system_c", 0) < self.system_pause_c - 10)

    def update(self, r):
        if self.paused_at is None and self.hot(r):
            res = docker(self.host, "pause", self.container)
            if res.returncode == 0:
                self.paused_at = time.time()
                log(event="thermal_pause", machine=self.name, container=self.container, **r)
            else:
                log(event="thermal_pause_failed", machine=self.name, error=res.stderr.strip()[:200], **r)
        elif self.paused_at is not None and self.cool(r) and time.time() - self.paused_at >= self.min_pause:
            res = docker(self.host, "unpause", self.container)
            if res.returncode == 0 or "not paused" in res.stderr:
                log(event="thermal_resume", machine=self.name, paused_s=round(time.time() - self.paused_at), **r)
                self.paused_at = None
            else:
                log(event="thermal_resume_failed", machine=self.name, error=res.stderr.strip()[:200], **r)


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--log", required=True, help="CSV of every sample")
    ap.add_argument("--interval", type=int, default=30)
    ap.add_argument("--min-pause", type=int, default=300)
    ap.add_argument("--rollout-container", default="vllm-rl")
    ap.add_argument("--trainer-container", default="rl27-train")
    ap.add_argument("--gpu-pause-c", type=float, default=85)
    ap.add_argument("--gpu-resume-c", type=float, default=65)
    ap.add_argument("--spark-system-pause-c", type=float, default=90)
    args = ap.parse_args()
    guards = {
        "5090": Guard("5090", None, args.rollout_container, args.gpu_pause_c, args.gpu_resume_c, None,
                      args.min_pause),
        "spark": Guard("spark", SPARK, args.trainer_container, args.gpu_pause_c, args.gpu_resume_c,
                       args.spark_system_pause_c, args.min_pause),
    }
    readers = {"5090": local_5090, "spark": spark}
    new = not os.path.exists(args.log)
    with open(args.log, "a", newline="") as f:
        w = csv.writer(f)
        if new:
            w.writerow(["time", "machine", "gpu_c", "system_c", "power_w", "sm_mhz", "hw_slowdown", "paused"])
        log(event="thermal_guard_start", **{k: v for k, v in vars(args).items() if k != "log"})
        while True:
            for name, read in readers.items():
                try:
                    r = read()
                except Exception as e:  # a missed sample is not a reason to stop guarding
                    log(event="thermal_read_failed", machine=name, error=str(e)[:200])
                    continue
                guards[name].update(r)
                w.writerow([time.strftime("%Y-%m-%dT%H:%M:%S"), name, r["gpu_c"], r.get("system_c"),
                            r["power_w"], r["sm_mhz"], r["hw_slowdown"], guards[name].paused_at is not None])
                f.flush()
            time.sleep(args.interval)


if __name__ == "__main__":
    main()
