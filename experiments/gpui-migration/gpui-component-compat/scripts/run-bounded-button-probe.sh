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
runtime_verification_json='{}'
runtime_artifact_hashes_json='{}'
artifact_hash_status=0
post_success_failure_status=""
focused_real_pdf_mode=false

if (( $# > 1 )); then
  echo "usage: $0 [fixed runner mode; see scripts/build-guard.mjs]" >&2
  exit 2
fi

mode_json=$(node "$guard" runner-mode "$requested_mode") || exit 2
runner_mode=$(node -e 'process.stdout.write(JSON.parse(process.argv[1]).name)' "$mode_json")
case "$runner_mode" in
  native-shell-rectangle-real|native-shell-pen-highlight-real|native-shell-text-box-real|native-shell-core-editor-real|native-shell-focused-real|engineering-visual-properties-real|line-arrow-save-reopen-real-exact|vertex-path-cutover-real|callout-cutover-real|cloud-plus-cutover-real|dimension-cutover-real|arc-cutover-real|measurement-path-cutover-real|two-document-save-failure-real|document-image-real|snapshot-cutover-real|semantic-snapping-cutover-real|redact-cutover-real|viewer-state-real)
    focused_real_pdf_mode=true
    ;;
esac
cargo_subcommand=$(node -e 'process.stdout.write(JSON.parse(process.argv[1]).cargoSubcommand ?? "test")' "$mode_json")
case "$cargo_subcommand" in
  test|build|check) ;;
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
source_hashes_json=$(node --input-type=module -e '
import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
const mode = JSON.parse(process.argv[1]);
const probe = process.argv[2];
const allowed = new Set([
  "Cargo.toml",
  "Cargo.lock",
  "src/lib.rs",
  "src/accessible_button.rs",
  "src/adaptive_performance.rs",
  "src/application_close.rs",
  "src/application_close_workspace.rs",
  "src/cad_view_control.rs",
  "src/continuous_view_control.rs",
  "src/document_resource.rs",
  "src/document_session.rs",
  "src/document_tab_bar.rs",
  "src/document_viewer.rs",
  "src/document_workspace.rs",
  "src/native_application.rs",
  "src/dimension_property_inspector.rs",
  "src/engineering_visual_property_inspector.rs",
  "src/ink_property_inspector.rs",
  "src/measurement_property_inspector.rs",
  "src/page_scale_control.rs",
  "src/page_view_control.rs",
  "src/straight_line_property_inspector.rs",
  "src/text_box_property_inspector.rs",
  "src/vertex_path_property_inspector.rs",
  "src/native_document_view_state.rs",
  "src/native_runtime_layout.rs",
  "src/viewer_icons.rs",
  "src/viewer_toolbar_strip.rs",
  "src/zoom_control.rs",
  "src/bin/component_story.rs",
  "src/bin/butter-paper-pdf-worker.rs",
  "../gpui-gallery/Cargo.toml",
  "../gpui-gallery/Cargo.lock",
  "../gpui-gallery/src/annotation_adapter.rs",
  "../gpui-gallery/src/annotation_model.rs",
  "../gpui-gallery/src/semantic_snapping.rs",
  "../gpui-gallery/src/page_geometry.rs",
  "../gpui-gallery/src/image_asset_decode.rs",
  "../gpui-gallery/src/highlight_compositor.rs",
  "../gpui-gallery/src/pdf_worker.rs",
  "../gpui-gallery/src/bin/butter-paper-pdf-worker.rs",
  "../gpui-gallery/src/pdf_file_authority.rs",
  "../gpui-gallery/src/pdf_engine.rs",
  "../gpui-gallery/src/pdf_engine/straight_line_pdf.rs",
  "../gpui-gallery/src/viewer.rs",
  "../gpui-gallery/tests/annotation_adapter.rs",
  "../gpui-gallery/tests/pdf_persistence.rs",
  "tests/document_workspace.rs",
  "tests/build-guard.test.mjs",
  "tests/application_close_integration.rs",
  "scripts/run-native-reader.sh",
  "scripts/build-guard.mjs",
  "scripts/run-bounded-button-probe.sh",
  "../performance/results/public-fixtures-v1/fixture-index.json",
  "../performance/results/public-fixtures-v1/bp-annotation-all-v1.pdf",
  "../performance/results/public-fixtures-v1/bp-multi-page-v1.pdf",
  "../performance/results/public-fixtures-v1/bp-image-checker-v1.png",
]);
const hashes = {};
for (const relative of mode.sourceHashRelatives ?? []) {
  if (!allowed.has(relative)) throw new Error(`source hash path is not allowlisted: ${relative}`);
  hashes[relative] = createHash("sha256").update(readFileSync(resolve(probe, relative))).digest("hex");
}
process.stdout.write(JSON.stringify(hashes));
' "$mode_json" "$probe_dir") || exit 2

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
const [status, reason, started, ended, before, beforeDisposition, after, log, mode, modeJson, dispositionState, dispositionReason, sourceHashes, runtimeVerification, runtimeArtifactHashes] = process.argv.slice(1);
const resolvedMode = JSON.parse(modeJson);
process.stdout.write(`${JSON.stringify({
  status: Number(status),
  limitReason: reason || null,
  runnerMode: mode,
  cargoSubcommand: resolvedMode.cargoSubcommand ?? "test",
  cargoArguments: resolvedMode.cargoArgs,
  controlledFailureStatus: resolvedMode.controlledFailureStatus,
  sourceHashes: JSON.parse(sourceHashes),
  runtimeVerification: JSON.parse(runtimeVerification),
  runtimeArtifactHashes: JSON.parse(runtimeArtifactHashes),
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
' "$status" "$limit_reason" "$started" "$ended" "$before" "$before_disposition" "$after" "$log_file" "$runner_mode" "$mode_json" "$target_disposition_state" "$target_disposition_reason" "$source_hashes_json" "$runtime_verification_json" "$runtime_artifact_hashes_json" \
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
printf 'source-hashes=%s\n' "$source_hashes_json" >>"$raw_log"
if [[ "$focused_real_pdf_mode" == "true" ]]; then
  runtime_verification_json=$(node --input-type=module - "$probe_dir" "${BP_PDFIUM_LIBRARY-}" "$runner_mode" <<'NODE'
import { createHash } from "node:crypto";
import { existsSync, readFileSync, realpathSync, statSync } from "node:fs";
import { resolve } from "node:path";

const [probeDirectory, ambientLibrary, runnerMode] = process.argv.slice(2);
if (process.platform !== "linux") {
  throw new Error(`the focused real-PDF runner currently supports Linux, not ${process.platform}`);
}
const target = { arm64: "aarch64-unknown-linux-gnu", x64: "x86_64-unknown-linux-gnu" }[process.arch];
if (!target) throw new Error(`unsupported Linux architecture ${process.arch}`);
const migrationDirectory = resolve(probeDirectory, "..");
const galleryDirectory = resolve(migrationDirectory, "gpui-gallery");
const manifestPath = resolve(galleryDirectory, "pdfium-development-binaries.json");
const fixtureIndexPath = resolve(
  migrationDirectory,
  "performance/results/public-fixtures-v1/fixture-index.json",
);
const fixtureId = ["native-shell-core-editor-real", "native-shell-focused-real"].includes(runnerMode)
  ? "bp-annotation-all-v1"
  : "bp-multi-page-v1";
const fixturePath = resolve(
  migrationDirectory,
  `performance/results/public-fixtures-v1/${fixtureId}.pdf`,
);
const regularPngPath = resolve(
  migrationDirectory,
  "performance/results/public-fixtures-v1/bp-image-checker-v1.png",
);
const manifest = JSON.parse(readFileSync(manifestPath, "utf8"));
if (
  manifest.schemaVersion !== 1 ||
  manifest.purpose !== "prototype-development-only" ||
  manifest.productionApproved !== false ||
  manifest.apiBuild !== 7881
) {
  throw new Error("development PDFium manifest does not match the reviewed policy");
}
const asset = manifest.assets.find((candidate) => candidate.target === target);
if (!asset || !/^[0-9a-f]{64}$/.test(asset.sha256)) {
  throw new Error(`development PDFium has no checksum pin for ${target}`);
}
const artifactRoot = resolve(galleryDirectory, "target", "pdfium-development", target);
const receiptPath = resolve(artifactRoot, ".butter-paper-pdfium-development.json");
const expectedReceipt = {
  schemaVersion: 1,
  target: asset.target,
  archiveSha256: asset.sha256,
  archiveBytes: asset.bytes,
  apiBuild: manifest.apiBuild,
  library: asset.library,
};
const receipt = JSON.parse(readFileSync(receiptPath, "utf8"));
if (JSON.stringify(receipt) !== JSON.stringify(expectedReceipt)) {
  throw new Error("development PDFium receipt does not match the checksum-pinned manifest");
}
const version = readFileSync(resolve(artifactRoot, "VERSION"), "utf8");
if (!version.includes(`BUILD=${manifest.apiBuild}\n`)) {
  throw new Error(`development PDFium VERSION is not build ${manifest.apiBuild}`);
}
const libraryPath = resolve(artifactRoot, asset.library);
if (!existsSync(libraryPath) || !statSync(libraryPath).isFile()) {
  throw new Error(`development PDFium library is missing: ${libraryPath}`);
}
const canonicalLibrary = realpathSync(libraryPath);
if (ambientLibrary && realpathSync(ambientLibrary) !== canonicalLibrary) {
  throw new Error("BP_PDFIUM_LIBRARY is not bound to the reviewed development receipt");
}
const fixtureIndex = JSON.parse(readFileSync(fixtureIndexPath, "utf8"));
const fixture = fixtureIndex.fixtures.find((candidate) => candidate.fixture_id === fixtureId);
if (!fixture) throw new Error(`the reviewed fixture index has no ${fixtureId} entry`);
const fixtureBytes = readFileSync(fixturePath);
const sha256 = (bytes) => createHash("sha256").update(bytes).digest("hex");
const fixtureSha256 = sha256(fixtureBytes);
if (
  fixture.artifacts.pdf.sha256 !== fixtureSha256 ||
  fixture.artifacts.pdf.bytes !== fixtureBytes.byteLength
) {
  throw new Error(`${fixtureId}.pdf does not match the reviewed fixture index`);
}
const regularPng = fixtureIndex.assets.find((candidate) => candidate.id === "bp-image-checker-v1");
if (!regularPng) throw new Error("the reviewed fixture index has no bp-image-checker-v1 asset");
const regularPngBytes = readFileSync(regularPngPath);
const regularPngSha256 = sha256(regularPngBytes);
if (
  regularPng.file !== "bp-image-checker-v1.png" ||
  regularPng.sha256 !== regularPngSha256 ||
  regularPng.bytes !== regularPngBytes.byteLength ||
  regularPng.width_pixels !== 512 ||
  regularPng.height_pixels !== 384
) {
  throw new Error("bp-image-checker-v1.png does not match the reviewed fixture index");
}
process.stdout.write(JSON.stringify({
  binding: {
    target,
    apiBuild: manifest.apiBuild,
    archiveSha256: asset.sha256,
    fixtureId,
    fixtureSha256,
    libraryPath: canonicalLibrary,
    fixturePath,
  },
  assets: {
    regularPng: {
      id: regularPng.id,
      path: regularPngPath,
      sha256: regularPngSha256,
      bytes: regularPngBytes.byteLength,
      widthPixels: regularPng.width_pixels,
      heightPixels: regularPng.height_pixels,
    },
  },
  hashes: {
    pdfiumManifest: sha256(readFileSync(manifestPath)),
    pdfiumReceipt: sha256(readFileSync(receiptPath)),
    pdfiumLibrary: sha256(readFileSync(canonicalLibrary)),
    fixtureIndex: sha256(readFileSync(fixtureIndexPath)),
    fixture: fixtureSha256,
    regularPng: regularPngSha256,
  },
}));
NODE
  ) || exit 2
  BP_PDFIUM_LIBRARY=$(node -e 'process.stdout.write(JSON.parse(process.argv[1]).binding.libraryPath)' "$runtime_verification_json")
  export BP_PDFIUM_LIBRARY
  printf 'runtime-verification=%s\n' "$runtime_verification_json" >>"$raw_log"
fi
wall_seconds=$(node -e 'const p=require(process.argv[1]); process.stdout.write(String(p.wallSeconds))' "$probe_dir/build-guard-policy.json")
rust_min_stack_bytes=$(node -e 'const p=require(process.argv[1]); process.stdout.write(String(p.rustMinStackBytes))' "$probe_dir/build-guard-policy.json")

unset RUSTUP_TOOLCHAIN
cd "$probe_dir" || exit 2
setsid env \
  -u RUSTUP_TOOLCHAIN \
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
if [[ "$focused_real_pdf_mode" == "true" && "$status" == "0" ]]; then
  runtime_artifact_hashes_json=$(node --input-type=module - "$target_dir" "$raw_log" "$runner_mode" 2>>"$raw_log" <<'NODE'
import { createHash } from "node:crypto";
import { readFileSync, realpathSync, statSync } from "node:fs";
import { basename, dirname, resolve } from "node:path";

const [targetDirectory, logPath, runnerMode] = process.argv.slice(2);
const sha256File = (path) => createHash("sha256").update(readFileSync(path)).digest("hex");
const workerPath = realpathSync(resolve(targetDirectory, "debug/butter-paper-pdf-worker"));
if (!statSync(workerPath).isFile()) throw new Error("the focused PDF worker is not a file");
const log = readFileSync(logPath, "utf8");
const testTarget = runnerMode.startsWith("native-shell-")
  ? "application_close_integration"
  : "document_workspace";
const testPattern = new RegExp(`Running tests/${testTarget}\\.rs \\(([^)\\n]+)\\)`, "g");
const matches = [...log.matchAll(testPattern)];
if (matches.length !== 1) {
  throw new Error(`expected one exact ${testTarget} test binary, found ${matches.length}`);
}
const testPath = realpathSync(matches[0][1]);
const expectedDirectory = realpathSync(resolve(targetDirectory, "debug/deps"));
if (
  dirname(testPath) !== expectedDirectory ||
  !(new RegExp(`^${testTarget}-[0-9a-f]+$`).test(basename(testPath))) ||
  !statSync(testPath).isFile()
) {
  throw new Error("the focused test executable escaped its fixed Cargo target");
}
process.stdout.write(JSON.stringify({
  pdfWorker: { path: workerPath, sha256: sha256File(workerPath) },
  testExecutable: { path: testPath, sha256: sha256File(testPath) },
}));
NODE
  )
  artifact_hash_status=$?
  if (( artifact_hash_status != 0 )); then
    runtime_artifact_hashes_json='{}'
    limit_reason="runtime-artifact-hash"
    post_success_failure_status=2
    status=$post_success_failure_status
    printf 'runtime-artifact-hash-failure=status-%s\n' "$artifact_hash_status" >>"$raw_log"
  else
    printf 'runtime-artifact-hashes=%s\n' "$runtime_artifact_hashes_json" >>"$raw_log"
  fi
fi
end_epoch=$(date +%s)
if [[ "$status" == "124" && -z "$limit_reason" ]]; then
  limit_reason="wall-time"
elif [[ -n "$post_success_failure_status" ]]; then
  status=$post_success_failure_status
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
