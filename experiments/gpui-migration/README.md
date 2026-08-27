# Butter Paper native GPUI rebuild

This experiment is a functional rebuild of Butter Paper in Rust and GPUI. It
is not a line-by-line Electron port and does not target Nova pixel parity.

The shipping Electron application remains the product reference for user
capabilities and document compatibility. The native application may use a
different layout, interaction detail, or implementation when it provides the
same useful outcome with a coherent desktop experience.

## Active direction

- Use the pinned Longbridge GPUI Component and Zed GPUI graph in
  `gpui-component-compat`.
- Use GPUI Component for ordinary application controls: buttons, menus,
  dialogs, tabs, inputs, lists, popovers, tooltips, progress, and resizable
  regions.
- Use product-owned GPUI rendering for PDF pages, annotation scenes, selection
  handles, hit testing, and other document-canvas behavior.
- Keep document, PDF, annotation, persistence, and session state outside UI
  components.
- Build complete user journeys in functional chunks. Test a chunk when its
  journey is usable; do not run the full acceptance gate after each small code
  change.

GitHub issue #82 is the active migration specification. The current functional
implementation graph is:

- #85: consolidated native reader and viewer;
- #86: multi-document workspace and command routing;
- #87: text and input-method editing;
- #88: shape and path annotation editing;
- #89: engineering measurement and snapping; and
- #90: media, pending redaction, and imported-annotation compatibility.

These issues contain changing work state and acceptance evidence. Issues #83
and #84 are completed foundation and template inputs. Later packaging,
distribution, and promotion work remains outside the current implementation
chunks.

## Keep and reuse

The existing experiment contains substantial reusable work:

- the checksum-bound Longbridge/Zed source preparation and license policy;
- the bounded build and storage guard;
- the PDFium worker, cancellation, stale-result rejection, mapped surfaces,
  page and thumbnail rendering, and resource release;
- the GPUI-free annotation model, geometry, persistence, generated-document,
  template, and PDF modules currently exposed by `gpui-gallery` without its UI
  features;
- native document sessions, safe Save and Save As, dirty-close protection,
  multi-document ownership, and template storage;
- real GPUI Component examples that are useful in the new application shell.

The current `gpui-component-compat` shell is a source of proven behavior, not a
required final architecture. Its large workspace module will be replaced by
capability modules rather than extended indefinitely.

## Historical material

The previous parity program, HTML reviews, prototypes, and chronological
ledgers are preserved under
`archive/parity-era-2026-08-27/`. They are historical reference only and must
not be used as the active migration plan.

`gpui-gallery` also contains the historical GPUI-CE application and its custom
`butter_ui` component layer. Do not extend that UI. Reuse only its GPUI-free
domain and PDF modules until those modules are extracted into capability
crates.

The versioned performance harness under `performance/` is preserved as
historical evidence. A later functional chunk will select the minimum reusable
runner needed for one matched Electron/GPUI journey. Old v4-v7 protocols are
not active product requirements.

## Functional chunks

1. **Consolidated native reader**
   Open multiple real PDFs, render pages and thumbnails, navigate, zoom, fit,
   rotate, scroll, switch documents, close safely, and recover from a failed
   open in one runnable GPUI Component application.
2. **Core annotation editor**
   Select and edit Rectangle/Ellipse, Line/Arrow, Pen/Highlight, Text Box, and
   Image annotations. Include properties, undo/redo, Save, close, and reopen.
3. **Engineering workflows**
   Add paths, clouds/callouts, measurement and calibration, useful snapping,
   Snapshot, and visibly pending redaction as coherent workflows.
4. **Documents and reusable content**
   Complete templates, imported-annotation preservation, multi-document dirty
   state, clipboard commands, recovery, and safe publication.
5. **Native alpha and decision**
   Build the same candidate for Linux, macOS, and Windows; run a matched
   correctness and performance journey; then decide whether to fund PDFium
   distribution, signatures, updater replacement, and production promotion.

Each chunk ends with a runnable journey, focused deterministic tests, one warm
full gate, and fresh native evidence on the applicable platform.

## Build boundaries

All work remains under `experiments/gpui-migration` until production promotion
is separately approved. Production Electron sources remain unchanged by this
experiment.

Rust builds on this VPS use the experiment build wrapper and
`host-storage-guard`. Ordinary test failures retain the owned Cargo target.
Safety failures may clean only the allowlisted disposable target.

Remote GPU testing is a chunk-level acceptance lane. Before leasing paid
compute, record the task budget, expected time-to-live (TTL), hard maximum TTL,
and independent cleanup. Copy evidence off the machine before verifying its
deletion. macOS and Windows functional testing uses the approved local devices
when the relevant chunk is ready.

## Evidence terms

- **Passed:** a current check completed successfully.
- **Failed:** a current check ran and did not pass.
- **Blocked:** a named external decision or unavailable capability prevents it.
- **Not run:** the check has not been attempted for the current candidate.
- **Development-only:** source or development runtime evidence.
- **Packaged:** evidence from the exact package candidate.
- **Physical-device:** evidence from a real supported device and session.

Compilation and deterministic tests do not imply visual, accessibility,
packaging, or physical-device acceptance.
