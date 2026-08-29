#!/usr/bin/env bash
set -euo pipefail

if (( $# > 1 )); then
  echo "usage: $0 [PDF path]" >&2
  exit 2
fi

while IFS= read -r environment_name; do
  case "$environment_name" in
    BP_GPUI_PERF_*|BP_PDF_WORKER_EXE)
      echo "native reader refuses ambient benchmark variable: $environment_name" >&2
      exit 2
      ;;
  esac
done < <(compgen -e)

script_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
probe_dir=$(cd "$script_dir/.." && pwd)
migration_dir=$(cd "$probe_dir/.." && pwd)
target_dir="$migration_dir/.build-targets/gpui-component-compat/debug"
story="$target_dir/component_story"
worker="$target_dir/butter-paper-pdf-worker"
gallery_dir="$migration_dir/gpui-gallery"
pdfium_manifest="$gallery_dir/pdfium-development-binaries.json"
default_fixture="$migration_dir/performance/results/public-fixtures-v1/bp-multi-page-v1.pdf"

if [[ ! -x "$story" ]]; then
  echo "native reader binary is missing or not executable: $story" >&2
  echo "run the guarded native-reader-build mode first" >&2
  exit 1
fi
if [[ ! -x "$worker" ]]; then
  echo "PDF worker is missing or not executable: $worker" >&2
  exit 1
fi
pdfium_library=$(node --input-type=module - "$gallery_dir" "$pdfium_manifest" <<'NODE'
import { existsSync, readFileSync, statSync } from "node:fs";
import { resolve } from "node:path";

const [galleryDirectory, manifestPath] = process.argv.slice(2);
if (process.platform !== "linux") {
  throw new Error(`the native reader launcher currently supports Linux, not ${process.platform}`);
}
const targets = {
  arm64: "aarch64-unknown-linux-gnu",
  x64: "x86_64-unknown-linux-gnu",
};
const target = targets[process.arch];
if (!target) throw new Error(`unsupported Linux architecture ${process.arch}`);

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
let receipt;
try {
  receipt = JSON.parse(readFileSync(receiptPath, "utf8"));
} catch {
  throw new Error(`development PDFium verification receipt is missing or invalid: ${receiptPath}`);
}
if (JSON.stringify(receipt) !== JSON.stringify(expectedReceipt)) {
  throw new Error("development PDFium receipt does not match the checksum-pinned manifest");
}
const version = readFileSync(resolve(artifactRoot, "VERSION"), "utf8");
if (!version.includes(`BUILD=${manifest.apiBuild}\n`)) {
  throw new Error(`development PDFium VERSION is not build ${manifest.apiBuild}`);
}
const library = resolve(artifactRoot, asset.library);
if (!existsSync(library) || !statSync(library).isFile()) {
  throw new Error(`development PDFium library is missing: ${library}`);
}
process.stdout.write(library);
NODE
)

pdf_path="$default_fixture"
if (( $# == 0 )); then
  if [[ ! -f "$default_fixture" ]]; then
    echo "default public multi-page fixture is missing: $default_fixture" >&2
    exit 1
  fi
else
  if [[ ! -f "$1" ]]; then
    echo "PDF path is not a file: $1" >&2
    exit 1
  fi
  case "$1" in
    *.[Pp][Dd][Ff]) ;;
    *) echo "PDF path must end in .pdf: $1" >&2; exit 1 ;;
  esac
  pdf_path=$(realpath -- "$1")
fi

exec env \
  BP_NATIVE_DEVELOPMENT=1 \
  BP_PDFIUM_LIBRARY="$pdfium_library" \
  "$story" --open "$pdf_path"
