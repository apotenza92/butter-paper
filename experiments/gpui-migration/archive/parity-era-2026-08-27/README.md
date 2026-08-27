# Parity-era archive

This directory preserves the Electron-parity migration material that was
superseded on 2026-08-27.

The archived program tried to freeze individual Electron interactions and
reproduce them through small GPUI compatibility probes. It produced useful
foundation, PDF, annotation, persistence, and session work, but its active
planning and evidence layers became larger than the product feedback they
provided.

Contents:

- `docs/`: chronological audits, parity ledgers, and the former experiment
  README;
- `review-pages/`: static HTML comparison pages and interaction prototypes;
- `prototypes/`: superseded alternative GPUI application experiments.

These files are historical reference. The active direction is documented in
`../../README.md` and GitHub issue #82. Do not append current progress to this
archive.

Archived pages preserve their original text and relative links. Links from the
moved HTML and Markdown files to captures, performance output, or active source
may no longer resolve from the archive directory. Treat them as historical
citations. Use `../../captures/`, `../../performance/`, and the active paths
named in `../../README.md` when inspecting retained material.

The Rust backend and current GPUI Component candidate were not moved here.
They contain reusable implementation that will be consolidated by capability.
The older GPUI-CE UI remains in `gpui-gallery` temporarily because its
GPUI-free domain modules are still dependencies of the active candidate.
