# GPUI foundation advisory resolution

Date: 2026-08-21

Issue: [#56 — Resolve the distributable GPUI foundation and pin the native
toolchain](https://github.com/apotenza92/butter-paper/issues/56)

## Result

This is historical direct-Zed investigation. The active foundation decision
now pins GPUI-CE; see
[`gpui-adoption-source-strategies.md`](gpui-adoption-source-strategies.md) and
[`FOUNDATION.md`](../gpui-gallery/FOUNDATION.md). The earlier local compatibility
patch was removed after GPUI-CE passed the independent foundation gate.

The exact patched GPUI candidate still contains `paste 1.0.15`,
`rustybuzz 0.20.1`, and `ttf-parser 0.25.1`. All three RustSec records are
informational **unmaintained** notices, not known-vulnerability records, and
list no patched versions. The migration gate keeps them visible as warnings
while it continues to deny vulnerabilities, yanked packages, and ignored
advisories.

No newer official Zed revision clears the findings. A contained downstream
patch can clear `rustybuzz` and `ttf-parser` and remove the Linux `paste` path
without replacing GPUI's renderer or text system:

1. Remove Zed's `image/exr` feature.
2. Upgrade official `resvg` and `usvg` from `0.46.0` to `0.48.1`.
3. Replace GPUI's one direct `ttf-parser` lookup with `skrifa`.
4. Patch `cosmic-text 0.19.0` from `fontdb 0.23` to `0.24`, matching an open
   upstream change.

A disposable Linux x86_64 probe compiled this combination through the full
GPUI Linux platform graph. Its Linux normal graph contained none of the three
packages. The macOS graph still contains `paste` through `metal 0.33.0`, so
this does not clear the six-target union. This is not an accepted source pin or
cross-platform qualification.

## Exact baseline and paths

The historical Zed compatibility patch did not alter these paths, so the
advisory subgraph was identical before and after that license fix.

| Input | Audited value |
| --- | --- |
| Manifest | [`gpui-gallery/Cargo.toml`](../gpui-gallery/Cargo.toml), SHA-256 `2536f9bab32fb552f6f88cb6be3051f3a82a11ce3f81cebd9151de96e2cf0e25` |
| Lockfile | [`gpui-gallery/Cargo.lock`](../gpui-gallery/Cargo.lock), SHA-256 `8c27e8a3104b13d5598b4f9556b863b1d3350beb377abaaa0f99d361ed943d4c` |
| Zed source | [`f4178619acd0d47ea1f76a2025c42962c6d6638c`](https://github.com/zed-industries/zed/commit/f4178619acd0d47ea1f76a2025c42962c6d6638c) |
| Toolchain | Rust `1.97.1` |
| Newer Zed checked | `main` at [`91bf967e279fba3b326c096aeb66053cb2373547`](https://github.com/zed-industries/zed/commit/91bf967e279fba3b326c096aeb66053cb2373547) |

Locked offline inverse-tree checks found each package on all six issue targets.
The `cosmic-text` path is Linux-only, but other `ttf-parser` paths are common.

| Advisory and classification | Exact normal path | Feature or source cause |
| --- | --- | --- |
| [`RUSTSEC-2024-0436`](https://rustsec.org/advisories/RUSTSEC-2024-0436.html), `INFO Unmaintained` | `gpui -> image 0.25.10 -> exr 1.74.2 -> pulp 0.22.3 -> paste 1.0.15` | Zed explicitly enables `image/exr` in its [workspace manifest](https://github.com/zed-industries/zed/blob/f4178619acd0d47ea1f76a2025c42962c6d6638c/Cargo.toml#L638-L655). `paste` is a compile-time procedural macro. |
| Same `paste` notice on Apple targets | `gpui/gpui_apple/gpui_macos -> core-video or metal -> metal 0.33.0 -> paste 1.0.15` | Removing `image/exr` does not remove this macOS build path. Official `metal 0.33.0` still declares `paste`. |
| [`RUSTSEC-2026-0206`](https://rustsec.org/advisories/RUSTSEC-2026-0206.html), `INFO Unmaintained` | `gpui -> usvg 0.46.0 -> rustybuzz 0.20.1`; also through `gpui -> resvg 0.46.0 -> usvg` | Zed enables `resvg/text` in its [workspace manifest](https://github.com/zed-industries/zed/blob/f4178619acd0d47ea1f76a2025c42962c6d6638c/Cargo.toml#L771-L776); `usvg/text` selects Rustybuzz. |
| [`RUSTSEC-2026-0192`](https://rustsec.org/advisories/RUSTSEC-2026-0192.html), `INFO Unmaintained` | Direct `gpui -> ttf-parser`; through `rustybuzz` and `fontdb 0.23 -> usvg`; plus Linux `gpui_wgpu -> cosmic-text 0.19 -> fontdb 0.23` | GPUI has one direct [character-map lookup](https://github.com/zed-industries/zed/blob/f4178619acd0d47ea1f76a2025c42962c6d6638c/crates/gpui/src/svg_renderer.rs#L42-L49). The other paths come from SVG text and Linux text rendering. |

## Resolution evidence

### `paste`: remove EXR; patch `pulp` only if EXR is required

Removing only `image/exr` eliminates the image-owned path. The cost is loss of
OpenEXR decoding. Butter Paper has no documented OpenEXR requirement. It does
not eliminate the Apple `metal -> paste` path.

The original [`paste` repository](https://github.com/dtolnay/paste) is archived.
The active [`pastey 0.2.3`](https://github.com/AS1100K/pastey/releases/tag/v0.2.3)
fork documents a drop-in alias. Clearing the six-target union would require a
reviewed immutable `metal` patch to use that alias; if OpenEXR is retained,
`pulp` needs the same change. Test generated SIMD and Metal bindings on x86_64
and ARM64. Current official releases retain both paths.

### `rustybuzz`: take the official Resvg migration

The Rustybuzz maintainer declared the project deprecated, directed users to
HarfRust, and approved the RustSec notice in
[issue 166](https://github.com/harfbuzz/rustybuzz/issues/166). Its archived
repository's latest release remains
[`v0.20.1`](https://github.com/harfbuzz/rustybuzz/releases/tag/v0.20.1).

Official [`resvg/usvg 0.48.1`](https://github.com/linebender/resvg/releases/tag/v0.48.1)
replaces Rustybuzz and `ttf-parser` with `harfrust` and `skrifa`. The
[changelog](https://github.com/linebender/resvg/blob/v0.48.1/CHANGELOG.md#0480-2026-07-31)
warns of small rendering changes and about 500 KiB added optimized size. The
merged [migration PR](https://github.com/linebender/resvg/pull/922) also found
longer compilation. Its Rust 1.85 minimum is below the issue's pin.

Disabling `resvg/text` would break SVG text and GPUI's custom fallback. Upgrade
instead, then compare text, emoji, fallback, variable/color fonts, and icons.

### `ttf-parser`: resolve three independent seams

The Resvg update removes the SVG-owned paths, but two seams remain:

- GPUI's one direct call can use `skrifa 0.44`, already selected by
  `usvg 0.48.1`: `FontRef::from_index(...).charmap().map(ch)`. The probe
  compiled this small substitution.
- `cosmic-text 0.19.0` already uses HarfRust and Skrifa but still selects
  `fontdb 0.23`. Open upstream
  [PR 526](https://github.com/pop-os/cosmic-text/pull/526), exact head
  `ea83c272ea892a2bdb3fbe28521df87d91d6596c`, changes it to `fontdb 0.24` and
  removes a temporary ignore. Its build and `cargo-deny` checks pass, but it is
  unmerged. The same one-line package patch compiled in the probe.

`ttf-parser` has nuanced current status. Its original author called it
unmaintained in [issue 217](https://github.com/harfbuzz/ttf-parser/issues/217),
which also contains an unconfirmed algorithmic-complexity concern. Another
repository member later offered to review it, but the latest release is still
[`v0.25.1` from 2024-11-29](https://github.com/harfbuzz/ttf-parser/releases/tag/v0.25.1),
and RustSec lists no patched version. Branch activity is not a released fix.

## Probe result and limits

The disposable probe applied all four changes and ran `cargo check` for a
minimal Linux consumer with `font-kit`, `wayland`, and `x11`. It passed, and
inverse Linux normal-dependency checks found none of the three packages. A
separate target-union check showed that macOS still selects `paste` through
`metal 0.33.0`.

The host had Rust 1.93.1. Pinned Zed uses two unrelated
`std::hint::cold_path` calls stabilized after that host version, so the probe
removed those two hints. The accepted Rust 1.97.1 source does not require this
probe-only edit.

Not run: accepted fork/lockfile, `cargo-deny 0.20.2` (not installed), other
targets, packaged tests, SVG comparison, or Butter Paper size measurements.

## Recommendation for issue #56

- Keep the three records visible as warnings with `--warn unmaintained`;
  continue denying vulnerabilities and yanked packages.
- Do not add permanent advisory ignores or describe these informational records
  as known vulnerabilities.
- Keep the existing no-op tracing-license fix separate from the SVG/font-stack
  upgrade. The latter has visual risk and still cannot clear `paste` without a
  reviewed `metal` patch.
- Treat the four Linux-proven changes as a later maintained-stack candidate.
  Regenerate its lockfile and compare SVG/font output before adoption.
- Run `cargo-deny 0.20.2`, the six required native compile probes, deterministic
  SVG/font tests, and native visual checks before accepting the foundation.
- If Butter Paper chooses a strict zero-unmaintained policy, keep issue #56
  blocked until Zed, `cosmic-text`, and `metal` publish equivalent changes or
  all three immutable patches are reviewed. Under the recorded warning policy,
  these notices do not by themselves block the license/toolchain foundation.

This is a contained dependency and call-site update with demonstrated Linux
source compatibility. It is not a broad renderer/text port. Its principal
remaining risk is rendering drift from the official Resvg font-stack change,
not unresolved GPUI API breakage.
