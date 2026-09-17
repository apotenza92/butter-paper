#!/usr/bin/env python3
"""Run a macOS verification command under the existing host and target budgets."""
import json
import os
from pathlib import Path
import re
import shutil
import signal
import subprocess
import sys
import time


def metrics(root, target):
    free = shutil.disk_usage(root).free // 1024
    allocated = 0
    for directory, _, files in os.walk(target):
        for name in files:
            try:
                allocated += os.lstat(os.path.join(directory, name)).st_blocks // 2
            except FileNotFoundError:
                pass  # Cargo may rename or remove an output during the scan.
    vm = subprocess.check_output(["/usr/bin/vm_stat"], text=True)
    page = int(re.search(r"page size of (\d+)", vm).group(1))
    available = sum(
        int(re.search(rf"{name}:\s+(\d+)", vm).group(1))
        for name in ["Pages free", "Pages inactive", "Pages speculative"]
    ) * page // 1024
    return free, allocated, available


def stop(process, grace):
    try:
        os.killpg(process.pid, signal.SIGTERM)
    except ProcessLookupError:
        return
    try:
        process.wait(timeout=grace)
    except subprocess.TimeoutExpired:
        try:
            os.killpg(process.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
        process.wait()


def main():
    if sys.platform != "darwin" or len(sys.argv) < 2:
        raise SystemExit("usage on macOS: run-native-bounded.py COMMAND [ARG ...]")
    root = Path(__file__).resolve().parent.parent
    policy = json.loads((root / "build-guard-policy.json").read_text())
    target = root.parent / ".build-targets/gpui-migration"
    target.mkdir(parents=True, exist_ok=True)
    free, allocated, available = metrics(root, target)
    if (free < policy["preflightFreeKiB"] or allocated > policy["maxTargetKiB"]
            or available < policy["minMemoryKiB"]):
        raise SystemExit("native verification preflight exceeded a host or target budget")
    env = os.environ.copy()
    env.update(CARGO_TARGET_DIR=str(target), CARGO_BUILD_JOBS="1", CARGO_INCREMENTAL="0",
               RUST_MIN_STACK=str(policy["rustMinStackBytes"]))
    process = subprocess.Popen(sys.argv[1:], cwd=root, env=env, start_new_session=True)
    start = time.monotonic()
    reason = None
    try:
        while process.poll() is None:
            free, allocated, available = metrics(root, target)
            if free < policy["runtimeStopFreeKiB"]:
                reason = "runtime-free-space"
            elif allocated > policy["maxTargetKiB"]:
                reason = "target-size"
            elif available < policy["minMemoryKiB"]:
                reason = "available-memory"
            elif time.monotonic() - start > policy["wallSeconds"]:
                reason = "wall-time"
            if reason:
                stop(process, policy["terminationGraceSeconds"])
                break
            time.sleep(1)
    finally:
        if process.poll() is None:
            stop(process, policy["terminationGraceSeconds"])
    code = process.wait()
    print(json.dumps(dict(status=code, reason=reason, duration=time.monotonic() - start,
                          freeKiB=free, targetKiB=allocated)))
    return code if not reason else 125


if __name__ == "__main__":
    sys.exit(main())
