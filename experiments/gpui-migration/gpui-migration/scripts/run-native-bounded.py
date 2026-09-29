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


def resolve_target(command, root, environment, policy):
    """Resolve and pin the target Cargo actually receives, before any spawning."""
    command = list(command)
    environment = dict(environment)
    if command and Path(command[0]).name == "env":
        command.pop(0)
        while command and "=" in command[0] and not command[0].startswith("-"):
            name, value = command.pop(0).split("=", 1)
            if not re.fullmatch(r"[A-Za-z_][A-Za-z0-9_]*", name):
                raise ValueError("invalid env assignment")
            environment[name] = value
        if not command or command[0].startswith("-"):
            raise ValueError("env options are not supported by the bounded runner")
    if not command:
        raise ValueError("a command is required")
    cargo = Path(command[0]).name == "cargo"
    split = command.index("--") if "--" in command else len(command)
    arguments, forwarded = command[:split], command[split:]
    explicit = []
    cleaned = []
    index = 0
    while index < len(arguments):
        argument = arguments[index]
        if cargo and argument == "--target-dir":
            index += 1
            if index == len(arguments) or arguments[index].startswith("-"):
                raise ValueError("--target-dir requires a directory")
            explicit.append(arguments[index])
        elif cargo and argument.startswith("--target-dir="):
            explicit.append(argument.split("=", 1)[1])
        else:
            cleaned.append(argument)
        index += 1
    if len(explicit) > 1:
        raise ValueError("multiple --target-dir overrides are ambiguous")
    value = (explicit[0] if explicit else environment.get("CARGO_TARGET_DIR"))
    if value is None:
        target = root.parent / policy["targetRelativeToMigration"]
    else:
        if not value.strip():
            raise ValueError("Cargo target directory must not be empty")
        target = Path(value)
        if not target.is_absolute():
            target = root / target
    target = target.resolve()
    if target == Path(target.anchor) or target == root.resolve():
        raise ValueError("Cargo target must be a dedicated output directory")
    environment["CARGO_TARGET_DIR"] = str(target)
    if cargo:
        # An explicit canonical CLI target takes precedence over Cargo config.
        # Test executable arguments after -- must remain untouched.
        command = cleaned + ["--target-dir", str(target)] + forwarded
    return command, environment, target


def metrics(root, target):
    free = min(shutil.disk_usage(root).free, shutil.disk_usage(target).free) // 1024
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
    try:
        command, env, target = resolve_target(sys.argv[1:], root, os.environ, policy)
    except ValueError as error:
        raise SystemExit(str(error)) from error
    target.mkdir(parents=True, exist_ok=True)
    free, allocated, available = metrics(root, target)
    if (free < policy["preflightFreeKiB"] or allocated > policy["maxTargetKiB"]
            or available < policy["minMemoryKiB"]):
        raise SystemExit(f"native verification preflight exceeded a host or target budget: {target} ({allocated} KiB)")
    env.update(CARGO_TARGET_DIR=str(target), CARGO_BUILD_JOBS="1", CARGO_INCREMENTAL="0",
               RUST_MIN_STACK=str(policy["rustMinStackBytes"]))
    process = subprocess.Popen(command, cwd=root, env=env, start_new_session=True)
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
                          freeKiB=free, targetKiB=allocated, targetDirectory=str(target))))
    return code if not reason else 125


if __name__ == "__main__":
    sys.exit(main())
