# UI source policy

Read this before changing native controls, theme styling or the UI source gate.
The core conventions are in [AGENTS.md](../../AGENTS.md); native verification
follows [native-review.md](native-review.md) and
[the UX checklist](../planning/ux-review.md).

`crates/butter-paper/tests/ui_source_policy.rs` runs with `cargo test`. It checks
stock-tab/close/menu contracts, token usage in inspector/panel/viewer-toolbar/
system-theme modules, and embedded measurement scroll ownership.

The colour rule catches direct numeric `rgb`/`rgba`/`hsla` constructors. This is
a bounded source check, not a Rust parser or complete style analysis. It does
not inspect document raster colours, infer semantic roles or certify rendered
geometry and gestures. PDF canvas roles keep paper-relative colours independent
of the shell theme; dynamic annotation colours are document data.

Extend the gate with regressions for concrete failures. When the user approves
a changed UI contract, update its precise assertion and verify the new behaviour.
Do not blanket-disable rules or broaden unrelated allowances merely to make a
failure pass. Native compiled interaction tests and visual review remain
required for UI changes.
