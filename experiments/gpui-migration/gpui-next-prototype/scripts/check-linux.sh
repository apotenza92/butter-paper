#!/usr/bin/env bash
set -euo pipefail

prototype_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

# The pinned upstream revisions use two standard-library APIs that are newer
# than the VPS Rust 1.93 toolchain. Keep the compatibility flags local to this
# throwaway experiment; Rust 1.96 and later do not need them.
export RUSTC_BOOTSTRAP=1
export RUSTFLAGS="-Zcrate-attr=feature(cold_path,atomic_try_update)"

cargo check --locked --manifest-path "${prototype_root}/Cargo.toml"
cargo test --locked --manifest-path "${prototype_root}/Cargo.toml"
