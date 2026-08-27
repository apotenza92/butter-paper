#!/usr/bin/env bash
set -euo pipefail

prototype_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

cargo run \
  --locked \
  --manifest-path "${prototype_root}/Cargo.toml" \
  --features visual-capture \
  --bin capture-shells
