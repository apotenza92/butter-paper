#!/usr/bin/env bash
set -Eeuo pipefail

script_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)
probe_dir=$(cd -- "$script_dir/.." && pwd -P)
migration_dir=$(cd -- "$probe_dir/.." && pwd -P)
policy="$probe_dir/portable/ubuntu24-build-policy.json"
target="$migration_dir/.build-targets/gpui-component-portable-u24"
state="$probe_dir/.prepared/portable-ubuntu24"
source_snapshot="$state/source"
cargo_home="$state/cargo-home"
image_context="$state/image-context"
pdfium="$migration_dir/gpui-gallery/target/pdfium-development/x86_64-unknown-linux-gnu/lib/libpdfium.so"
run_id="$(date -u +%Y%m%dT%H%M%S%N)-$$"
run_state="$state/runs/$run_id"
prepared_overlay="$run_state/prepared"
toolchain_name=1.97.1-x86_64-unknown-linux-gnu
toolchain=$(rustup +1.97.1 which rustc 2>/dev/null | sed 's,/bin/rustc$,,' || true)
image=butter-paper-portable-u24:ubuntu24-20260820
container_name="bp-portable-u24-$$"
target_sentinel=.butter-paper-portable-ubuntu24-target.json
state_sentinel=.butter-paper-portable-ubuntu24-state.json

usage() {
  echo "usage: $0 [--print-plan|build]" >&2
}

sample_target_kib() {
  local raw first
  raw=$("$@" 2>/dev/null) || true
  first=${raw%%$'\n'*}
  first=${first%%[[:space:]]*}
  if [[ "$first" =~ ^[0-9]+$ ]]; then
    printf '%s\n' "$first"
  else
    printf '0\n'
  fi
}

mock_du_value_then_fail() {
  printf '2409044\n'
  return 1
}

print_plan() {
  node --input-type=module - "$policy" <<'NODE'
import { readFileSync } from "node:fs";
const policy = JSON.parse(readFileSync(process.argv[2], "utf8"));
console.log(JSON.stringify({
  targetRelative: policy.paths.targetRelativeToMigration,
  sourceReadOnly: true,
  rustReadOnly: true,
  cargoOffline: true,
  network: "none",
  readOnlyRoot: true,
  capDrop: "ALL",
  noNewPrivileges: true,
  resources: {
    memory: policy.limits.memory,
    memorySwap: policy.limits.memorySwap,
    cpus: policy.limits.cpus,
    pids: policy.limits.pids,
    tmpfs: policy.limits.tmpfs,
  },
  diskKiB: {
    preflight: policy.limits.preflightFreeKiB,
    runtimeStop: policy.limits.runtimeStopFreeKiB,
    absoluteMin: policy.limits.absoluteMinFreeKiB,
    targetMax: policy.limits.maxTargetKiB,
  },
  cargoJobs: policy.limits.cargoJobs,
  incremental: policy.limits.cargoIncremental,
  compileTimeoutSeconds: policy.limits.compileTimeoutSeconds,
  cleanup: {
    ordinaryFailure: "retain",
    diskSafety: "remove-owned-target-only",
  },
  preparedOverlay: {
    uniquePerRun: true,
    seededFromFrozenSource: true,
    writable: true,
    immutableSourceRemainsReadOnly: true,
    priorFailureEvidencePreserved: true,
  },
  testUtilities: {
    qpdf: policy.testUtilities.qpdf.version,
    "poppler-utils": policy.testUtilities["poppler-utils"].version,
    runtimeArchive: false,
  },
  licenseReceipts: {
    fontconfig: {
      package: "libfontconfig-dev",
      sourcePackage: "fontconfig",
      version: "2.15.0-1.1ubuntu2",
      path: "/usr/share/doc/libfontconfig-dev/copyright",
      sha256: "b215a61cdd3e62b5b17cc28b1852c78acb3dd38be0fb30706f7efc050dba91db",
      provenance: policy.snapshot.url,
    },
  },
}, null, 2));
NODE
}

free_kib() { df -Pk "$migration_dir" | awk 'NR == 2 { print $4 }'; }
target_kib() { sample_target_kib du -sk "$target"; }
memory_kib() { awk '/^MemAvailable:/ { print $2 }' /proc/meminfo; }

assert_owned_path() {
  local candidate=$1 expected=$2
  [[ "$candidate" == "$expected" && "$candidate" != / && "$candidate" != "$probe_dir" && "$candidate" != "$migration_dir" ]] || {
    echo "refusing unsafe owned path: $candidate" >&2
    exit 125
  }
  [[ ! -L "$candidate" ]] || { echo "refusing symlink owned path: $candidate" >&2; exit 125; }
}

verify_sentinel() {
  local directory=$1 sentinel=$2 content=$3
  [[ -f "$directory/$sentinel" ]] || return 1
  [[ "$(<"$directory/$sentinel")" == "$content" ]]
}

remove_owned_source_snapshot() {
  local expected='{"kind":"butter-paper-portable-ubuntu24-source-snapshot"}'
  [[ "$source_snapshot" == "$state/source" &&
     -d "$state" && ! -L "$state" &&
     -d "$source_snapshot" && ! -L "$source_snapshot" ]] || {
    echo "source snapshot ownership path is unsafe" >&2
    return 125
  }
  [[ -f "$source_snapshot/.snapshot-owner.json" &&
     ! -L "$source_snapshot/.snapshot-owner.json" ]] &&
    verify_sentinel "$source_snapshot" .snapshot-owner.json "$expected" || {
    echo "source snapshot ownership sentinel is not exact" >&2
    return 125
  }
  rm -rf --one-file-system -- "$source_snapshot"
  [[ ! -e "$source_snapshot" ]] || {
    echo "source snapshot cleanup failed" >&2
    return 125
  }
}

test_source_snapshot_deletion() {
  local fixture=$1 test_root sibling sentinel_target outcome
  test_root=$(mktemp -d)
  state="$test_root/state"
  source_snapshot="$state/source"
  sibling="$state/prior-failure/evidence"
  sentinel_target="$test_root/sentinel-target"
  mkdir -p "$source_snapshot" "${sibling%/*}"
  printf '%s\n' source > "$source_snapshot/content"
  printf '%s\n' sibling > "$sibling"
  case "$fixture" in
    canonical)
      printf '%s\n' '{"kind":"butter-paper-portable-ubuntu24-source-snapshot"}' > "$source_snapshot/.snapshot-owner.json"
      ;;
    wrong)
      printf '%s\n' '{"kind":"some-other-snapshot"}' > "$source_snapshot/.snapshot-owner.json"
      ;;
    malformed)
      printf '%s\n' '{not-json' > "$source_snapshot/.snapshot-owner.json"
      ;;
    symlink-sentinel)
      printf '%s\n' '{"kind":"butter-paper-portable-ubuntu24-source-snapshot"}' > "$sentinel_target"
      ln -s "$sentinel_target" "$source_snapshot/.snapshot-owner.json"
      ;;
    *) return 2 ;;
  esac

  if [[ "$fixture" == canonical ]]; then
    remove_owned_source_snapshot
    [[ ! -e "$source_snapshot" && -f "$sibling" ]]
    outcome='accepted source-removed sibling-preserved'
  else
    if remove_owned_source_snapshot; then
      echo "unsafe source sentinel was accepted: $fixture" >&2
      return 1
    fi
    [[ -d "$source_snapshot" && -f "$source_snapshot/content" && -f "$sibling" ]]
    outcome='rejected source-preserved sibling-preserved'
    rm -- "$source_snapshot/.snapshot-owner.json"
    [[ ! -e "$sentinel_target" ]] || rm -- "$sentinel_target"
    printf '%s\n' '{"kind":"butter-paper-portable-ubuntu24-source-snapshot"}' > "$source_snapshot/.snapshot-owner.json"
    remove_owned_source_snapshot
  fi
  rm -- "$sibling"
  rmdir -- "${sibling%/*}" "$state" "$test_root"
  printf '%s\n' "$outcome"
}

cleanup_owned_prepared_overlay() {
  local expected_run_state="$state/runs/$run_id"
  local expected_sentinel="{\"kind\":\"butter-paper-portable-ubuntu24-run\",\"runId\":\"$run_id\"}"
  local entry name save_pid= delete_pid= save_count=0 delete_count=0
  local -a top_entries surface_entries

  [[ "$run_state" == "$expected_run_state" && "$prepared_overlay" == "$run_state/prepared" ]] || {
    echo "portable run-state path is not exact" >&2
    return 125
  }
  for entry in "$state" "$state/runs" "$run_state" "$prepared_overlay"; do
    [[ -d "$entry" && ! -L "$entry" ]] || {
      echo "portable run-state ownership path is unsafe: $entry" >&2
      return 125
    }
  done
  [[ -f "$run_state/.run-owner.json" && ! -L "$run_state/.run-owner.json" ]] &&
    verify_sentinel "$run_state" .run-owner.json "$expected_sentinel" || {
    echo "portable run-state ownership is not proven" >&2
    return 125
  }
  [[ -d "$prepared_overlay/gpui-component-c27f5d5c" &&
     ! -L "$prepared_overlay/gpui-component-c27f5d5c" ]] &&
    diff -qr \
      "$source_snapshot/gpui-component-compat/.prepared/gpui-component-c27f5d5c" \
      "$prepared_overlay/gpui-component-c27f5d5c" >/dev/null || {
    echo "prepared overlay changed the pinned dependency content" >&2
    return 125
  }

  mapfile -d '' top_entries < <(find "$prepared_overlay" -mindepth 1 -maxdepth 1 -print0)
  for entry in "${top_entries[@]}"; do
    name=${entry##*/}
    case "$name" in
      gpui-component-c27f5d5c)
        ;;
      real-document-spine-surfaces)
        [[ -d "$entry" && ! -L "$entry" ]] || {
          echo "worker surface root is unsafe" >&2
          return 125
        }
        ;;
      document-spine-save-*.pdf)
        [[ "$name" =~ ^document-spine-save-([0-9]+)\.pdf$ && -f "$entry" && ! -L "$entry" ]] || {
          echo "save scratch is unsafe: $name" >&2
          return 125
        }
        save_pid=${BASH_REMATCH[1]}
        (( save_count += 1 ))
        ;;
      document-spine-delete-*.pdf)
        [[ "$name" =~ ^document-spine-delete-([0-9]+)\.pdf$ && -f "$entry" && ! -L "$entry" ]] || {
          echo "delete scratch is unsafe: $name" >&2
          return 125
        }
        delete_pid=${BASH_REMATCH[1]}
        (( delete_count += 1 ))
        ;;
      *)
        echo "unexpected prepared overlay entry: $name" >&2
        return 125
        ;;
    esac
  done
  (( save_count == 1 && delete_count == 1 )) && [[ "$save_pid" == "$delete_pid" ]] || {
    echo "prepared scratch PID set is not exact" >&2
    return 125
  }

  if [[ -e "$prepared_overlay/real-document-spine-surfaces" ]]; then
    mapfile -d '' surface_entries < <(
      find "$prepared_overlay/real-document-spine-surfaces" -mindepth 1 -maxdepth 1 -print0
    )
    (( ${#surface_entries[@]} <= 1 )) || {
      echo "worker surface set is not exact" >&2
      return 125
    }
    if (( ${#surface_entries[@]} == 1 )); then
      entry=${surface_entries[0]}
      name=${entry##*/}
      [[ "$name" == "$save_pid" && -d "$entry" && ! -L "$entry" ]] || {
        echo "worker surface PID is unsafe: $name" >&2
        return 125
      }
      ! find "$entry" -mindepth 1 -print -quit | grep -q . || {
        echo "worker surface contains payload" >&2
        return 125
      }
    fi
  fi

  rm -rf --one-file-system -- "$run_state"
  [[ ! -e "$run_state" ]] || {
    echo "current-run state cleanup failed" >&2
    return 125
  }
  printf '%s\n' 'prepared_overlay_cleanup=verified' >> "$target/portable-ubuntu24-receipt.txt"
}

test_owned_overlay_cleanup() {
  local fixture=$1 test_root sibling
  test_root=$(mktemp -d)
  trap 'rm -rf --one-file-system -- "$test_root"' RETURN
  state="$test_root/state"
  run_id=test-current
  run_state="$state/runs/$run_id"
  prepared_overlay="$run_state/prepared"
  source_snapshot="$test_root/source"
  target="$test_root/target"
  sibling="$state/runs/prior-failure/evidence"
  mkdir -p \
    "$prepared_overlay/gpui-component-c27f5d5c" \
    "$source_snapshot/gpui-component-compat/.prepared/gpui-component-c27f5d5c" \
    "$target" "${sibling%/*}"
  printf '%s\n' '{"prior":"preserved"}' > "$sibling"
  printf '%s\n' '{"kind":"butter-paper-portable-ubuntu24-run","runId":"test-current"}' > "$run_state/.run-owner.json"
  printf '%s\n' pinned > "$prepared_overlay/gpui-component-c27f5d5c/content"
  printf '%s\n' pinned > "$source_snapshot/gpui-component-compat/.prepared/gpui-component-c27f5d5c/content"

  case "$fixture" in
    expected)
      printf '%s\n' save > "$prepared_overlay/document-spine-save-42.pdf"
      printf '%s\n' delete > "$prepared_overlay/document-spine-delete-42.pdf"
      mkdir -p "$prepared_overlay/real-document-spine-surfaces/42"
      ;;
    absent-surface)
      printf '%s\n' save > "$prepared_overlay/document-spine-save-42.pdf"
      printf '%s\n' delete > "$prepared_overlay/document-spine-delete-42.pdf"
      ;;
    payload)
      printf '%s\n' save > "$prepared_overlay/document-spine-save-42.pdf"
      printf '%s\n' delete > "$prepared_overlay/document-spine-delete-42.pdf"
      mkdir -p "$prepared_overlay/real-document-spine-surfaces/42"
      printf '%s\n' payload > "$prepared_overlay/real-document-spine-surfaces/42/payload"
      ;;
    symlink)
      ln -s /dev/null "$prepared_overlay/document-spine-save-42.pdf"
      printf '%s\n' delete > "$prepared_overlay/document-spine-delete-42.pdf"
      ;;
    device)
      mkfifo "$prepared_overlay/document-spine-save-42.pdf"
      printf '%s\n' delete > "$prepared_overlay/document-spine-delete-42.pdf"
      ;;
    deeper)
      printf '%s\n' save > "$prepared_overlay/document-spine-save-42.pdf"
      printf '%s\n' delete > "$prepared_overlay/document-spine-delete-42.pdf"
      mkdir -p "$prepared_overlay/real-document-spine-surfaces/42/deeper"
      ;;
    extra)
      printf '%s\n' save > "$prepared_overlay/document-spine-save-42.pdf"
      printf '%s\n' delete > "$prepared_overlay/document-spine-delete-42.pdf"
      printf '%s\n' extra > "$prepared_overlay/extra.pdf"
      ;;
    mismatched)
      printf '%s\n' save > "$prepared_overlay/document-spine-save-42.pdf"
      printf '%s\n' delete > "$prepared_overlay/document-spine-delete-43.pdf"
      ;;
    nonnum)
      printf '%s\n' save > "$prepared_overlay/document-spine-save-nope.pdf"
      printf '%s\n' delete > "$prepared_overlay/document-spine-delete-nope.pdf"
      ;;
    duplicate)
      printf '%s\n' save > "$prepared_overlay/document-spine-save-42.pdf"
      printf '%s\n' save > "$prepared_overlay/document-spine-save-43.pdf"
      printf '%s\n' delete > "$prepared_overlay/document-spine-delete-42.pdf"
      ;;
    *) return 2 ;;
  esac

  if [[ "$fixture" == expected || "$fixture" == absent-surface ]]; then
    cleanup_owned_prepared_overlay
    [[ -f "$sibling" && ! -e "$run_state" ]]
    [[ "$(<"$target/portable-ubuntu24-receipt.txt")" == prepared_overlay_cleanup=verified ]]
    printf '%s\n' 'accepted sibling-preserved receipt-written'
  else
    if cleanup_owned_prepared_overlay; then
      echo "unsafe fixture was accepted: $fixture" >&2
      return 1
    fi
    [[ -f "$sibling" && -d "$run_state" ]]
    printf '%s\n' 'rejected sibling-preserved'
  fi
}

cleanup_disk_safety() {
  local expected='{"kind":"butter-paper-portable-ubuntu24-target","path":".build-targets/gpui-component-portable-u24"}'
  if verify_sentinel "$target" "$target_sentinel" "$expected"; then
    rm -rf --one-file-system -- "$target"
  else
    echo "disk-safety cleanup refused: target ownership is not proven" >&2
  fi
}

terminate_owned_run() {
  local pgid=${1:-}
  [[ -z "$pgid" ]] || kill -TERM -- "-$pgid" 2>/dev/null || true
  docker rm -f "$container_name" >/dev/null 2>&1 || true
}

run_monitored() {
  local wall_seconds=$1 watches_target=$2
  shift 2
  setsid "$@" &
  local pgid=$! start=$SECONDS reason= status
  while kill -0 "$pgid" 2>/dev/null; do
    local free size
    free=$(free_kib)
    size=$(target_kib)
    if (( free < 20971520 )); then reason=runtime-free-space
    elif (( watches_target == 1 && size > 5242880 )); then reason=target-size
    elif (( SECONDS - start >= wall_seconds )); then reason=wall-time
    else sleep 2; continue
    fi
    echo "portable build stop: $reason (freeKiB=$free targetKiB=$size)" >&2
    terminate_owned_run "$pgid"
    sleep 2
    kill -KILL -- "-$pgid" 2>/dev/null || true
    wait "$pgid" 2>/dev/null || true
    [[ "$reason" == runtime-free-space || "$reason" == target-size ]] && cleanup_disk_safety
    return 125
  done
  wait "$pgid" || status=$?
  return "${status:-0}"
}

case "${1:-build}" in
  --print-plan)
    (( $# == 1 )) || { usage; exit 2; }
    print_plan
    exit 0
    ;;
  --test-target-kib-du-failure)
    (( $# == 1 )) || { usage; exit 2; }
    sample_target_kib mock_du_value_then_fail
    exit 0
    ;;
  --test-owned-overlay-cleanup)
    (( $# == 2 )) || { usage; exit 2; }
    test_owned_overlay_cleanup "$2"
    exit 0
    ;;
  --test-source-snapshot-deletion)
    (( $# == 2 )) || { usage; exit 2; }
    test_source_snapshot_deletion "$2"
    exit 0
    ;;
  build)
    (( $# <= 1 )) || { usage; exit 2; }
    ;;
  *)
    usage
    exit 2
    ;;
esac

assert_owned_path "$target" "$migration_dir/.build-targets/gpui-component-portable-u24"
assert_owned_path "$state" "$probe_dir/.prepared/portable-ubuntu24"
(( $(free_kib) >= 31457280 )) || { echo "preflight requires 30 GiB free" >&2; exit 125; }
(( $(memory_kib) >= 350000 )) || { echo "preflight memory is too low" >&2; exit 125; }
[[ "$toolchain" == */.rustup/toolchains/"$toolchain_name" ]] || { echo "Rust 1.97.1 toolchain path is not exact" >&2; exit 125; }
[[ "$("$toolchain/bin/rustc" --version)" == rustc\ 1.97.1* ]] || { echo "Rust 1.97.1 is unavailable" >&2; exit 125; }
[[ -f "$pdfium" ]] || { echo "the reviewed development PDFium library is unavailable" >&2; exit 125; }

mkdir -p -- "$state" "$migration_dir/.build-targets"
if [[ ! -f "$state/$state_sentinel" ]]; then
  printf '%s\n' '{"kind":"butter-paper-portable-ubuntu24-state","path":".prepared/portable-ubuntu24"}' > "$state/$state_sentinel"
fi
verify_sentinel "$state" "$state_sentinel" '{"kind":"butter-paper-portable-ubuntu24-state","path":".prepared/portable-ubuntu24"}' || {
  echo "portable state ownership is not proven" >&2
  exit 125
}

snapshot_tmp=$(mktemp -d "$state/source.new.XXXXXX")
trap 'rm -rf --one-file-system -- "$snapshot_tmp" 2>/dev/null || true; terminate_owned_run ""' EXIT INT TERM
mkdir -p "$snapshot_tmp/gpui-component-compat/.prepared/real-document-spine-surfaces" "$snapshot_tmp/gpui-gallery" "$snapshot_tmp/performance/fixtures" "$snapshot_tmp/performance/results/public-fixtures-v1"
cp -a "$probe_dir/Cargo.toml" "$probe_dir/Cargo.lock" "$probe_dir/rust-toolchain.toml" "$probe_dir/src" "$probe_dir/tests" "$probe_dir/vendor" "$snapshot_tmp/gpui-component-compat/"
cp -a "$probe_dir/.prepared/gpui-component-c27f5d5c" "$snapshot_tmp/gpui-component-compat/.prepared/"
gallery="$migration_dir/gpui-gallery"
cp -a "$gallery/Cargo.toml" "$gallery/Cargo.lock" "$gallery/rust-toolchain.toml" "$gallery/src" "$snapshot_tmp/gpui-gallery/"
for input in comparison-workload.json comparison-workload-v4.materialized.json comparison-workload-v5.materialized.json; do
  cp -a "$migration_dir/performance/$input" "$snapshot_tmp/performance/"
done
for input in bp-annotation-density-v1.fixture.json bp-rectangle-v1.fixture.json; do
  cp -a "$migration_dir/performance/fixtures/$input" "$snapshot_tmp/performance/fixtures/"
done
for input in bp-multi-page-v1.pdf bp-annotation-all-v1.pdf; do
  cp -a "$migration_dir/performance/results/public-fixtures-v1/$input" "$snapshot_tmp/performance/results/public-fixtures-v1/"
done
printf '%s\n' '{"kind":"butter-paper-portable-ubuntu24-source-snapshot"}' > "$snapshot_tmp/.snapshot-owner.json"
if [[ -e "$source_snapshot" ]]; then
  remove_owned_source_snapshot
fi
mv "$snapshot_tmp" "$source_snapshot"
snapshot_tmp=

[[ "$run_state" == "$state/runs/"* && "$run_state" != "$state/runs/" && ! -L "$run_state" ]] || {
  echo "portable run-state path is unsafe" >&2
  exit 125
}
mkdir -p -- "$run_state" "$prepared_overlay"
printf '%s\n' "{\"kind\":\"butter-paper-portable-ubuntu24-run\",\"runId\":\"$run_id\"}" > "$run_state/.run-owner.json"
cp --reflink=auto -a "$source_snapshot/gpui-component-compat/.prepared/." "$prepared_overlay/"
diff -qr "$source_snapshot/gpui-component-compat/.prepared" "$prepared_overlay" >/dev/null || {
  echo "prepared overlay does not match the frozen source seed" >&2
  exit 125
}

if [[ ! -d "$cargo_home" ]]; then
  cargo_tmp=$(mktemp -d "$state/cargo-home.new.XXXXXX")
  cp -a /root/.cargo/. "$cargo_tmp/"
  mv "$cargo_tmp" "$cargo_home"
fi

mkdir -p "$target"
if [[ ! -f "$target/$target_sentinel" ]]; then
  printf '%s\n' '{"kind":"butter-paper-portable-ubuntu24-target","path":".build-targets/gpui-component-portable-u24"}' > "$target/$target_sentinel"
fi
verify_sentinel "$target" "$target_sentinel" '{"kind":"butter-paper-portable-ubuntu24-target","path":".build-targets/gpui-component-portable-u24"}' || {
  echo "portable target ownership is not proven" >&2
  exit 125
}

mkdir -p "$image_context"
cp -a "$probe_dir/portable/Containerfile.ubuntu24" "$image_context/"
ca_deb="$image_context/ca-certificates_20240203_all.deb"
if [[ ! -f "$ca_deb" ]] || ! printf '%s  %s\n' 641de77d8f142cfd62a1a6f964ba67b20754d3337c480efb529d086075a06c9a "$ca_deb" | sha256sum --check --status; then
  curl --fail --silent --show-error --location \
    https://snapshot.ubuntu.com/ubuntu/20260820T000000Z/pool/main/c/ca-certificates/ca-certificates_20240203_all.deb \
    --output "$ca_deb"
fi
printf '%s  %s\n' 641de77d8f142cfd62a1a6f964ba67b20754d3337c480efb529d086075a06c9a "$ca_deb" | sha256sum --check --strict

run_monitored 600 0 docker build --pull=false --network=default --file "$image_context/Containerfile.ubuntu24" --tag "$image" "$image_context"

inner='set -Eeuo pipefail
export PATH=/opt/rust/bin:/usr/bin:/bin
export CARGO_HOME=/cargo-home CARGO_TARGET_DIR=/target CARGO_BUILD_JOBS=1 CARGO_INCREMENTAL=0 RUSTUP_TOOLCHAIN= RUST_MIN_STACK=16777216
rustc --version
cargo --version
cargo build --locked --offline --bins
cargo build --manifest-path ../gpui-gallery/Cargo.toml --locked --offline --no-default-features --features pdfium-worker --bin butter-paper-pdf-worker
binaries=(/target/debug/component_story /target/debug/butter-paper-pdf-worker)
for binary in "${binaries[@]}"; do
  test -x "$binary"
  ! ldd "$binary" | grep -q "not found"
  highest=$(readelf --version-info "$binary" | grep -o "GLIBC_[0-9][0-9.]*" | sort -Vu | tail -1)
  test -n "$highest"
  test "$(printf "%s\n%s\n" "$highest" GLIBC_2.39 | sort -V | tail -1)" = GLIBC_2.39
done
BP_PDFIUM_LIBRARY=/pdfium/libpdfium.so cargo test --locked --offline --test document_workspace -- --ignored --exact real_pdfium_worker_opens_navigates_and_exits_without_an_orphan
{
  printf "candidate=portable-ubuntu24\n"
  printf "rust="; rustc --version
  printf "cargo="; cargo --version
  . /etc/os-release; printf "os=%s %s\n" "$ID" "$VERSION_ID"
  printf "snapshot=20260820T000000Z\n"
  dpkg-query -W -f="package=\${Package} version=\${Version}\n" build-essential ca-certificates clang git libfontconfig-dev libvulkan-dev libx11-dev libxcb1-dev libxcb-xkb-dev libxkbcommon-dev libxkbcommon-x11-dev poppler-utils pkg-config qpdf xz-utils
  printf "test_utility_runtime_archive=false\n"
  printf "test_utility_closure_begin\n"
  dpkg-query -W -f="\${Package}=\${Version}\n" | sort
  printf "test_utility_closure_end\n"
  for package in poppler-utils qpdf; do
    printf "test_utility_license package=%s version=%s copyright_sha256=" "$package" "$(dpkg-query -W -f="\${Version}" "$package")"
    sha256sum "/usr/share/doc/$package/copyright" | cut -d " " -f1
  done
  fontconfig_version=$(dpkg-query -W -f="\${Version}" libfontconfig-dev)
  fontconfig_source=$(dpkg-query -W -f="\${source:Package}" libfontconfig-dev)
  fontconfig_license=/usr/share/doc/libfontconfig-dev/copyright
  fontconfig_license_sha256=$(sha256sum "$fontconfig_license" | cut -d " " -f1)
  test "$fontconfig_version" = 2.15.0-1.1ubuntu2
  test "$fontconfig_source" = fontconfig
  test "$fontconfig_license_sha256" = b215a61cdd3e62b5b17cc28b1852c78acb3dd38be0fb30706f7efc050dba91db
  printf "dependency_license package=libfontconfig-dev source_package=%s version=%s path=%s copyright_sha256=%s provenance=https://snapshot.ubuntu.com/ubuntu/20260820T000000Z\n" \
    "$fontconfig_source" "$fontconfig_version" "$fontconfig_license" "$fontconfig_license_sha256"
  for binary in "${binaries[@]}"; do sha256sum "$binary"; done
} > /target/portable-ubuntu24-receipt.txt
printf "%s\n" "{\"status\":0,\"runnerMode\":\"portable-ubuntu24-development\",\"cargoSubcommand\":\"build\",\"cargoArguments\":[\"--locked\",\"--offline\",\"--bins\"],\"targetDisposition\":{\"state\":\"retained\",\"reason\":\"successful-run\"},\"realPdfWorkerTest\":\"passed\"}" > /target/portable-ubuntu24-build-summary.json'

run_monitored 2700 1 docker run --name "$container_name" --rm \
  --network none --read-only --cap-drop ALL --security-opt no-new-privileges \
  --memory 3g --memory-swap 3g --cpus 2 --pids-limit 512 \
  --tmpfs /tmp:rw,noexec,nosuid,size=1g \
  --mount "type=bind,src=$source_snapshot,dst=/source,readonly" \
  --mount "type=bind,src=$toolchain,dst=/opt/rust,readonly" \
  --mount "type=bind,src=$cargo_home,dst=/cargo-home" \
  --mount "type=bind,src=$target,dst=/target" \
  --mount "type=bind,src=$pdfium,dst=/pdfium/libpdfium.so,readonly" \
  --mount "type=bind,src=$prepared_overlay,dst=/source/gpui-component-compat/.prepared" \
  "$image" /bin/bash -lc "$inner"

cleanup_owned_prepared_overlay

echo "portable Ubuntu 24 candidate validated: $target"
