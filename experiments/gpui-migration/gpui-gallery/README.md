# Historical gallery and active backend modules

This crate contains two different kinds of code. Keep the distinction clear.

## Active reusable code

`gpui-component-compat` depends on this crate with default features disabled.
That path reuses GPUI-free PDF, document, annotation, geometry, persistence,
template, image, and verification modules. Keep those modules working until
they are extracted into capability crates owned by the native application.

The active candidate must continue to resolve one pinned Zed GPUI identity.
The optional GPUI-CE dependencies in this crate are not part of that candidate
graph.

## Historical code

The `gallery` feature, gallery binary, `src/main.rs`, and `src/butter_ui/`
custom component layer belong to the superseded GPUI-CE experiment. Do not add
product UI or new migration behavior to those surfaces. They remain intact
only so historical evidence can be inspected and the reusable backend can be
separated safely.

The active application direction and functional ticket graph are documented
in [`../README.md`](../README.md). Historical parity material is under
[`../archive/parity-era-2026-08-27/`](../archive/parity-era-2026-08-27/).
GitHub issue #82 owns the specification; issues #85 through #90 own the current
implementation chunks.
