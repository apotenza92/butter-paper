#!/usr/bin/env bash
set -uo pipefail

probe_dir=$(cd "$(dirname "$0")/.." && pwd)
migration_dir=$(cd "$probe_dir/.." && pwd)
target_dir="$migration_dir/.build-targets/gpui-component-compat"
guard="$probe_dir/scripts/build-guard.mjs"
evidence_dir="$probe_dir/.prepared/evidence"
run_id="$(date -u +%Y%m%dT%H%M%SZ)-$$"
raw_log="$evidence_dir/button-probe-$run_id.raw.log"
log_file="$evidence_dir/button-probe-$run_id.log"
summary_file="$evidence_dir/button-probe-$run_id.summary.json"
lock_file="$probe_dir/.prepared/button-probe.lock"
requested_mode="${1:-all-targets}"
build_pid=""
build_pgid=""
limit_reason=""
target_disposition_state="retained"
target_disposition_reason="unexpected-runner-exit"

if (( $# > 1 )); then
  echo "usage: $0 [fixed runner mode; see scripts/build-guard.mjs]" >&2
  exit 2
fi

mode_json=$(node "$guard" runner-mode "$requested_mode") || exit 2
runner_mode=$(node -e 'process.stdout.write(JSON.parse(process.argv[1]).name)' "$mode_json")
cargo_subcommand=$(node -e 'process.stdout.write(JSON.parse(process.argv[1]).cargoSubcommand ?? "test")' "$mode_json")
case "$cargo_subcommand" in
  test|build) ;;
  *) echo "runner mode selected a non-allowlisted Cargo subcommand" >&2; exit 2 ;;
esac
controlled_failure_status=$(node -e '
const value = JSON.parse(process.argv[1]).controlledFailureStatus;
if (value !== null) process.stdout.write(String(value));
' "$mode_json")
mapfile -t cargo_mode_args < <(node -e '
for (const argument of JSON.parse(process.argv[1]).cargoArgs) console.log(argument);
' "$mode_json")
manifest_relative=$(node -e '
const mode = JSON.parse(process.argv[1]);
process.stdout.write(mode.manifestRelativeToProbe ?? "Cargo.toml");
' "$mode_json")
case "$manifest_relative" in
  Cargo.toml|../gpui-gallery/Cargo.toml) ;;
  *) echo "runner mode selected a non-allowlisted Cargo manifest" >&2; exit 2 ;;
esac
manifest_path=$(cd "$probe_dir" && realpath "$manifest_relative")
target_relative=$(node -e '
const mode = JSON.parse(process.argv[1]);
process.stdout.write(mode.targetRelativeToMigration ?? ".build-targets/gpui-component-compat");
' "$mode_json")
case "$target_relative" in
  .build-targets/gpui-component-compat|.build-targets/gpui-gallery-backend|.build-targets/gpui-component-performance) ;;
  *) echo "runner mode selected a non-allowlisted Cargo target" >&2; exit 2 ;;
esac
target_dir="$migration_dir/$target_relative"

mkdir -p "$evidence_dir"
exec 9>"$lock_file"
if ! flock -n 9; then
  echo "another component probe owns the build lock" >&2
  exit 2
fi

trim_log() {
  local retained_bytes
  retained_bytes=$(node -e 'const p=require(process.argv[1]); process.stdout.write(String(p.retainedLogBytes))' "$probe_dir/build-guard-policy.json")
  if [[ -f "$raw_log" ]]; then
    tail -c "$retained_bytes" "$raw_log" >"$log_file"
    rm -f "$raw_log"
  fi
}

cleanup_disposable_target() {
  node "$guard" cleanup "$target_dir" >/dev/null
}

apply_disposition() {
  local status=$1
  local reason=${2:--}
  local disposition_json
  disposition_json=$(node "$guard" disposition "$status" "$reason" false)
  local action
  action=$(node -e 'process.stdout.write(JSON.parse(process.argv[1]).action)' "$disposition_json")
  target_disposition_reason=$(node -e 'process.stdout.write(JSON.parse(process.argv[1]).reason)' "$disposition_json")
  if [[ "$action" == "clean" ]]; then
    cleanup_disposable_target
    target_disposition_state="cleaned"
  else
    target_disposition_state="retained"
  fi
}

write_summary() {
  local status=$1
  local started=$2
  local ended=$3
  local before=$4
  local before_disposition=$5
  local after=$6
  node -e '
const [status, reason, started, ended, before, beforeDisposition, after, log, mode, modeJson, dispositionState, dispositionReason] = process.argv.slice(1);
const resolvedMode = JSON.parse(modeJson);
process.stdout.write(`${JSON.stringify({
  status: Number(status),
  limitReason: reason || null,
  runnerMode: mode,
  cargoSubcommand: resolvedMode.cargoSubcommand ?? "test",
  cargoArguments: resolvedMode.cargoArgs,
  controlledFailureStatus: resolvedMode.controlledFailureStatus,
  durationSeconds: Number(ended) - Number(started),
  diskBefore: JSON.parse(before),
  diskBeforeDisposition: JSON.parse(beforeDisposition),
  diskAfter: JSON.parse(after),
  targetDisposition: {
    state: dispositionState,
    reason: dispositionReason,
  },
  log,
}, null, 2)}\n`);
' "$status" "$limit_reason" "$started" "$ended" "$before" "$before_disposition" "$after" "$log_file" "$runner_mode" "$mode_json" "$target_disposition_state" "$target_disposition_reason" \
    | tee "$summary_file"
}

stop_build() {
  if [[ -n "$build_pid" ]] && { kill -0 "$build_pid" 2>/dev/null || { [[ -n "$build_pgid" && "$build_pgid" =~ ^[0-9]+$ ]] && kill -0 -- "-$build_pgid" 2>/dev/null; }; }; then
    if [[ -n "$build_pgid" && "$build_pgid" =~ ^[0-9]+$ ]] && kill -0 -- "-$build_pgid" 2>/dev/null; then
      kill -TERM -- "-$build_pgid" 2>/dev/null || true
    else
      kill -TERM "$build_pid" 2>/dev/null || true
    fi
    local grace
    grace=$(node -e 'const p=require(process.argv[1]); process.stdout.write(String(p.terminationGraceSeconds))' "$probe_dir/build-guard-policy.json")
    local deadline=$((SECONDS + grace))
    while { kill -0 "$build_pid" 2>/dev/null || { [[ -n "$build_pgid" && "$build_pgid" =~ ^[0-9]+$ ]] && kill -0 -- "-$build_pgid" 2>/dev/null; }; } && (( SECONDS < deadline )); do
      sleep 0.2
    done
    if kill -0 "$build_pid" 2>/dev/null || { [[ -n "$build_pgid" && "$build_pgid" =~ ^[0-9]+$ ]] && kill -0 -- "-$build_pgid" 2>/dev/null; }; then
      if [[ -n "$build_pgid" && "$build_pgid" =~ ^[0-9]+$ ]] && kill -0 -- "-$build_pgid" 2>/dev/null; then
        kill -KILL -- "-$build_pgid" 2>/dev/null || true
      else
        kill -KILL "$build_pid" 2>/dev/null || true
      fi
      while [[ -n "$build_pgid" && "$build_pgid" =~ ^[0-9]+$ ]] && kill -0 -- "-$build_pgid" 2>/dev/null; do
        sleep 0.1
      done
    fi
  fi
}

on_signal() {
  limit_reason="interrupted-$1"
  stop_build
}

on_exit() {
  local status=$?
  trap - EXIT
  stop_build
  trim_log
  exit "$status"
}

trap 'on_signal INT' INT
trap 'on_signal TERM' TERM
trap 'on_signal HUP' HUP
trap on_exit EXIT

start_epoch=$(date +%s)
preflight_json=$(node "$guard" preflight "$target_dir")
preflight_status=$?
printf '%s\n' "$preflight_json" >"$raw_log"
if (( preflight_status != 0 )); then
  limit_reason=$(node -e 'process.stdout.write(JSON.parse(process.argv[1]).reason || "")' "$preflight_json")
  apply_disposition "$preflight_status" "${limit_reason:--}"
  disk_after_json=$(node "$guard" metrics "$target_dir")
  end_epoch=$(date +%s)
  write_summary "$preflight_status" "$start_epoch" "$end_epoch" "$preflight_json" "$preflight_json" "$disk_after_json"
  exit "$preflight_status"
fi

node "$guard" initialize "$target_dir" >>"$raw_log"
disk_before_json=$(node "$guard" metrics "$target_dir")
printf 'disk-before=%s\n' "$disk_before_json" >>"$raw_log"
wall_seconds=$(node -e 'const p=require(process.argv[1]); process.stdout.write(String(p.wallSeconds))' "$probe_dir/build-guard-policy.json")
rust_min_stack_bytes=$(node -e 'const p=require(process.argv[1]); process.stdout.write(String(p.rustMinStackBytes))' "$probe_dir/build-guard-policy.json")

setsid env \
  CARGO_BUILD_JOBS=1 \
  CARGO_INCREMENTAL=0 \
  CARGO_NET_OFFLINE=true \
  CARGO_TARGET_DIR="$target_dir" \
  RUST_MIN_STACK="$rust_min_stack_bytes" \
  /usr/bin/time -v \
  timeout --signal=TERM --kill-after=10s "${wall_seconds}s" \
  cargo "$cargo_subcommand" \
    --manifest-path "$manifest_path" \
    -j 1 \
    --locked \
    --offline \
    "${cargo_mode_args[@]}" \
  >>"$raw_log" 2>&1 &
build_pid=$!
build_pgid=$(ps -o pgid= -p "$build_pid" | tr -d ' ')
if [[ -z "$build_pgid" || ! "$build_pgid" =~ ^[0-9]+$ ]]; then
  limit_reason="process-group-unavailable"
  stop_build
fi

poll_milliseconds=$(node -e 'const p=require(process.argv[1]); process.stdout.write(String(p.pollMilliseconds))' "$probe_dir/build-guard-policy.json")
poll_seconds=$(node -e 'process.stdout.write(String(Number(process.argv[1])/1000))' "$poll_milliseconds")
while [[ -z "$limit_reason" ]] && kill -0 "$build_pid" 2>/dev/null; do
  sample_json=$(node "$guard" sample "$target_dir")
  sample_status=$?
  if (( sample_status != 0 )); then
    limit_reason=$(node -e 'process.stdout.write(JSON.parse(process.argv[1]).reason)' "$sample_json")
    printf 'guard-stop=%s\n' "$sample_json" >>"$raw_log"
    stop_build
    break
  fi
  sleep "$poll_seconds"
done

wait "$build_pid" 2>/dev/null
status=$?
end_epoch=$(date +%s)
if [[ "$status" == "124" && -z "$limit_reason" ]]; then
  limit_reason="wall-time"
elif [[ -n "$limit_reason" ]]; then
  status=125
fi
if (( status == 0 )) && [[ -n "$controlled_failure_status" ]]; then
  printf 'controlled-failure-status=%s\n' "$controlled_failure_status" >>"$raw_log"
  status=$controlled_failure_status
fi

disk_before_disposition_json=$(node "$guard" metrics "$target_dir")
apply_disposition "$status" "${limit_reason:--}"
disk_after_json=$(node "$guard" metrics "$target_dir")
write_summary "$status" "$start_epoch" "$end_epoch" "$disk_before_json" "$disk_before_disposition_json" "$disk_after_json"

trim_log
exit "$status"
