# Direct Butter Paper migration to GPUI

Date: 2026-08-20

## Decision

Yes. Butter Paper can preserve its current visual design by rewriting its
renderer as Butter Paper-owned GPUI components. The earlier generic shells do
not show a GPUI limitation. They followed this experiment's then-current policy
of evaluating a different component system and explicitly did not target Nova
pixel parity (`experiments/gpui-migration/README.md`).

The correct visual-parity experiment is therefore not another generic shell.
It is an exact reconstruction of the current Electron/Nova interface using
Butter Paper tokens, geometry, icons, interaction states, and domain behavior.
GPUI supplies the rendering and application framework. Butter Paper supplies
the design system.

This is feasible, but it is a full application rewrite rather than a mechanical
React-to-Rust translation. The main feasibility gates are PDF rendering,
editable text and input method editor (IME) behavior, accessible overlays,
annotation interactions, and native packaging/updating. Basic shell rendering
is not the hard part.

## Evidence boundary

- The experiment pins Zed/GPUI commit
  [`e0931d5a9dbf4f781b336fdf448739e74a2ac0b5`](https://github.com/zed-industries/zed/tree/e0931d5a9dbf4f781b336fdf448739e74a2ac0b5),
  dated 2026-08-17.
- The latest locally fetched official Zed source on 2026-08-20 is
  `f4178619acd0d47ea1f76a2025c42962c6d6638c`, 81 commits ahead but only three
  days newer. Current platform selection still includes macOS, Linux, and
  Windows in
  [`crates/gpui_platform/src/gpui_platform.rs`](https://github.com/zed-industries/zed/blob/f4178619acd0d47ea1f76a2025c42962c6d6638c/crates/gpui_platform/src/gpui_platform.rs).
- GPUI describes itself as active-development, pre-1.0, and subject to frequent
  breaking changes. Butter Paper must pin a revision and isolate GPUI APIs
  behind owned modules. See the pinned
  [`crates/gpui/README.md`](https://github.com/zed-industries/zed/blob/e0931d5a9dbf4f781b336fdf448739e74a2ac0b5/crates/gpui/README.md).
- The Butter Paper comparison uses the current local implementation. It does
  not treat historical experiment captures as current proof.

## What “direct migration” means

A direct migration should have four owned layers:

1. `butter_ui`: visual tokens and ordinary controls that recreate the current
   Nova interface.
2. `butter_shell`: tabs, rails, toolbars, sidebars, menus, dialogs, and command
   routing composed from `butter_ui`.
3. `butter_document`: page layout, bitmap presentation, virtualization,
   scrolling, annotation painting, hit testing, selection, and editing.
4. `butter_platform`: files, application menus, windows, secure storage,
   updater, application identity, and packaging-specific services.

React components, hooks, CSS, Base UI behavior, and Zustand state cannot be
imported into GPUI. Their contracts can be retained, then implemented as Rust
types, GPUI entities, actions, and elements.

## GPUI capability assessment

| Area | Official GPUI evidence | Butter Paper implication | Assessment |
| --- | --- | --- | --- |
| Styling and layout | Views build element trees with a Tailwind-style API. `Styled` includes flex, grid, sizes, min/max, spacing, position, overflow, backgrounds, borders, radii, shadows, opacity, and typography. Low-level `Element` implementations own layout and painting. Sources: [`element.rs`](https://github.com/zed-industries/zed/blob/e0931d5a9dbf4f781b336fdf448739e74a2ac0b5/crates/gpui/src/element.rs), [`styled.rs`](https://github.com/zed-industries/zed/blob/e0931d5a9dbf4f781b336fdf448739e74a2ac0b5/crates/gpui/src/styled.rs). | Translate `apps/desktop/src/renderer/src/styles.css`, `shellSpacing.ts`, and generated component geometry into Rust theme constants and component builders. CSS cannot be reused. | Supported. Exact geometry is realistic. |
| Fonts and text | `Text`, `StyledText`, `TextSystem`, shaped lines, font runs, wrapping, alignment, ellipsis, and embedded font registration are available. Sources: [`elements/text.rs`](https://github.com/zed-industries/zed/blob/e0931d5a9dbf4f781b336fdf448739e74a2ac0b5/crates/gpui/src/elements/text.rs), [`text_system.rs`](https://github.com/zed-industries/zed/blob/e0931d5a9dbf4f781b336fdf448739e74a2ac0b5/crates/gpui/src/text_system.rs), [`examples/text.rs`](https://github.com/zed-industries/zed/blob/e0931d5a9dbf4f781b336fdf448739e74a2ac0b5/crates/gpui/examples/text.rs). | Bundle the same Geist and annotation fonts. Native shaping/rasterization will not produce byte-identical Chromium pixels on every operating system, so compare geometry and bounded pixel differences. | Supported with cross-engine pixel differences. |
| Editable text | `EntityInputHandler` and `ElementInputHandler` connect a custom control to platform text input. The official input example implements selection, caret movement, Unicode boundaries, clipboard, and IME behavior. Sources: [`input.rs`](https://github.com/zed-industries/zed/blob/e0931d5a9dbf4f781b336fdf448739e74a2ac0b5/crates/gpui/src/input.rs), [`examples/input.rs`](https://github.com/zed-industries/zed/blob/e0931d5a9dbf4f781b336fdf448739e74a2ac0b5/crates/gpui/examples/input.rs). | Inputs and editable text boxes are deep controls, not styled `div`s. Reimplementing them raw is possible but high risk. | Supported, high effort. |
| SVG and icons | `svg()` accepts asset paths, external paths, or raw bytes and supports transformations. Source: [`elements/svg.rs`](https://github.com/zed-industries/zed/blob/e0931d5a9dbf4f781b336fdf448739e74a2ac0b5/crates/gpui/src/elements/svg.rs). | Reuse the current Lucide SVGs and Butter Paper AEC, Fit Width, Fit Page, and Continuous assets. Preserve their reviewed sizes and stroke geometry. | Supported. |
| Images and PDF page bitmaps | `img()` accepts resources, decoded images, and `RenderImage`; it supports object-fit and image caching. Sources: [`elements/img.rs`](https://github.com/zed-industries/zed/blob/e0931d5a9dbf4f781b336fdf448739e74a2ac0b5/crates/gpui/src/elements/img.rs), [`assets.rs`](https://github.com/zed-industries/zed/blob/e0931d5a9dbf4f781b336fdf448739e74a2ac0b5/crates/gpui/src/assets.rs). | A native PDF renderer can hand RGBA or decoded page images to GPUI. CSS page filters in `styles.css` need preprocessing or custom rendering. | Bitmap presentation supported. PDF decoding is not supplied. |
| Custom canvas and annotations | `canvas()`, `PathBuilder`, and `Window::paint_*` support custom quads, fills, strokes, curves, arcs, gradients, glyphs, images, and SVG. Sources: [`elements/canvas.rs`](https://github.com/zed-industries/zed/blob/e0931d5a9dbf4f781b336fdf448739e74a2ac0b5/crates/gpui/src/elements/canvas.rs), [`path_builder.rs`](https://github.com/zed-industries/zed/blob/e0931d5a9dbf4f781b336fdf448739e74a2ac0b5/crates/gpui/src/path_builder.rs), [`examples/painting.rs`](https://github.com/zed-industries/zed/blob/e0931d5a9dbf4f781b336fdf448739e74a2ac0b5/crates/gpui/examples/painting.rs). | Rectangle, ellipse, arc, line, polygon, ink, cloud, selection handles, snap markers, and overlays can all be drawn directly. Port the existing coordinate and hit-test contracts rather than redesigning them. | Supported, but the largest domain port. |
| External GPU surfaces | GPUI's `Surface` source is a `CVPixelBuffer` only on macOS in this revision. Source: [`elements/surface.rs`](https://github.com/zed-industries/zed/blob/e0931d5a9dbf4f781b336fdf448739e74a2ac0b5/crates/gpui/src/elements/surface.rs). | Do not plan around zero-copy cross-platform PDF textures. Start with cached `RenderImage` page rasters or build and qualify a custom renderer. | Cross-platform gap. |
| Scrolling and virtualization | `ScrollHandle`, two-axis overflow scrolling, variable-height `list`, and optimized equal-height `uniform_list` are present. Sources: [`elements/div.rs`](https://github.com/zed-industries/zed/blob/e0931d5a9dbf4f781b336fdf448739e74a2ac0b5/crates/gpui/src/elements/div.rs), [`elements/list.rs`](https://github.com/zed-industries/zed/blob/e0931d5a9dbf4f781b336fdf448739e74a2ac0b5/crates/gpui/src/elements/list.rs), [`elements/uniform_list.rs`](https://github.com/zed-industries/zed/blob/e0931d5a9dbf4f781b336fdf448739e74a2ac0b5/crates/gpui/src/elements/uniform_list.rs). | Thumbnails fit `uniform_list`. The document viewport still needs Butter Paper's two-axis scrollbars, zoom anchoring, fit calculations, page modes, overscan, and raster scheduling. | Supported primitives; domain behavior must be ported. |
| Menus, popovers, dialogs, tooltips | Core GPUI provides deferred and anchored painting, tooltips, native application menus, file prompts, actions, and window prompts. Sources: [`examples/popover.rs`](https://github.com/zed-industries/zed/blob/e0931d5a9dbf4f781b336fdf448739e74a2ac0b5/crates/gpui/examples/popover.rs), [`examples/set_menus.rs`](https://github.com/zed-industries/zed/blob/e0931d5a9dbf4f781b336fdf448739e74a2ac0b5/crates/gpui/examples/set_menus.rs), [`app.rs`](https://github.com/zed-industries/zed/blob/e0931d5a9dbf4f781b336fdf448739e74a2ac0b5/crates/gpui/src/app.rs). | GPUI core does not supply complete shadcn/Base UI widgets. Butter Paper must own placement, collision handling, dismissal order, modality, focus return, keyboard navigation, constrained-window behavior, and styling. | Mechanisms supported; complete controls are application work. |
| Keyboard and focus | Actions, key contexts, keymaps, `FocusHandle`, tab stops, tab indexes, focus tracking, and focus-visible styling are built in. Source: [`docs/key_dispatch.md`](https://github.com/zed-industries/zed/blob/e0931d5a9dbf4f781b336fdf448739e74a2ac0b5/crates/gpui/docs/key_dispatch.md). | Map existing shortcuts to semantic actions. Mouse and native menu paths must dispatch the same action. Implement roving focus and focus traps in compound controls. | Supported. |
| Accessibility | GPUI integrates AccessKit on macOS, Windows, X11, and Wayland. It supports roles, labels, values, accessible actions, stable IDs, and synthetic children for custom-painted elements. Source: [`_accessibility.rs`](https://github.com/zed-industries/zed/blob/e0931d5a9dbf4f781b336fdf448739e74a2ac0b5/crates/gpui/src/_accessibility.rs) and platform adapter sources under `crates/gpui_{macos,windows,linux}`. | Browser semantics disappear. Every owned component and custom annotation surface needs deliberate IDs, roles, state, focus order, actions, labels, and announcements. A visually correct port can still fail this gate. | Supported but manual and high risk. |
| Pointer, drag, resize, and file drop | Interactive elements expose mouse, scroll, drag, drag-move, drop, and cursor handlers. GPUI has pointer capture and external path drop events. Sources: [`interactive.rs`](https://github.com/zed-industries/zed/blob/e0931d5a9dbf4f781b336fdf448739e74a2ac0b5/crates/gpui/src/interactive.rs), [`examples/drag_drop.rs`](https://github.com/zed-industries/zed/blob/e0931d5a9dbf4f781b336fdf448739e74a2ac0b5/crates/gpui/examples/drag_drop.rs). | Annotation placement, panning, resize handles, sidebar splits, tab drag, and dropped PDFs are implementable. Preserve capture cancellation, clamping, and keyboard alternatives. | Supported. |
| State and async work | GPUI `Entity`, `Context`, observation, typed events, subscriptions, notifications, and event-loop-integrated foreground/background tasks are first-class. Sources: [`docs/contexts.md`](https://github.com/zed-industries/zed/blob/e0931d5a9dbf4f781b336fdf448739e74a2ac0b5/crates/gpui/docs/contexts.md), [`app/context.rs`](https://github.com/zed-industries/zed/blob/e0931d5a9dbf4f781b336fdf448739e74a2ac0b5/crates/gpui/src/app/context.rs). | Port Zustand and React state to domain entities. Keep renderer jobs generation-tagged and reject stale completions, as the current application does. | Supported; architectural rewrite required. |
| Testing and capture | `#[gpui::test]`, `TestAppContext`, and visual contexts simulate actions, keystrokes, text, pointer input, clicks, resize, prompts, and clipboard. Sources: [`examples/testing.rs`](https://github.com/zed-industries/zed/blob/e0931d5a9dbf4f781b336fdf448739e74a2ac0b5/crates/gpui/examples/testing.rs), [`app/test_context.rs`](https://github.com/zed-industries/zed/blob/e0931d5a9dbf4f781b336fdf448739e74a2ac0b5/crates/gpui/src/app/test_context.rs). | Unit and interaction tests are viable. Playwright tests do not transfer. Direct rendered screenshot support is macOS-only in this pin; Linux and Windows need a real window and operating-system capture or a new backend. | Functional tests supported; cross-platform visual harness gap. |
| Platforms and packaging | `gpui_platform::application()` selects Metal/macOS, Win32/DirectWrite/Windows, and Wayland or X11/Linux. The GPUI README lists the required features. Zed uses separate project scripts for bundles, signing, notarization, Linux packages, and a Windows installer. Sources: [`gpui_platform.rs`](https://github.com/zed-industries/zed/blob/e0931d5a9dbf4f781b336fdf448739e74a2ac0b5/crates/gpui_platform/src/gpui_platform.rs), [`script/bundle-mac`](https://github.com/zed-industries/zed/blob/e0931d5a9dbf4f781b336fdf448739e74a2ac0b5/script/bundle-mac), [`script/bundle-linux`](https://github.com/zed-industries/zed/blob/e0931d5a9dbf4f781b336fdf448739e74a2ac0b5/script/bundle-linux), [`script/bundle-windows.ps1`](https://github.com/zed-industries/zed/blob/e0931d5a9dbf4f781b336fdf448739e74a2ac0b5/script/bundle-windows.ps1). | GPUI does not replace Electron Builder, signing, stable/beta identities, TUF update verification, installer behavior, or N-1 update qualification. Those need a Rust-native application and release implementation. | Runtime supported; distribution is a separate major workstream. |

## Raw GPUI versus unstyled reusable behavior

Raw GPUI is sufficient, but implementing every control from raw input and
paint APIs would repeat difficult work. There is a middle route that does not
adopt the rejected `gpui-component` visuals.

The pinned `gpui-component` repository now includes an Apache-2.0
`gpui-base` crate. Its official architecture says it exists for applications
that own a different visual system. It supplies unstyled behavior and
infrastructure for buttons, input, dialogs, focus traps, popup positioning,
tooltips, resizable panels, scrollbars, virtual lists, selection controls, and
other compound behavior. Presentation remains application-owned. Sources:

- [`crates/base/README.md`](https://github.com/longbridge/gpui-component/blob/c6ffd3e166abb43bff8845f0aa61711adb128dcf/crates/base/README.md)
- [`docs/ARCHITECTURE.md`](https://github.com/longbridge/gpui-component/blob/c6ffd3e166abb43bff8845f0aa61711adb128dcf/docs/ARCHITECTURE.md)
- [`crates/base/Cargo.toml`](https://github.com/longbridge/gpui-component/blob/c6ffd3e166abb43bff8845f0aa61711adb128dcf/crates/base/Cargo.toml)

This creates two viable choices:

| Choice | Visual ownership | Benefit | Cost/risk |
| --- | --- | --- | --- |
| Butter UI on raw GPUI | Fully Butter Paper | Fewest dependencies and total control | Butter Paper must implement and maintain input, focus traps, overlay collision, resizing, scrollbars, and all accessible control behavior. |
| Butter UI on GPUI plus selected `gpui-base` behavior | Fully Butter Paper | Reuses difficult behavior without importing the styled component appearance | Adds a pre-1.0 dependency and requires strict adapters and verification of each behavior contract. |

The first choice is the recommended starting point because the requested test
is a clean direct migration without a `gpui-component` dependency. Keep
`gpui-base` only as a documented fallback for one isolated behavior if a raw
implementation exposes a measured input or accessibility risk and the
dependency is explicitly approved. Butter Paper owns every visible component
in either case.

Zed itself proves that application-owned controls can be built on raw GPUI:
its `crates/ui/src/components` contains buttons, tabs, modals, popovers, menus,
tooltips, scrollbars, and tables. That crate is GPL-3.0-or-later while GPUI core
is Apache-2.0, so it can be studied as official implementation evidence but
must not be copied or depended on without a separate license decision. Sources:
[`Zed UI components`](https://github.com/zed-industries/zed/tree/e0931d5a9dbf4f781b336fdf448739e74a2ac0b5/crates/ui/src/components),
[`crates/ui/Cargo.toml`](https://github.com/zed-industries/zed/blob/e0931d5a9dbf4f781b336fdf448739e74a2ac0b5/crates/ui/Cargo.toml),
[`crates/gpui/Cargo.toml`](https://github.com/zed-industries/zed/blob/e0931d5a9dbf4f781b336fdf448739e74a2ac0b5/crates/gpui/Cargo.toml).

## Butter Paper scope

The current renderer contains approximately 54,359 TypeScript/TSX lines,
including tests. Product components account for approximately 17,324 TSX
lines, and the generated Nova UI layer for approximately 3,452 TSX lines. The
application imports about 30 ordinary UI primitives. These figures are a scope
inventory, not an effort estimate.

The visual primitives are not the largest part. The three hardest current
components are:

- `apps/desktop/src/renderer/src/components/AnnotationLayer.tsx`: 3,956 lines;
- `apps/desktop/src/renderer/src/components/DocumentViewport.tsx`: 3,808 lines;
- `apps/desktop/src/renderer/src/components/PageView.tsx`: 1,360 lines.

Other critical sources include:

- `apps/desktop/src/renderer/src/styles.css` and `components.json`: Nova theme,
  fonts, colors, radii, scrollbars, focus states, and shadcn/Base UI policy;
- `apps/desktop/src/renderer/src/components/ui`: ordinary control contracts;
- `apps/desktop/src/renderer/src/components/domain-ui`: reviewed product-owned
  exceptions;
- `apps/desktop/src/renderer/src/pdf-tools`: 22 annotation tool modes and their
  presentation/hit-test contracts;
- `apps/desktop/src/renderer/src/services/documentSession.ts` and
  `renderCoordinator.ts`: rendering, cancellation, caching, and stale-result
  behavior;
- `packages/pdf/src/browser.ts`: PDF.js browser rendering to canvas and
  `ImageBitmap`;
- `apps/desktop/src/preload/index.ts`, `apps/desktop/src/main/window.ts`, and
  `apps/desktop/src/shared/protocol.ts`: the Electron privilege and platform
  boundary.

The existing pure-GPUI `experiments/gpui-migration/gpui-gallery` is useful
evidence and should not be confused with the generic 0.5.2 shell. It already:

- translates reviewed Nova shell tokens in `src/nova_theme.rs`;
- opens files and renders a real page bitmap through the temporary Poppler
  backend in `src/pdf_document.rs`;
- uses `uniform_list` for virtualized thumbnails;
- registers native menus, key bindings, and actions;
- implements zoom, fit width, fit page, page navigation, tabs, rails, and a
  representative shell in `src/main.rs`.

It was set aside when the experiment changed to a component-library-first
direction. It lacks a reusable semantic primitive architecture and a complete
accessibility implementation, so it is not production-ready. It is still the
best local proof that a direct visual migration is viable and can be refactored
into the first parity slice instead of discarded.

## Known visual differences that need explicit handling

Most Nova visuals map directly to GPUI styles. These cases need custom work:

- Chromium and GPUI use different text shaping/rasterization paths. Match the
  exact font files, sizes, weights, line heights, and scale factor, then use
  bounded image differences rather than requiring identical glyph pixels.
- CSS color functions and variables must become resolved Rust colors. Preserve
  the source OKLCH values in a documented token generator or store reviewed
  resolved colors for light and dark themes.
- The current page raster contrast/saturation filter is not an `img()` option.
  Apply it in the page raster pipeline or a qualified custom shader.
- The closable-tab CSS gradient mask needs a custom clipping/fade treatment.
- DOM portals and Base UI dismissal/focus behavior do not exist automatically.
  GPUI overlays need explicit paint order, edge containment, focus return, and
  shortcut compatibility.
- CSS reduced-motion queries do not transfer. Expose a platform setting and
  make Butter Paper motion conditional.
- Native title bars and menu bars differ by operating system. Compare the
  product-owned content region separately from unavoidable native chrome.

## Migration plan

### Gate 0: freeze the current contract

Capture the existing Electron app at identical logical sizes and scale factors
on macOS, Windows, and Linux. Include light/dark, empty/document-loaded,
normal/hover/focus/pressed/selected/disabled, long titles, constrained windows,
menus, popovers, dialogs, both page modes, and both scroll axes. Record exact
bounds, fonts, icons, colors, keyboard behavior, focus order, and accessibility
semantics. Do not redesign during this gate.

### Gate 1: Butter Paper primitives

Create an owned theme and these first controls: text styles, separator, icon,
label, button, icon button, toggle, joined button group, tooltip, focus ring,
and scroll area. Reproduce the existing Nova samples side by side and with
overlays. Do not introduce `gpui-component` or `gpui-base` in this gate.

### Gate 2: compound controls

Port the exact ViewerToolbar, rails, closable tabs, selects, dropdown/context
menus, popovers, sidebar resize handle, transient panels, and dialogs. Each
component must pass pointer, keyboard, constrained-window, focus-return, and
AccessKit checks before shell assembly.

### Gate 3: existing shell, not a new concept

Assemble the current Butter Paper layout at the same dimensions with the same
action order, spacing, density, surfaces, and icons. Compare matched captures
region by region. The shell must collapse or contain content at every window
size covered by the Electron tests.

### Gate 4: one real read-only document slice

Open a PDF, show its real page, virtualize real thumbnails, navigate pages, and
implement zoom, fit width, fit page, continuous and single-page layout. Keep the
page renderer behind a trait so Poppler remains disposable. Measure first page,
scroll, zoom, cache, memory, draw, and present behavior separately.

### Gate 5: one complete annotation slice

Port rectangle creation and selection end to end: pointer capture, coordinate
mapping, snapping, handles, keyboard movement, undo/redo, save, reopen, and
accessible semantics. This is the minimum proof that the canvas and document
model can migrate without integrity loss.

### Gate 6: domain and platform completion

Port remaining annotation tools, text editing, images/signatures, templates,
printing/export, secure storage, native menus, window state, stable/beta
identity, updater verification, installers, signing, and N-1 update tests.
Electron remains the product and rollback path until these gates pass on all
supported platforms.

## Testing implications

- Port pure calculation and state-machine tests directly to Rust.
- Use `#[gpui::test]` for actions, focus, key dispatch, pointer input, scroll,
  resize, prompt, and clipboard behavior.
- Add stable debug selectors and AccessKit IDs to the owned components.
- Replace Playwright UI driving with GPUI tests plus platform accessibility
  automation. Do not use coordinate input when a semantic action is available.
- Use direct Metal texture capture on macOS for deterministic component images.
  This pin has no equivalent Linux/Windows headless renderer, so qualify those
  platforms through real GPU windows and native capture.
- Keep screenshots, event logs, and PDF fixtures disposable or ignored unless
  the repository intentionally adopts reviewed baselines.
- Compare visual, interaction, accessibility, document-integrity,
  performance, and packaging gates separately. Passing one is not evidence for
  another.

## Recommendation

Resume the direct `gpui-gallery` route, but refactor it into an owned
`butter_ui` component hierarchy before adding more features. Use the current
Electron/Nova app as the visual and behavioral specification. Do not select a
new shell direction. Do not import the styled `gpui-component` appearance.

Implement the initial component hierarchy on raw GPUI. Treat selected
`gpui-base` modules as a later fallback, not part of the planned architecture.
Any such dependency needs a separate decision backed by a failing direct-GPUI
test and must reproduce Butter Paper's interaction and accessibility contract
without affecting its visuals.

The immediate next milestone should show the existing Electron shell and the
GPUI shell at the same size, font, scale, theme, content, and state. It should
include pixel overlays plus keyboard and accessibility results. That milestone
will answer the visual question that the previous prototypes did not test.
