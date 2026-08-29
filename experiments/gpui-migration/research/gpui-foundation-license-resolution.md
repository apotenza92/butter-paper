# GPUI foundation license resolution

Date: 2026-08-21

Issue: [#56 — Resolve the distributable GPUI foundation and pin the native
toolchain](https://github.com/apotenza92/butter-paper/issues/56)

## Result

This is historical direct-Zed investigation. Its fork recommendation is
superseded by the accepted exact GPUI-CE candidate documented in
[`gpui-adoption-source-strategies.md`](gpui-adoption-source-strategies.md) and
[`FOUNDATION.md`](../gpui-gallery/FOUNDATION.md).

The current foundation still fails the distribution gate. The exact pinned
graph includes `ztracing` and `zlog` as normal dependencies on all six target
triples. Their manifests declare `GPL-3.0-or-later`. It also includes the
GPL-marked `ztracing_macro` procedural macro in the build graph. Removing
`profiler`, `runtime_shaders`, `rfd`, GPUI default features, and test features
does not remove this chain.

No newer official Zed revision and no existing GPUI feature selection resolves
the failure as of this audit. Zed `main` is seven commits ahead of the pin at
[`91bf967e279fba3b326c096aeb66053cb2373547`](https://github.com/zed-industries/zed/commit/91bf967e279fba3b326c096aeb66053cb2373547).
Its manifests have the same unconditional dependencies and license fields. One
of the seven commits expands `ztracing` use for GPUI on the web rather than
removing it ([comparison](https://github.com/zed-industries/zed/compare/f4178619acd0d47ea1f76a2025c42962c6d6638c...91bf967e279fba3b326c096aeb66053cb2373547)).

Two packages previously reported as “unknown” need a narrower classification:
`gpui_shared_string` and `gpui_util` omit the Cargo `license` field, but each
directory contains only an Apache license symlink to Zed's root Apache text.
That is authoritative source-level license evidence, but it is still an
automated metadata defect. It should be resolved with checked provenance and a
scanner clarification, not waived as an unknown license.

The most practical compatible path that keeps the current GPUI platform split
is a small, reviewed fork patch against the exact pin. The patch must remove
the `ztracing` instrumentation from the Apache-licensed `gpui` and `sum_tree`
crates and remove the resulting dependency chain. A release pin must then use
the fork's full commit SHA and a regenerated lockfile. This is a proposed
resolution, not a completed or validated implementation.

The official crates.io `gpui 0.2.2` package and the last pre-`ztracing`
official Git revision provide compatible fallback graphs. Both predate
`gpui_platform`, require a source port back to the monolithic application API,
and are thousands of commits behind the experiment. They are not drop-in
upgrades.

This report is an engineering license/provenance audit, not legal advice. Any
decision to distribute GPL-covered code or change Butter Paper's license needs
explicit legal and product approval.

## Exact audited baseline

The worktree changed during the audit. The hashes below identify the state that
was re-audited after the concurrent feature cleanup:

| Input | Audited value |
| --- | --- |
| Gallery manifest | [`gpui-gallery/Cargo.toml`](../gpui-gallery/Cargo.toml), SHA-256 `2536f9bab32fb552f6f88cb6be3051f3a82a11ce3f81cebd9151de96e2cf0e25` |
| Gallery lockfile | [`gpui-gallery/Cargo.lock`](../gpui-gallery/Cargo.lock), SHA-256 `8c27e8a3104b13d5598b4f9556b863b1d3350beb377abaaa0f99d361ed943d4c` |
| Lockfile packages | 710 total; 23 from the Zed Git source |
| Zed/GPUI revision | [`f4178619acd0d47ea1f76a2025c42962c6d6638c`](https://github.com/zed-industries/zed/commit/f4178619acd0d47ea1f76a2025c42962c6d6638c) |
| GPUI packages | `gpui 0.2.2`; `gpui_platform 0.1.0` |
| Direct production features | GPUI defaults off; `gpui_platform/font-kit`, `wayland`, and `x11` |
| Removed from production selection | `gpui/profiler`, `gpui_platform/runtime_shaders`, `rfd`, and test-support |
| Zed Rust toolchain | Rust `1.97.1` from the pinned [`rust-toolchain.toml`](https://github.com/zed-industries/zed/blob/f4178619acd0d47ea1f76a2025c42962c6d6638c/rust-toolchain.toml) |

The 23 Zed packages are `collections`, `derive_refineable`, `gpui`,
`gpui_apple`, `gpui_linux`, `gpui_macos`, `gpui_macros`, `gpui_platform`,
`gpui_shared_string`, `gpui_util`, `gpui_web`, `gpui_wgpu`, `gpui_windows`,
`http_client`, `media`, `perf`, `refineable`, `scheduler`, `sum_tree`,
`util_macros`, `zlog`, `ztracing`, and `ztracing_macro`.

The gallery package now declares `license = "MIT"`, consistent with the
repository [MIT license](../../../LICENSE). The package itself is no longer an
unmarked root in Cargo license output.

## Runtime and build graph result

The audit used Cargo's locked, target-filtered normal dependency graph:

```sh
cargo tree --locked --offline \
  --manifest-path experiments/gpui-migration/gpui-gallery/Cargo.toml \
  --target TARGET \
  -e normal \
  --format '{p}|{l}|{r}'
```

The command was repeated for these target triples:

| Target | Rust 1.97.1 support tier | Result |
| --- | --- | --- |
| `aarch64-apple-darwin` | Tier 1 with host tools | Same five Zed findings |
| `x86_64-apple-darwin` | Tier 2 with host tools | Same five Zed findings |
| `aarch64-pc-windows-msvc` | Tier 1 with host tools | Same five Zed findings |
| `x86_64-pc-windows-msvc` | Tier 1 with host tools | Same five Zed findings |
| `aarch64-unknown-linux-gnu` | Tier 1 with host tools | Same five Zed findings; `self_cell` has the selectable `Apache-2.0 OR GPL-2.0-only` expression |
| `x86_64-unknown-linux-gnu` | Tier 1 with host tools | Same five Zed findings; `self_cell` has the selectable `Apache-2.0 OR GPL-2.0-only` expression |

The tier values and operating-system floors come from the frozen Rust 1.97.1
[platform table](https://doc.rust-lang.org/1.97.1/rustc/platform-support.html).
Rust's [Apple target page](https://doc.rust-lang.org/1.97.1/rustc/platform-support/apple-darwin.html)
requires Clang and potentially Xcode/macOS SDKs for cross-compilation. Its
[MSVC target page](https://doc.rust-lang.org/1.97.1/rustc/platform-support/windows-msvc.html)
says non-Windows-to-MSVC cross-compilation is not supported. The
[`aarch64-unknown-linux-gnu` page](https://doc.rust-lang.org/1.97.1/rustc/platform-support/aarch64-unknown-linux-gnu.html)
says that target can be cross-compiled from any host with the required C
toolchain. Rust target tier is not evidence that GPUI, native libraries, or a
packaged Butter Paper application works on that target.

`self_cell` is not a GPL blocker because its SPDX expression offers Apache-2.0
as a distribution choice. A policy that rejects any string containing `GPL`
will produce a false positive; the implementation must parse SPDX expressions.

## Every GPL-marked or metadata-unknown finding

### `gpui_shared_string 0.1.0`

- Cargo result: no license metadata.
- Runtime path: direct normal dependency of `gpui` on every target.
- Primary license evidence: the package directory has
  [`LICENSE-APACHE`](https://github.com/zed-industries/zed/blob/f4178619acd0d47ea1f76a2025c42962c6d6638c/crates/gpui_shared_string/LICENSE-APACHE),
  a symlink to the repository's
  [Apache-2.0 text](https://github.com/zed-industries/zed/blob/f4178619acd0d47ea1f76a2025c42962c6d6638c/LICENSE-APACHE).
- Metadata evidence: its pinned
  [`Cargo.toml`](https://github.com/zed-industries/zed/blob/f4178619acd0d47ea1f76a2025c42962c6d6638c/crates/gpui_shared_string/Cargo.toml)
  omits `license`.
- Resolution: record Apache-2.0 through a checksum-bound scanner
  clarification. The pinned Apache text SHA-256 is
  `752daf2fb234ca4a1fa372c073fe127f44b7b90fd2529ae44273a64f9d53da7a`.
  Also request that upstream add `license = "Apache-2.0"`.

### `gpui_util 0.1.0`

- Cargo result: no license metadata.
- Runtime paths: direct normal dependency of `gpui` and dependencies of other
  GPUI platform crates on every target.
- Primary license evidence: the package directory has
  [`LICENSE-APACHE`](https://github.com/zed-industries/zed/blob/f4178619acd0d47ea1f76a2025c42962c6d6638c/crates/gpui_util/LICENSE-APACHE),
  a symlink to the same root Apache-2.0 text.
- Metadata evidence: its pinned
  [`Cargo.toml`](https://github.com/zed-industries/zed/blob/f4178619acd0d47ea1f76a2025c42962c6d6638c/crates/gpui_util/Cargo.toml)
  omits `license`.
- Resolution: use the same checksum-bound Apache-2.0 clarification and request
  complete upstream Cargo metadata.

Zed contributor `notpeter` explained that the per-crate license symlink is the
applicable license and listed the Apache exceptions in the open
[license clarification issue](https://github.com/zed-industries/zed/issues/14753#issuecomment-2877036928).
The pinned Zed license check also requires each first-party crate to carry a
`LICENSE-GPL` or `LICENSE-APACHE` symlink
([script](https://github.com/zed-industries/zed/blob/f4178619acd0d47ea1f76a2025c42962c6d6638c/script/check-licenses)).
This evidence resolves the source license, but not the missing Cargo metadata.

### `ztracing 0.1.0`

- Cargo result: `GPL-3.0-or-later` in the pinned
  [`Cargo.toml`](https://github.com/zed-industries/zed/blob/f4178619acd0d47ea1f76a2025c42962c6d6638c/crates/ztracing/Cargo.toml).
- Runtime paths: unconditional normal dependency of `gpui`; unconditional
  normal dependency of `sum_tree`, which is itself a normal `gpui` dependency.
- Transitive effect: `ztracing` unconditionally depends on `zlog` and
  `ztracing_macro`.
- License-file ambiguity: the directory contains both Apache and GPL symlinks.
  That does not create an SPDX dual-license expression. The manifest says GPL,
  and Zed's later relicensing PR explicitly names `ztracing` as GPL
  ([PR #57948](https://github.com/zed-industries/zed/pull/57948)). Treat it as
  GPL unless the copyright holder publishes a clear compatible license grant.
- Resolution: remove it from the distributed graph, obtain an explicit
  compatible upstream license grant, or make an approved GPL product-license
  decision. Do not clarify or waive it as Apache.

### `zlog 0.1.0`

- Cargo result: `GPL-3.0-or-later` in the pinned
  [`Cargo.toml`](https://github.com/zed-industries/zed/blob/f4178619acd0d47ea1f76a2025c42962c6d6638c/crates/zlog/Cargo.toml).
- Runtime path: unconditional normal dependency of `ztracing`; therefore it is
  in all six normal target graphs.
- License files: GPL only.
- Resolution: removing `ztracing` removes the runtime path. `sum_tree` also
  lists `zlog` as a development dependency; remove that entry in a fork if the
  automated policy scans the full lockfile rather than the distributable normal
  graph.

### `ztracing_macro 0.1.0`

- Cargo result: `GPL-3.0-or-later` in the pinned
  [`Cargo.toml`](https://github.com/zed-industries/zed/blob/f4178619acd0d47ea1f76a2025c42962c6d6638c/crates/ztracing_macro/Cargo.toml).
- Build path: normal procedural-macro dependency of `ztracing`. Cargo marks it
  `(proc-macro)` because it runs on the build host; it is not a separately
  linked runtime library.
- Distribution effect: it is still source and build provenance for the
  shipped binary, and the current acceptance criterion does not permit it.
- License-file ambiguity: like `ztracing`, the directory has Apache and GPL
  symlinks, but its manifest and PR #57948 say GPL.
- Resolution: removing `ztracing` removes this macro from the build graph.

The public license-clarification issue remains open. A later question about the
multiple `ztracing` license files received no direct maintainer reply
([question](https://github.com/zed-industries/zed/issues/14753#issuecomment-4218240756)).
PR #57948 is newer and is the stronger statement of upstream intent: it says
that `ztracing` was relicensed under GPL while Apache components were
unchanged. The remaining Apache symlink is unresolved repository ambiguity,
not a safe basis for an Apache-only distribution decision.

## Why feature selection does not fix it

The current manifest already applies the useful feature reduction:

- `default-features = false` on `gpui` and `gpui_platform`;
- no `gpui/profiler`;
- no `gpui_platform/runtime_shaders`;
- no `rfd`;
- `test-support` only in development dependencies;
- explicit Linux `font-kit`, `wayland`, and `x11` platform features.

The pinned `gpui` manifest declares `ztracing.workspace = true` as an ordinary,
non-optional dependency. The pinned `sum_tree` manifest does the same
([`gpui/Cargo.toml`](https://github.com/zed-industries/zed/blob/f4178619acd0d47ea1f76a2025c42962c6d6638c/crates/gpui/Cargo.toml),
[`sum_tree/Cargo.toml`](https://github.com/zed-industries/zed/blob/f4178619acd0d47ea1f76a2025c42962c6d6638c/crates/sum_tree/Cargo.toml)).
No GPUI feature controls either edge. `ZTRACING` is an environment-controlled
compile-time configuration inside `ztracing`; leaving it unset changes the
instrumentation behavior but does not remove the crate or its GPL dependencies.

Therefore a Cargo feature-only resolution does not exist at the pin.

## Newer official revision check

The audited pin was committed on 2026-08-20. At audit time, official Zed `main`
resolved to `91bf967e279fba3b326c096aeb66053cb2373547`, seven commits ahead. The current
[`gpui` manifest](https://github.com/zed-industries/zed/blob/91bf967e279fba3b326c096aeb66053cb2373547/crates/gpui/Cargo.toml),
[`sum_tree` manifest](https://github.com/zed-industries/zed/blob/91bf967e279fba3b326c096aeb66053cb2373547/crates/sum_tree/Cargo.toml),
[`ztracing` manifest](https://github.com/zed-industries/zed/blob/91bf967e279fba3b326c096aeb66053cb2373547/crates/ztracing/Cargo.toml),
[`zlog` manifest](https://github.com/zed-industries/zed/blob/91bf967e279fba3b326c096aeb66053cb2373547/crates/zlog/Cargo.toml),
and
[`ztracing_macro` manifest](https://github.com/zed-industries/zed/blob/91bf967e279fba3b326c096aeb66053cb2373547/crates/ztracing_macro/Cargo.toml)
preserve the same edges and GPL fields. Rust remains `1.97.1`.

Conclusion: there is no newer official revision to pin for this resolution.
Recheck this fact immediately before implementation because `main` is moving.

## Concrete resolution options

### Option A — Compatible fork patch at the current pin

This is the preferred technical experiment if Butter Paper needs the current
`gpui_platform` split.

Create a reviewable fork commit whose parent is exactly
`f4178619acd0d47ea1f76a2025c42962c6d6638c`. The patch surface at that revision
is small and exact:

1. Remove `ztracing.workspace = true` from `crates/gpui/Cargo.toml`.
2. Remove two `#[ztracing::instrument(skip_all)]` attributes from
   `crates/gpui/src/svg_renderer.rs`.
3. Remove `ztracing.workspace = true`, `tracing.workspace = true`, and the
   related cargo-shear ignore from `crates/sum_tree/Cargo.toml`.
4. Remove two `use ztracing::instrument` imports and seven
   `#[instrument(skip_all)]` attributes from `crates/sum_tree/src/cursor.rs`
   and `crates/sum_tree/src/sum_tree.rs`.
5. Remove the `sum_tree` development-only `zlog` edge if the policy audits the
   whole resolved lockfile.
6. Pin both `gpui` and `gpui_platform` to the resulting full fork commit SHA.
   Regenerate and commit `Cargo.lock`.

At the current default configuration, the `ztracing` attribute macro is a
no-op. Removing these nine attributes therefore preserves the experiment's
non-`ZTRACING` behavior more closely than replacing them with always-active
`tracing::instrument`. Do not copy or relicense implementation from the GPL
crates. Keep the forked Apache license notices.

Required proof before acceptance:

- target-filtered normal graphs contain no `ztracing`, `zlog`, or
  `ztracing_macro`;
- the full-lock policy either contains no GPL-only package or explicitly
  proves that non-distributed development packages are outside the policy;
- checksum-bound Apache clarifications cover only `gpui_shared_string` and
  `gpui_util`;
- source-build and runtime probes pass natively on the six target/architecture
  lanes, with X11 and Wayland tested separately;
- a clean checkout fetches the fork by commit and reproduces the lockfile;
- the patch is rebased and re-audited for every GPUI revision change.

Unresolved: this patch was not built in this research task. It needs upstream
API and behavior review, and it creates a maintained downstream fork.

### Option B — Upstream the same dependency removal or relicensing

Ask Zed to make `ztracing` optional and default-off for GPUI consumers, remove
it, or grant an explicit compatible license for `ztracing`, `zlog`, and
`ztracing_macro`. Also ask Zed to add Apache SPDX fields to
`gpui_shared_string` and `gpui_util`.

No exact resolving upstream commit exists. Do not point the experiment at
`main` while waiting. Keep the current rejected pin until a concrete upstream
commit is reviewed and all probes pass.

### Option C — Official monolithic `gpui 0.2.2` crate

The official crates.io package is Apache-2.0 and includes its Apache license
([packaged manifest](https://docs.rs/crate/gpui/0.2.2/source/Cargo.toml.orig),
[license](https://docs.rs/crate/gpui/0.2.2/source/LICENSE-APACHE)). Its Cargo
checksum is
`979b45cfa6ec723b6f42330915a1b3769b930d02b2d505f9697f8ca602bee707`.
A temporary locked graph with defaults off and explicit `font-kit`,
`runtime_shaders`, `wayland`, and `x11` contained no GPL-only or missing-license
normal dependency on the six target triples. Its internal `gpui_* 0.2.2`
packages declare Apache-2.0.

This is not the same source boundary as the current experiment:

- it is monolithic; crates.io has no official `gpui_platform 0.1.0` package;
- it uses `Application::new()` instead of the current
  `gpui_platform::application()` boundary;
- its packaged VCS record names
  [`69e2130295c2649963eb639fc70b4f2ee8ea1624`](https://github.com/zed-industries/zed/commit/69e2130295c2649963eb639fc70b4f2ee8ea1624)
  but marks the package dirty
  ([`.cargo_vcs_info.json`](https://docs.rs/crate/gpui/0.2.2/source/.cargo_vcs_info.json));
- that source commit pins Rust `1.90`, not `1.97.1`;
- transitive versions remain reproducible only when Butter Paper commits the
  generated lockfile.

A host `cargo check` probe with Rust 1.93.1 did not complete because the
disposable filesystem filled while compiling dependencies. This is a blocked
probe, not a compile failure in GPUI. No native macOS, Windows, ARM64, X11, or
Wayland runtime probe was run.

### Option D — Last official Git revision before runtime `ztracing`

Zed added the normal `sum_tree -> ztracing` edge in
[PR #44147](https://github.com/zed-industries/zed/pull/44147), merge commit
`b558be7ec60b265837e34d6f9b6f0ef176c20082`. Its parent,
[`07fe8e9bb1484b2771d8a9d80f7fc370cee9c4ac`](https://github.com/zed-industries/zed/commit/07fe8e9bb1484b2771d8a9d80f7fc370cee9c4ac),
is the exact last revision before that runtime edge. A temporary target-filtered
normal graph at this revision, with the same explicit platform features, had no
GPL-only or missing-license dependency on all six triples. `sum_tree` still had
a development-only `zlog` dependency.

This revision pins Rust `1.91.1` and is 6,322 commits behind the experiment
pin. Zed did not extract `gpui_platform` until
[PR #49277](https://github.com/zed-industries/zed/pull/49277) on 2026-02-19,
after `ztracing` entered the graph. Therefore no official split-platform Git
revision exists in the clean interval. This option requires a monolithic API
port and carries a large maintenance and defect-backport burden.

### Option E — Approved GPL distribution

Butter Paper could make a separate product and legal decision to comply with
GPL-3.0-or-later for the combined distribution. That decision is outside issue
#56's engineering scope and conflicts with the current MIT-only distribution
assumption. Dynamic linking or process separation is not a demonstrated escape
here: `ztracing` is integrated into the Rust build graph, and this audit does
not establish a legally independent work.

## Toolchain and target pin required after license resolution

Whichever compatible source option is selected, pin these inputs under the
migration experiment:

| Input | Required pin |
| --- | --- |
| Rust for current-pin fork | `1.97.1`, matching Zed's exact pin |
| Rust components | `rustfmt`, `clippy`; add only components actually used by deterministic checks |
| macOS targets | `aarch64-apple-darwin`, `x86_64-apple-darwin`; pin Xcode/SDK and deployment target in native CI |
| Windows targets | `aarch64-pc-windows-msvc`, `x86_64-pc-windows-msvc`; pin Visual Studio 2022 workload and Windows SDK in native CI |
| Linux targets | `aarch64-unknown-linux-gnu`, `x86_64-unknown-linux-gnu`; pin distribution image, glibc floor, compiler/linker, Vulkan, fontconfig, X11, and Wayland development packages |
| Cargo features | GPUI defaults off; platform `font-kit`, `wayland`, and `x11` explicit; test-support only in tests |
| Source | Full compatible Git SHA or exact crates.io version plus checksum; committed `Cargo.lock` |

Do not use one cross-compile result as the six-target acceptance gate. Windows
MSVC and Apple linking need their native vendor toolchains. Linux X11 and
Wayland need separate compile and runtime lanes. An ARM64 compile result is not
an ARM64 runtime result.

The pin should be paired with reproducible commands for:

1. SPDX-aware license and notice generation on each target-filtered normal
   graph.
2. Git/crates.io provenance with source SHA or package checksum and license
   file checksum.
3. A vulnerability scan against the committed lockfile, with policy and
   database timestamp recorded.
4. Locked clean-checkout compilation for all six triples.
5. Native startup, rendering, input, accessibility, X11, and Wayland probes in
   the platform qualification lanes.

## Evidence status

Passed:

- Re-audited the current manifest and lockfile after their concurrent feature
  cleanup.
- Resolved target-filtered normal graphs for all six target triples.
- Traced every GPL-marked or Cargo-metadata-unknown Zed package to pinned
  manifests and license files.
- Verified that current feature reduction does not remove the GPL chain.
- Checked the exact current official `main` revision and its manifests.
- Resolved compatible normal graphs for the crates.io and pre-`ztracing`
  fallback candidates.
- Verified Rust 1.97.1 target tiers and cross-toolchain constraints from
  official Rust documentation.
- The repository `pnpm check` gate passed: repository hygiene, generated-icon
  verification, type checking, builds, 143 test files with 1,081 tests, and the
  signature-relay suite with 2 files and 23 tests. It ran under Node 22.22.1
  and emitted the repository's Node 24.16.0 engine warning.
- Markdown whitespace validation passed. All three local links resolved, and
  all 35 external citations returned HTTP 200 during validation.

Failed:

- The current pinned normal graph fails the MIT-compatible distribution gate
  because `ztracing` and `zlog` are GPL-3.0-or-later.
- The current build graph also contains the GPL-marked `ztracing_macro`.

Blocked:

- The crates.io fallback host compile probe stopped when the disposable
  filesystem ran out of space during dependency compilation. It did not reach
  GPUI compilation.
- No authoritative compatible license grant exists for the three GPL-marked
  crates.

Not run:

- No fork patch was implemented or compiled.
- No vulnerability scan was run.
- No native macOS, Windows, ARM64, X11, Wayland, packaging, or runtime probe was
  run.

## Acceptance decision

Issue #56 is not complete. Do not waive the current result and do not call the
foundation distributable.

To close the license portion while keeping current GPUI, select Option A or
obtain Option B, record the exact resulting commit, and prove the regenerated
graph. Option C or D can close the license portion only after an explicit
decision to accept the older monolithic API and after all compile/runtime gates
pass. The toolchain and six native target gates remain required under every
option.
