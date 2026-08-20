# Butter Paper GPUI migration review

This directory is the first migration milestone. It is independent from the
tracked Butter Paper application and is preserved on the dedicated
`codex/gpui-component-migration-spike` branch so the experiment can continue on
another machine without changing production sources.

The branch contains the review pages, standalone Rust source, and build and
benchmark scripts. To keep the cross-machine handoff small, visual captures and
other reproducible output are deliberately excluded: Rust `target/`, the
generated application bundle, Poppler runtime, renderer caches, temporary
Electron user data, the nested runtime source copy, raw machine-specific
benchmark reports, and the Hibbeler PDF corpus. The HTML keeps the expected
capture filenames so a later macOS capture pass can restore its visual evidence
without changing the review structure.

## Open the review

From this directory, run:

```sh
python3 -m http.server 4177
```

Then open <http://127.0.0.1:4177/index.html>.

The review includes actual Electron development-runtime captures from commit `9e947ae4b43eb05c15e60b1ef9bb6c7f16444081`, source-mapped static proposals, a full UI inventory, a risk register, and an independent Rust/GPUI component gallery.

`component-gates.html` is the component-by-component parity review. It uses direct crops from the actual Electron Hibbeler frame and the fresh `gpui-native-current-1152x768.png` frame. It records source mappings, interaction and accessibility contracts, responsive behavior, and pixel checkpoints for primitives, tabs, toolbar groups, both rails, thumbnails, and constrained windows.

## Default-first GPUI policy

GPUI is a UI framework, not a stock desktop widget library. The pinned crate provides layout and interactive elements, focus and actions, native application menus, text-input plumbing, virtual lists, images and surfaces, canvas/path painting, windows, and test contexts. It does not provide finished Button, Checkbox, Select, Slider, Tabs, Dialog, Tooltip, or Popover widgets with a default visual theme.

The proposal therefore uses this order:

1. Use a GPUI element or platform API directly when it supplies the required behavior.
2. Add a thin Butter Paper wrapper for ordinary controls, accessibility state, and the current Nova metrics.
3. Add a custom element only for the document viewport, PDF surface behavior, annotation painting, or another measured domain requirement.
4. Do not introduce a separate “GPUI look.” Preserve the current 32 px targets, shell spacing, meaningful icons, and neutral focus treatment unless a reviewed product change has independent value.

`pixel-overlay.html` is the visual-parity harness. It loads the current Electron captures by state and accepts a local GPUI screenshot with identical dimensions. It supports alpha overlay, blink, side-by-side inspection, a difference heatmap, X/Y alignment nudges, and exact/threshold pixel metrics. The scorer can isolate the title/menu, document tabs, workspace toolbar, left navigation, document viewport, or right tool rail. The initial self-comparison should report a 100% exact RGB match.

## Build and capture the GPUI gallery

Xcode 27 beta keeps the Metal compiler in an on-demand component. The component is installed on this machine as `com.apple.dt.toolchain.Metal.32023.917`. Keep selection scoped to each command:

```sh
cd gpui-gallery
node scripts/generate-lucide-assets.mjs
TOOLCHAINS=com.apple.dt.toolchain.Metal.32023.917 \
DEVELOPER_DIR=/Applications/Xcode-beta.app/Contents/Developer \
cargo build
```

The generator reads the exact installed `lucide-react@1.8.0` package from the
local pnpm content store. It creates normal 1.5-stroke SVGs, 16 px rail variants
that reproduce Lucide `absoluteStrokeWidth`, and Butter Paper's composite
Continuous icon. GPUI loads these files with its native `AssetSource` and
`svg()` APIs. The app bundle copies the same assets into `Contents/Resources`.

Launch the gallery normally, or launch the equal-size shell capture mode:

```sh
BP_GPUI_CAPTURE_SHELL=1 \
TOOLCHAINS=com.apple.dt.toolchain.Metal.32023.917 \
DEVELOPER_DIR=/Applications/Xcode-beta.app/Contents/Developer \
./target/debug/butter-paper-gpui-gallery
```

Pass one or more PDF paths to launch the shell with real documents in closable
tabs. The current vertical slice reads page metadata with Poppler, renders the
active page on demand, and caches disposable PNG surfaces under
`gpui-gallery/target/pdf-cache/`:

```sh
BP_GPUI_CAPTURE_SHELL=1 \
TOOLCHAINS=com.apple.dt.toolchain.Metal.32023.917 \
DEVELOPER_DIR=/Applications/Xcode-beta.app/Contents/Developer \
./target/debug/butter-paper-gpui-gallery "/absolute/path/to/document.pdf"
```

The `+` action, File → Open, and Cmd/Ctrl+O open the same path through a native
file picker. Metadata, page, zoom, and thumbnail requests run on GPUI's
background executor. Viewport requests coalesce to one active job per document,
document and generation IDs reject stale completions, and visible thumbnail
misses use a maximum of two background jobs. Page Up and Page Down move between
pages. The thumbnail sidebar is a GPUI `uniform_list`: it virtualizes the
complete document, renders only the visible thumbnail range, and recenters on
the selected page. Set
`BP_GPUI_INITIAL_PAGE=8` for a deterministic page-navigation capture. Poppler is
a temporary spike backend, not the proposed packaged renderer architecture.
Set `BP_GPUI_ZOOM=400` to launch at 400%. The active raster follows the page's
declared dimensions and zoom at 2× density, capped at 4096 pixels wide for this
spike.

The viewer toolbar follows the current Electron action order and joined-control
geometry. Zoom Out and Zoom In use the production 1.1× step. The supported
range is 6.25–6400%. Fit Width and Fit Page use the production 24 px page gap
and 2% downward quantization rule against the current native window size.
Continuous and Single Page are selectable states. Cmd/Ctrl +, Cmd/Ctrl −, and
Cmd/Ctrl 0 route through GPUI actions and are also exposed in the native View
menu. The percentage and wheel-mode chevrons do not open popup menus yet.

## Launch the standalone macOS app

Build the disposable app bundle with its own Poppler runtime:

```sh
cd gpui-gallery
./scripts/build-app-bundle.sh
open "target/Butter Paper GPUI.app"
```

To launch the bundle with a PDF directly:

```sh
open -n "target/Butter Paper GPUI.app" --args "/absolute/path/to/document.pdf"
```

The current bundle is an ad-hoc signed ARM64 development artifact. It is 213 MB
because it contains the complete temporary Poppler runtime. Its PDF cache also
stays inside the disposable app bundle. Each metadata request has a five-second
safety limit and each page render has a 15-second limit; a stuck child renderer
is terminated and reported in the viewport.

The capture mode creates the same 1200×800 logical surface used by the Electron evidence. Export the native 2× window capture at 1152×768 to reproduce the Electron evidence's 0.96 logical scale. `captures/gpui-native-current-1152x768.png` is the current native frame, with side-by-side, overlay, difference, and region metrics beside it. The older `gpui-shell-*` and `comparison-*electron-vs-gpui.png` files remain historical v3 examples.

## Continuation gates

The performance gate is complete. Continue in this order, keeping each gate
independent and reviewable:

1. Capture a fresh GPUI one-document shell at 1200×800 and export it at
   1152×768. This gate is complete for the current Hibbeler fixture; see
   `captures/current-native-capture-evidence.md` and the region metrics.
2. Pass the primitive and compound gate: icon buttons, toolbar groups, tabs,
   zoom controls, and both rails. Score each region in `pixel-overlay.html` and
   verify keyboard focus, pressed/disabled state, and constrained width.
3. Pass the structural gate: real Hibbeler page surface, thumbnails, tabs,
   page navigation, and 400%/1600% zoom. Keep Poppler raster timings separate
   from GPUI frame timings.
4. Pass the domain gate with one rectangle annotation: coordinate mapping,
   handles, selection, undo, save/reopen, and accessibility semantics.
5. Only after these gates should a production-independent read-only slice be
   considered for a tracked migration proposal. Electron remains the rollback
   product until visual, behavior, accessibility, integrity, performance, and
   packaging evidence all pass.

## Evidence boundaries

- `captures/current-*.png` are actual current-runtime captures unless a page labels a state as representative.
- HTML proposals are review mockups. They are not evidence that GPUI can provide the pictured behavior. The files named `gpui-shell-*` and `comparison-*electron-vs-gpui*` come from the compiled native gallery.
- `gpui-gallery/` is a standalone source spike pinned to GPUI `0.2.2`.
- The compiled gallery can open a PDF, display and scroll a real page surface,
  hold multiple document tabs, virtualize all page thumbnails, and navigate
  through thumbnails or Page Up/Page Down. The asynchronous PDF path and stale
  request tests pass with the 935-page Hibbeler corpus. Three-iteration native
  open, page-navigation, and zoom runs now pass in an approved GUI-capable
  execution context. Some root-shell launches still hit a macOS `HIServices`
  application-registration denial; those failed attempts remain documented in
  `performance/results/gpui-native-launch-blocker.md`. Raw reports are local
  output and are intentionally not part of the portable branch.
- The gallery is not linked to production packages and does not import production source.
- The runtime copy, dependency tree, browser data, Rust target files, and new
  captures produced during later runs are disposable. The selected captures in
  this branch are the review baseline.

## Reproduce the performance comparison

`performance/protocol.md` fixes the Hibbeler identity, 1200×800 window, page and
zoom sequences, metric definitions, and interpretation boundaries. The two
standalone runners retain raw events and process samples:

```sh
node performance/electron-runner.mjs --scenario open-pdf --pdf "/absolute/path/to/Hibbeler.pdf" --iterations 5 --output performance/results/electron-open-pdf.json
node performance/gpui-runner.mjs --scenario open-pdf --pdf "/absolute/path/to/Hibbeler.pdf" --iterations 5 --output performance/results/gpui-open-pdf.json
```

Repeat with `page-navigation` and `zoom`. The GPUI runner uses a fresh disposable
cache for each cold iteration. Generate the review page from matching reports
with `performance/generate-comparison.mjs`; see `performance/README.md` for the
full command. A successful GPUI run needs functioning GUI services and approved
native app access. A launch abort before `window-created` remains a failed
iteration and cannot supply visible-frame evidence. The current successful
three-iteration reports remain local machine-specific output; the generated
`performance/comparison.html` preserves their summarized comparison.

## Reproduce the current-runtime capture

1. Set `HIBBELER_PDF` to the local Hibbeler corpus file.
2. From the repository root, restore dependencies with `CI=true npx --yes pnpm@10.33.0 install --frozen-lockfile`.
3. Build the linked packages with `CI=true npx --yes pnpm@10.33.0 --filter @butter-paper/core build` and `CI=true npx --yes pnpm@10.33.0 --filter @butter-paper/pdf build`.
4. Launch the isolated development runtime:

   ```sh
   BP_TEST_MODE=1 \
   BP_TEST_USER_DATA_DIR="$(mktemp -d)" \
   BP_DEFAULT_SAMPLE_PDF="$HIBBELER_PDF" \
   BP_DISABLE_UPDATE_CHECKS=1 \
   npx --yes pnpm@10.33.0 --filter @butter-paper/desktop dev
   ```

5. Capture the empty shell, fit-width workspace, 400% zoom, properties panel, selected annotation, thumbnail navigation, and multiple-tab states.
6. Replace only the corresponding files in `captures/` and record the new commit in `index.html`.

Transient native menus and a reliable constrained-window capture remain blocked in this pass. The review labels those visuals as representative; the one-document shell and component crops are current native evidence.
