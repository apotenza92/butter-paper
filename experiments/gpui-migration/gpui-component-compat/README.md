# Native GPUI Component candidate

This crate is the current native Butter Paper candidate and the reviewed
dependency foundation for the functional rebuild. It is development-only and
does not modify or replace the production Electron application.

## Foundation

- Longbridge GPUI Component:
  `c27f5d5c8f70d534978c2f0739ad9e10d4e41eb4`
- Zed GPUI:
  `8b1497dbd22fb06f5838a7c0b84a1e54fafa71bc`
- Rust: `1.97.1`
- Prepared source digest:
  `630e9459587c2fef7a791467de6e61351f067ec71ccba95f2ac00c95e967c7a3`

`source-preparation-policy.json`, `THIRD_PARTY_NOTICES.md`, and the preparation
scripts pin source identity, patches, checksums, allowed dependencies, and
license provenance. The PDFium development manifest remains
`productionApproved: false`.

## Product direction

Use GPUI Component for ordinary controls and GPUI for the document canvas.
Application state owns documents, annotations, viewer state, persistence, and
commands. UI controls render snapshots and dispatch intent.

The existing implementation proves many required behaviors, but its
`DocumentWorkspace` has grown into an application-sized module. Treat it as a
source of working behavior while the active rebuild extracts these capability
modules:

- document session and resource ownership;
- PDF viewer and render scheduling;
- annotation editor and scene generation;
- safe persistence and recovery;
- document tabs and application commands;
- GPUI Component application shell.

Do not add another micro-parity behavior to the monolithic workspace. New work
must complete one of the functional chunks in `../README.md` and leave a
runnable user journey. GitHub issue #82 owns the specification; issues #85
through #90 own the current implementation chunks.

## Reusable implementation

Keep and consolidate:

- PDFium worker lifecycle, mapped surfaces, cancellation, stale-result
  rejection, page and thumbnail rendering, and bounded caches;
- safe Save and Save As publication and source reconciliation;
- multi-document sessions and dirty-close protection;
- annotation editing, history, geometry, and PDF round trips;
- template generation, import, storage, and document creation;
- current real GPUI Component controls and source-preparation policy.

`../gpui-gallery` is consumed with default features disabled. This exposes its
GPUI-free model and PDF modules without adding its GPUI-CE identity to the
candidate. Its old gallery binary and custom `butter_ui` controls are
historical and must not be extended.

## Verification

Run non-build gates first:

```sh
node --test tests/build-guard.test.mjs tests/source-preparation.test.mjs tests/foundation-truth.test.mjs
node scripts/foundation-truth.mjs
node scripts/prepare.mjs verify
node scripts/verify-cargo-graph.mjs
cargo deny --config deny.toml --exclude-dev --locked check \
  --warn unmaintained advisories licenses sources
```

Run Rust compilation and tests through both storage guards:

```sh
host-storage-guard check
host-storage-guard run -- bash scripts/run-bounded-button-probe.sh <focused-mode>
host-storage-guard run -- bash scripts/run-bounded-button-probe.sh all-targets
```

The wrapper uses the allowlisted target
`../.build-targets/gpui-component-compat`, one Cargo job, locked dependencies,
a 30 GiB preflight floor, a 20 GiB runtime stop floor, and a 4 GiB target cap.
Ordinary compile or test failure retains a valid target. Safety failures may
clean only the owned target. Use focused modes while iterating and
`all-targets` once at functional-chunk acceptance.

The current mode registry is intentionally injection-safe but overgrown. Its
replacement should allow a small reviewed test-target/filter interface instead
of adding one permanent mode for every test.

## Evidence boundary

The current source includes strong Linux development tests for PDF rendering,
annotations, persistence, templates, document sessions, and GPUI Component
composition. This does not establish current native visual or accessibility
acceptance, production PDFium redistribution, packaged macOS/Windows/Linux
behavior, updater replacement, or production promotion.

Current progress and acceptance evidence belong in GitHub issues. The former
2,000-line chronological README is preserved in
`../archive/parity-era-2026-08-27/docs/GPUI-COMPONENT-COMPAT-README.md`.
