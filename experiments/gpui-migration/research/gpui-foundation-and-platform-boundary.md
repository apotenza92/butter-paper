# GPUI foundation and platform support boundary

Date: 2026-08-21

Issue: [#44 — Choose the maintained GPUI foundation and platform support boundary](https://github.com/apotenza92/butter-paper/issues/44)

## Recommended decision

This is the historical direct-Zed platform baseline. Its source decision is
superseded by the accepted exact GPUI-CE candidate documented in
[`gpui-adoption-source-strategies.md`](gpui-adoption-source-strategies.md) and
[`FOUNDATION.md`](../gpui-gallery/FOUNDATION.md). The native qualification
matrix below remains applicable until it is rerun against GPUI-CE.

Do **not** approve the current GPUI pin for a distributed Butter Paper build yet.
Keep it as the exact experiment baseline while two prerequisites are resolved:

1. The runtime license graph must become compatible with Butter Paper's current
   MIT distribution policy, or the project must make an explicit, legally
   reviewed license change.
2. The exact pin must pass Butter Paper's native build, runtime, accessibility,
   input method editor (IME), packaging, updater, and document-integrity gates
   on every supported operating system, architecture, and Linux windowing
   backend.

The current candidate is Zed commit
[`f4178619acd0d47ea1f76a2025c42962c6d6638c`](https://github.com/zed-industries/zed/commit/f4178619acd0d47ea1f76a2025c42962c6d6638c),
committed on 2026-08-20. The local experiment pins that full revision for both
`gpui` and `gpui_platform`, and its lockfile resolves both packages to the same
revision ([manifest](../gpui-gallery/Cargo.toml),
[lockfile](../gpui-gallery/Cargo.lock)). Do not replace the revision with a
branch, tag, wildcard, or crates.io `*` requirement.

If the two prerequisites pass, use this conditional baseline:

| Item | Conditional production decision |
| --- | --- |
| GPUI revision | `f4178619acd0d47ea1f76a2025c42962c6d6638c`, with `Cargo.lock` committed |
| Rust | Rust `1.97.1`, minimal rustup profile, plus `rustfmt` and `clippy`; use the target-specific Apple, MSVC, and GNU build tools described below |
| Direct Zed dependencies | `gpui` and `gpui_platform` only |
| Production features | `gpui_platform/font-kit`, `gpui_platform/wayland`, and `gpui_platform/x11` |
| Non-production features | `gpui/test-support` and `gpui_platform/test-support` only in tests; `gpui/profiler` only in benchmark builds |
| Excluded features | Exclude `runtime_shaders` from release builds; compile and embed the macOS Metal library at build time |
| Supported architectures | `aarch64` and `x86_64` for macOS, Windows, and Linux, subject to native qualification |
| Linux graphics | Vulkan is the supported production path; do not claim the WGPU OpenGL path until it has its own native qualification |
| Linux sessions | Qualify X11 and Wayland separately; neither result stands in for the other |
| Component library | Butter Paper-owned controls on GPUI; no Zed `ui`, `gpui-component`, or `gpui-base` dependency |

This is a conditional technical baseline, not permission to ship. The license
finding below is currently a rejection trigger.

## Evidence boundary

- GPUI identifies itself as active-development, pre-1.0 software with frequent
  breaking changes. Its own setup guidance says to use the latest stable Rust
  and shows `gpui_platform` with `font-kit`, `wayland`, and `x11`
  ([GPUI README at the pin](https://github.com/zed-industries/zed/blob/f4178619acd0d47ea1f76a2025c42962c6d6638c/crates/gpui/README.md)).
- At the pin, the package versions are `gpui 0.2.2` and `gpui_platform 0.1.0`.
  Both package manifests say `Apache-2.0`
  ([`gpui/Cargo.toml`](https://github.com/zed-industries/zed/blob/f4178619acd0d47ea1f76a2025c42962c6d6638c/crates/gpui/Cargo.toml),
  [`gpui_platform/Cargo.toml`](https://github.com/zed-industries/zed/blob/f4178619acd0d47ea1f76a2025c42962c6d6638c/crates/gpui_platform/Cargo.toml)).
- The pinned Zed workspace selects Rust `1.97.1`, edition 2024, with a minimal
  toolchain plus `rustfmt`, `clippy`, `rust-analyzer`, and `rust-src`
  ([`rust-toolchain.toml`](https://github.com/zed-industries/zed/blob/f4178619acd0d47ea1f76a2025c42962c6d6638c/rust-toolchain.toml),
  [workspace package settings](https://github.com/zed-industries/zed/blob/f4178619acd0d47ea1f76a2025c42962c6d6638c/Cargo.toml)).
  The GPUI manifests do not declare a `rust-version`, so there is no lower
  Minimum Supported Rust Version (MSRV) contract. A successful build on an
  older compiler is useful experiment evidence, not a maintained toolchain
  promise.
- The previous experiment revision, `e0931d5a...`, is 81 Zed commits behind the
  current pin. The GPUI and platform directories changed by thousands of lines
  between the two local checkouts. This is direct evidence that an upgrade must
  be reviewed as source change, not treated as a routine patch-version bump.
- Butter Paper currently ships MIT-licensed source and has release matrices for
  `arm64` and `x64` on macOS, Windows, and Linux
  ([root package metadata](../../../package.json),
  [release workflow](../../../.github/workflows/release.yml)). A GPUI migration
  must preserve those product targets unless a separate product decision
  narrows support.

## Public API surface to accept

Butter Paper can build on these public GPUI facilities at the exact pin:

| Area | Public surface | Boundary decision |
| --- | --- | --- |
| Application and windows | `gpui_platform::application()`, `Application::run`, `App`, `Window`, window options, native menus and prompts | Call only through `butter_platform`; no direct dependency on `gpui_macos`, `gpui_windows`, or `gpui_linux`. [`gpui_platform` selects the implementation by target](https://github.com/zed-industries/zed/blob/f4178619acd0d47ea1f76a2025c42962c6d6638c/crates/gpui_platform/src/gpui_platform.rs). |
| State and rendering | `Entity`, `Context`, `Render`, `RenderOnce`, `Element`, `IntoElement`, `Styled`, actions, subscriptions, and foreground/background tasks | Presentation code may use these through one internal GPUI facade. Domain models, PDF persistence, and update logic must not depend on GPUI types. See GPUI's [ownership and data-flow guide](https://github.com/zed-industries/zed/blob/f4178619acd0d47ea1f76a2025c42962c6d6638c/crates/gpui/src/_ownership_and_data_flow.rs). |
| Layout and paint | `div`, text, `img`, `svg`, `canvas`, paths, custom `Element` paint, `ScrollHandle`, `list`, and `uniform_list` | Keep page coordinates, hit testing, annotation state, raster scheduling, and scrolling policy in Butter Paper modules. GPUI owns only layout, input delivery, and paint. The public exports are collected in [`gpui.rs`](https://github.com/zed-industries/zed/blob/f4178619acd0d47ea1f76a2025c42962c6d6638c/crates/gpui/src/gpui.rs). |
| Keyboard and text input | typed actions, key bindings and contexts, `FocusHandle`, tab stops, `EntityInputHandler`, and `ElementInputHandler` | Map every menu, mouse, keyboard, and assistive-technology route to Butter Paper commands. Keep editable-text and IME behavior behind owned controls. See the [key dispatch guide](https://github.com/zed-industries/zed/blob/f4178619acd0d47ea1f76a2025c42962c6d6638c/crates/gpui/docs/key_dispatch.md) and [input example](https://github.com/zed-industries/zed/blob/f4178619acd0d47ea1f76a2025c42962c6d6638c/crates/gpui/examples/input.rs). |
| Accessibility | `Role`, `AccessibleAction`, stable element IDs, `accessibility_id`, `aria_*` builders, `on_a11y_action`, and synthetic AccessKit children | Expose an owned semantic model. Use `gpui::accesskit`, which GPUI re-exports, instead of a separately versioned runtime dependency. See the pinned [accessibility guide](https://github.com/zed-industries/zed/blob/f4178619acd0d47ea1f76a2025c42962c6d6638c/crates/gpui/src/_accessibility.rs). |
| Test support | `#[gpui::test]`, `TestAppContext`, `VisualTestAppContext`, and simulated input | Put these behind `butter_testkit`. Do not compile `test-support` into production. The pin supplies a headless renderer only on macOS, not Windows or Linux ([platform selector](https://github.com/zed-industries/zed/blob/f4178619acd0d47ea1f76a2025c42962c6d6638c/crates/gpui_platform/src/gpui_platform.rs)). |

Do not wrap every pixel, color, or style call. Such a wrapper would duplicate
GPUI without containing risk. Instead, contain the unstable lifecycle,
platform, input, accessibility, overlay, document-render, and test boundaries.
Keep ordinary Butter Paper controls in an owned `butter_ui` layer that is
allowed to use the internal facade.

## Supporting crates and feature decisions

### Keep

- Keep `gpui` and `gpui_platform` at the same full revision. Mixed revisions
  are not supported by their workspace relationship.
- Keep ordinary Rust support crates only when Butter Paper owns their purpose.
  For example, an error crate, serialization crate, or PDF engine is not part
  of the GPUI foundation and must have its own version and license review.
- Use GPUI's public `App::prompt_for_paths` and `App::prompt_for_new_path` behind
  `butter_platform`. The pin implements native panels on macOS and Windows and
  an XDG Desktop Portal request on Linux
  ([public methods](https://github.com/zed-industries/zed/blob/f4178619acd0d47ea1f76a2025c42962c6d6638c/crates/gpui/src/app.rs),
  [Linux implementation](https://github.com/zed-industries/zed/blob/f4178619acd0d47ea1f76a2025c42962c6d6638c/crates/gpui_linux/src/linux/platform.rs)).

### Remove or defer

- Remove `rfd` from the planned foundation. The local gallery uses `rfd 0.17.2`
  for its prototype picker, but GPUI now supplies the required public API. A
  second dialog stack adds modality, parenting, portal, and upgrade risk
  ([local use](../gpui-gallery/src/main.rs)).
- Do not add a direct `accesskit` dependency for production. GPUI re-exports
  the exact compatible AccessKit API. A direct dev-only dependency is acceptable
  only if a test tool cannot use the re-export.
- Do not depend on Zed's `ui` crate. It is GPL-3.0-or-later
  ([manifest](https://github.com/zed-industries/zed/blob/f4178619acd0d47ea1f76a2025c42962c6d6638c/crates/ui/Cargo.toml)).
- Do not add `gpui-component` or `gpui-base` to the foundation. They create a
  second pre-1.0 revision relationship and do not remove the need for owned
  visual, interaction, and accessibility contracts. Reconsider one isolated
  behavior only after a direct-GPUI implementation fails a written gate.
- Do not expose `raw-window-handle` or a platform renderer to document code.
  If a future PDF engine needs GPU interop, place the unsafe handle and texture
  lifetime inside `butter_render`, with a CPU-image fallback on every platform.
- Do not ship `gpui/profiler`, `test-support`, `screen-capture`, or
  `runtime_shaders` by default. `runtime_shaders` embeds Metal source and calls
  runtime compilation; the default path compiles and embeds a `.metallib`
  ([Apple build script](https://github.com/zed-industries/zed/blob/f4178619acd0d47ea1f76a2025c42962c6d6638c/crates/gpui_apple/build.rs),
  [renderer selection](https://github.com/zed-industries/zed/blob/f4178619acd0d47ea1f76a2025c42962c6d6638c/crates/gpui_apple/src/metal_renderer.rs)).

## Platform support boundary

Source support is not Butter Paper qualification. The exact pin contains all
four requested backends, but each remains conditional until the matching
packaged candidate passes on native hardware.

| Target | Pinned implementation | Proposed Butter Paper boundary | Required proof before support |
| --- | --- | --- | --- |
| macOS `aarch64` and `x86_64` | AppKit/CoreText/Metal. GPUI's Metal build sets deployment target `10.15.7`; Zed also states macOS `10.15.7` or later. Sources: [Apple build script](https://github.com/zed-industries/zed/blob/f4178619acd0d47ea1f76a2025c42962c6d6638c/crates/gpui_apple/build.rs), [Zed macOS requirements](https://github.com/zed-industries/zed/blob/f4178619acd0d47ea1f76a2025c42962c6d6638c/docs/src/macos.md). | Preserve both current architectures. Do not claim the `10.15.7` floor for Butter Paper until a packaged candidate runs there; set the product floor from the oldest qualified host. | Native build, signed/notarized package, real PDF/annotation smoke, VoiceOver, IME, multi-window/menu behavior, and N-1 update replacement on each architecture. |
| Windows `aarch64` and `x86_64` | Win32, Direct3D 11, and DirectWrite. The text implementation requires Windows 10 Creators Update 1703 or later; Zed requires a DirectX 11 GPU. Sources: [DirectWrite implementation](https://github.com/zed-industries/zed/blob/f4178619acd0d47ea1f76a2025c42962c6d6638c/crates/gpui_windows/src/direct_write.rs), [Direct3D device selection](https://github.com/zed-industries/zed/blob/f4178619acd0d47ea1f76a2025c42962c6d6638c/crates/gpui_windows/src/directx_devices.rs), [Windows requirements](https://github.com/zed-industries/zed/blob/f4178619acd0d47ea1f76a2025c42962c6d6638c/docs/src/windows.md). | Windows 10 1703 plus a Direct3D 11-capable adapter is the framework floor. Butter Paper may choose a newer product floor, but not an older one. Preserve both current architectures. | Native MSVC build, hardware and WARP behavior decision, NSIS install/launch/uninstall, Narrator and UI Automation, IME, cloud-file behavior, PDF integrity, and N-1 replacement on each architecture. |
| Linux X11, `aarch64` and `x86_64` | X11/XCB, XIM, AccessKit Unix, and WGPU. X11 is compiled by the `x11` feature. Sources: [`gpui_linux` features](https://github.com/zed-industries/zed/blob/f4178619acd0d47ea1f76a2025c42962c6d6638c/crates/gpui_linux/Cargo.toml), [X11 client](https://github.com/zed-industries/zed/blob/f4178619acd0d47ea1f76a2025c42962c6d6638c/crates/gpui_linux/src/linux/x11/client.rs). | Vulkan on a supported GNU/Linux package baseline. Keep AppImage, DEB, and RPM promises separate. Do not infer distro compatibility from a successful Ubuntu build. | Real X11 GPU session, XIM for multiple scripts, Orca/AT-SPI, clipboard/drag/drop/portal, package install/launch, and AppImage updater on each architecture. |
| Linux Wayland, `aarch64` and `x86_64` | Wayland protocols, text-input-v3 IME, AccessKit Unix, XDG Desktop Portal, and WGPU. Wayland is compiled by the `wayland` feature. Sources: [`gpui_linux` features](https://github.com/zed-industries/zed/blob/f4178619acd0d47ea1f76a2025c42962c6d6638c/crates/gpui_linux/Cargo.toml), [Wayland client](https://github.com/zed-industries/zed/blob/f4178619acd0d47ea1f76a2025c42962c6d6638c/crates/gpui_linux/src/linux/wayland/client.rs). | Vulkan on a supported GNU/Linux package baseline. Treat Wayland as a separate runtime target, not an XWayland variation of the X11 result. | Real GNOME and KDE Wayland sessions, fractional scaling, text-input-v3 IME, Orca/AT-SPI, portal dialogs, clipboard/drag/drop, package launch, and AppImage updater on each architecture. |

On Linux, `gpui::guess_compositor()` chooses Wayland whenever
`WAYLAND_DISPLAY` is non-empty, otherwise X11 when `DISPLAY` is non-empty,
otherwise headless. `gpui_linux::current_platform()` does not attempt a
Wayland-to-X11 fallback after that choice
([selection logic](https://github.com/zed-industries/zed/blob/f4178619acd0d47ea1f76a2025c42962c6d6638c/crates/gpui/src/platform.rs),
[backend construction](https://github.com/zed-industries/zed/blob/f4178619acd0d47ea1f76a2025c42962c6d6638c/crates/gpui_linux/src/linux.rs)).
Butter Paper must not change process environment variables to select a backend.
If supported machines need a user-selectable backend or automatic fallback,
require an upstream public API or carry a small reviewed platform patch behind
`butter_platform`.

The Linux renderer asks WGPU for Vulkan or OpenGL adapters. Zed's published
Linux requirements and troubleshooting use Vulkan as the supported path
([WGPU context](https://github.com/zed-industries/zed/blob/f4178619acd0d47ea1f76a2025c42962c6d6638c/crates/gpui_wgpu/src/wgpu_context.rs),
[Linux requirements](https://github.com/zed-industries/zed/blob/f4178619acd0d47ea1f76a2025c42962c6d6638c/docs/src/linux.md)).
Butter Paper should therefore treat OpenGL selection as diagnostic fallback,
not production coverage, until tested explicitly.

## Accessibility boundary

GPUI integrates AccessKit with native adapters on macOS, Windows, X11, and
Wayland. The pinned platform sources instantiate `accesskit_macos`,
`accesskit_windows`, or `accesskit_unix`
([macOS adapter](https://github.com/zed-industries/zed/blob/f4178619acd0d47ea1f76a2025c42962c6d6638c/crates/gpui_macos/src/window.rs),
[Windows adapter](https://github.com/zed-industries/zed/blob/f4178619acd0d47ea1f76a2025c42962c6d6638c/crates/gpui_windows/src/window.rs),
[X11 adapter](https://github.com/zed-industries/zed/blob/f4178619acd0d47ea1f76a2025c42962c6d6638c/crates/gpui_linux/src/linux/x11/window.rs),
[Wayland adapter](https://github.com/zed-industries/zed/blob/f4178619acd0d47ea1f76a2025c42962c6d6638c/crates/gpui_linux/src/linux/wayland/window.rs)).

The framework does not create browser semantics. Butter Paper must supply:

- a stable, unique element ID and a role for every semantic node; nodes without
  both do not enter the tree;
- labels, descriptions, value/state, set and table metadata, focus, active
  descendant state, and keyboard-shortcut descriptions;
- explicit handlers for AccessKit actions other than the common behavior GPUI
  wires automatically;
- synthetic children for custom-painted page and annotation content; and
- stable IDs across frames. The pinned guide warns that duplicate global IDs
  can cause nodes to be silently dropped in release builds.

Create a `butter_a11y` semantic interface that describes controls and document
objects without exposing AccessKit IDs to the domain model. The GPUI adapter
maps this interface to `role`, `accessibility_id`, `aria_*`,
`on_a11y_action`, and synthetic nodes. Test the resulting native tree and real
actions with VoiceOver, Narrator, and Orca. A debug tree dump or a unit test is
not a substitute for assistive-technology operation.

Reject the foundation if GPUI cannot express or deliver any required control,
text-editing, document, annotation, live-status, focus, or action contract on
all four backends without depending on a private platform crate.

## License finding: current production blocker

Butter Paper is MIT-licensed ([license](../../../LICENSE)). The exact Zed
source says it is primarily GPL-3.0-or-later, with Apache-2.0 components where
marked
([Zed README](https://github.com/zed-industries/zed/blob/f4178619acd0d47ea1f76a2025c42962c6d6638c/README.md)).
Although the public GPUI and platform manifests are marked Apache-2.0, the
locked runtime graph is not Apache-only:

- `gpui` directly depends on `ztracing`; `sum_tree`, another `gpui` dependency,
  also depends on `ztracing`
  ([`gpui` manifest](https://github.com/zed-industries/zed/blob/f4178619acd0d47ea1f76a2025c42962c6d6638c/crates/gpui/Cargo.toml),
  [`sum_tree` manifest](https://github.com/zed-industries/zed/blob/f4178619acd0d47ea1f76a2025c42962c6d6638c/crates/sum_tree/Cargo.toml)).
- `ztracing` and its runtime dependency `zlog` are each marked
  `GPL-3.0-or-later`
  ([`ztracing` manifest](https://github.com/zed-industries/zed/blob/f4178619acd0d47ea1f76a2025c42962c6d6638c/crates/ztracing/Cargo.toml),
  [`zlog` manifest](https://github.com/zed-industries/zed/blob/f4178619acd0d47ea1f76a2025c42962c6d6638c/crates/zlog/Cargo.toml)).
- `gpui_shared_string` and `gpui_util` are runtime dependencies with no license
  field, so the repository's “Apache where marked” rule does not establish an
  Apache grant for them
  ([`gpui_shared_string` manifest](https://github.com/zed-industries/zed/blob/f4178619acd0d47ea1f76a2025c42962c6d6638c/crates/gpui_shared_string/Cargo.toml),
  [`gpui_util` manifest](https://github.com/zed-industries/zed/blob/f4178619acd0d47ea1f76a2025c42962c6d6638c/crates/gpui_util/Cargo.toml)).

Reproduce the runtime graph from the local experiment with:

```sh
cd experiments/gpui-migration/gpui-gallery
cargo tree --locked --offline --features gallery -i ztracing
cargo tree --locked --offline --features gallery -i zlog
```

This research does not give legal advice. The engineering decision is still
clear: do not distribute the candidate under the current MIT policy until the
copyright holders mark every required GPUI runtime crate with a compatible
license or provide a reviewed exception. The alternatives are an explicit
project-wide GPL distribution decision or a separately reviewed foundation;
do not assume that deleting a dependency line in a fork resolves the license
of copied source.

For every candidate revision, generate the complete target-specific license
inventory for all six architecture triples and package the required license
and notice texts. Reject a revision when any runtime crate is GPL-only,
unlicensed, has an unknown source, or has a license incompatible with the
approved Butter Paper distribution policy.

## Butter Paper-owned adapter seams

| Owned seam | Responsibilities | GPUI exposure rule |
| --- | --- | --- |
| `butter_domain` | document state, annotations, undo/redo, save/import/export contracts, commands | No GPUI or AccessKit types. |
| `butter_ui` | Nova tokens, controls, overlays, focus policy, keyboard behavior, accessible component semantics | Uses only the internal GPUI facade; never imports Zed `ui` or platform crates. |
| `butter_shell` | windows, tabs, rails, menus, dialogs, command routing, constrained layout | Consumes `butter_ui` and `butter_platform`; no direct native calls. |
| `butter_render` | PDF engine trait, raster cache, generation/cancellation, page images, canvas paint, coordinate transforms, GPU interop fallback | GPUI image/canvas types stop at the final presentation adapter. No GPUI types in the PDF engine. |
| `butter_a11y` | stable semantic IDs, roles, names, states, relationships, actions, document/annotation tree | Maps to GPUI's AccessKit re-export in one place. Domain objects keep stable Butter Paper IDs, not `NodeId`. |
| `butter_platform` | application lifecycle, windows, files, menus, clipboard, appearance, notifications, secure storage, file association, identity, packaging and updater | Only module allowed to call `gpui_platform` or use an approved platform patch. GPUI does not replace Butter Paper's release security boundary. |
| `butter_testkit` | GPUI contexts, semantic selectors, accessibility-tree assertions, native harness hooks, capture | Test features do not enter release builds. |

The renderer and application shell will necessarily use GPUI concepts. The
goal is not to make GPUI interchangeable. The goal is to keep document
integrity, persistence, security, release, and semantic contracts independent
of a fast-changing pre-1.0 API.

## Upgrade policy

1. **Pin one source and one toolchain.** Commit the full 40-character GPUI
   revision, its `Cargo.lock`, and a Butter Paper Rust toolchain file matching
   the selected source. Never follow `main`, a moving tag, or `*`.
2. **Review monthly; upgrade only deliberately.** Check upstream security and
   platform fixes at least monthly. Do not auto-merge revision bumps. An urgent
   security or correctness fix can start an out-of-cycle review.
3. **Use one revision bump per change.** Record old and new commits, inspect all
   changes to `gpui*` and every transitive Zed crate, and list adapter changes.
   Do not mix a GPUI upgrade with product feature work.
4. **Run dependency gates first.** Regenerate the target-specific dependency,
   license, notice, vulnerability, source, and unsafe-code inventories. Stop
   before porting if policy fails.
5. **Run deterministic gates.** Format, lint, build, and test all six target
   triples. Run owned UI, command, input, accessibility-tree, document
   round-trip, cancellation, stale-result, and renderer tests.
6. **Run native backend gates.** Test packaged candidates on macOS, Windows,
   X11, and Wayland, on both architectures. Include VoiceOver, Narrator, Orca,
   multiple IMEs, fractional and integer scaling, constrained windows, two-axis
   scroll, real PDFs, all annotation round trips, GPU recovery, file dialogs,
   clipboard, drag/drop, and application menus.
7. **Run release gates.** Preserve stable/beta identities, signing,
   notarization, installer/package behavior, TUF verification, and native N-1
   replacement. GPUI success is not updater success.
8. **Measure and compare.** Repeat the accepted startup, first-page, scroll,
   zoom, memory, and interaction benchmarks. Reject material regressions unless
   the product explicitly accepts them.
9. **Promote only the exact tested revision.** Keep the prior approved pin as
   the rollback point until the new release completes its observation period.
   Fix forward with another reviewed revision; do not patch published assets.

## Rejection and reconsideration triggers

Reject the current or a future GPUI foundation when any of these is true:

- the runtime license graph contains GPL-only, unlicensed, unknown-source, or
  otherwise unapproved code under Butter Paper's distribution policy;
- any required target cannot build with the pinned toolchain and supported
  vendor build tools;
- macOS/Windows/Linux `aarch64` or `x86_64`, X11, or Wayland requires a private
  GPUI platform API in ordinary product code;
- a supported Linux session needs backend override or fallback and GPUI cannot
  provide it through a maintainable public API or small reviewed patch;
- a supported GPU/driver cannot render reliably and no acceptable qualified
  fallback exists;
- VoiceOver, Narrator, or Orca cannot inspect and operate the required control,
  document, annotation, text-editing, focus, or status semantics;
- keyboard, pointer, clipboard, drag/drop, or IME behavior cannot match the
  current product contract;
- PDF content, save/import/export compatibility, annotation round trips, or
  stale-render cancellation regress;
- packaging, signing, application identity, TUF update verification, or native
  N-1 replacement cannot preserve the current release contract;
- the required Rust or vendor toolchain cannot be maintained on all release
  runners; or
- GPUI abandons an Apache-compatible, publicly consumable core, removes one of
  the four required backends, or makes the adapter patch set grow beyond a
  small independently testable boundary.

Reconsider a different foundation if a rejection trigger remains after one
bounded upstream-or-adapter attempt. Do not keep a long-lived private fork that
silently becomes Butter Paper's window system, renderer, text engine, and
accessibility platform.

## Evidence status

- **Passed:** The exact pin and lock resolution were inspected. The platform,
  accessibility, toolchain, feature, renderer, and license sources cited above
  were inspected at that revision. The local dependency graph confirms the
  `ztracing -> zlog` runtime path.
- **Failed:** The candidate fails the current MIT license acceptance gate. A
  focused `cargo check -j 1 --locked --features gallery --bin
  butter-paper-gpui-gallery` also fails on this host's Rust `1.93.1`: the
  experiment enables `gpui/profiler`, and that pinned source uses
  `std::hint::cold_path`, which this compiler still reports as unstable. This
  confirms that the host's older compiler is not evidence for the pinned
  workspace's Rust `1.97.1`; it does not establish a failure on `1.97.1`.
- **Blocked:** Production approval is blocked on an authoritative license
  clarification/change, availability of the exact Rust toolchain, and the full
  native qualification matrix.
- **Not run:** macOS, Windows, X11, Wayland, cross-architecture packaged-app,
  assistive-technology, IME, signing, updater, N-1, and release tests were not
  run for this research task. Historical experiment captures and builds are
  not current proof of those gates.
