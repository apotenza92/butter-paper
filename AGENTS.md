# Butter Paper repository instructions

## Current architecture

- Butter Paper is a native desktop application written in Rust with GPUI and gpui-component, in `experiments/gpui-migration/gpui-migration` (the crate keeps its migration-era path). It renders and edits PDFs through PDFium in a separate PDF worker process.
- The pnpm workspace also holds a CLI (`apps/cli`) built on the shared TypeScript `packages/core` and `packages/pdf`, and the Cloudflare signature relay (`apps/signature-relay`) used by phone signing.
- The Electron application was removed after 0.0.30. Its source remains in `main`'s history before the removal, and the final Electron reference working tree is tagged `electron-final`; do not revive it.
- GPUI and gpui-component are vendored as reviewed, checksum-pinned preparations under the crate's `.prepared/` directory. Change them only through the patches and `source-preparation-policy.json`, and verify with `node scripts/prepare.mjs verify` and `node scripts/prepare-zed.mjs verify` from the crate.
- Application icons live in `assets/app` (generated from `assets/` by `pnpm generate:icons`).

## UI conventions

- Compose standard controls from stock gpui-component controls. The property toolkit in the crate's `src/property_controls.rs` provides the panel, header, numeric and slider/input layouts for property families.
- Preserve Butter Paper's custom AEC tool icons and the Fit Width/Fit Page/Continuous icons at their explicit sizes.
- The PDF/canvas renderers, annotation layers, resize handles, virtualized thumbnails and the two-axis scroll area are domain UI rather than generic controls. Preserve their behaviour and regression coverage unless a task explicitly redesigns them.
- Popups, menus, selects and dialogs must remain keyboard accessible, contained at constrained window sizes and compatible with the application shortcut handler. A press that opens, acts inside or dismisses a popup or dialog must never also start a canvas selection or edit (`overlay_state::press_owned_by_overlay`).
- Use a top-right icon-only X to dismiss a transient panel or expanded subflow. Reserve the word `Cancel` for a modal decision beside a commit or destructive action. Keep destructive item controls hidden until hover or keyboard focus, but always keyboard accessible and separate from the item's primary click target.

## UI lint policy

- `pnpm check:ui` runs the GPUI source-policy gate and is included in `pnpm check`. It checks the stock-tab/close/menu contracts, token usage in inspector/panel/viewer-toolbar/system-theme modules, and embedded measurement scroll ownership. Its colour rule catches direct numeric `rgb`/`rgba`/`hsla` constructor calls; it is a bounded source check, not a Rust parser or complete style analysis. It does not inspect document raster colours, infer semantic roles, or certify rendered geometry/gestures. Native compiled interaction tests and visual review remain required. Extend this gate with regression tests for concrete failures; do not blanket-disable rules or broaden allowances to make a failure pass.
- PDF canvas roles keep paper-relative colours independent of the shell theme. Dynamic annotation colours are document data.

## Sources of truth

- Keep durable repository conventions in this file.
- Keep plans, region briefs, decisions and current work state in local Markdown under `docs/planning/`. Start at `docs/planning/README.md`; update existing files in place. Do not create duplicate GitHub plans, chronological worklogs or agent transcripts.
- Keep disposable output under ignored directories such as `test-results/`, `playwright-report/`, package `dist/` folders, `.vite/`, `release/`, and native `target/` folders.
- Do not add machine-specific absolute paths to tracked files.

## Agent skills

### GPUI migration UX review

For native UI changes and acceptance reviews, use the gpui-migration-ux-review skill alongside gpui-component. Always read docs/planning/ux-review.md and the relevant region brief, including if skill discovery is unavailable. Perform its contradiction-seeking review on the actual final screenshots before handoff. Maintain reusable lessons in that checklist and current defects in the region brief; do not duplicate plans or treat build success as visual acceptance.

GPUI properties use the application toolkit in `experiments/gpui-migration/gpui-migration/src/property_controls.rs`, composed from stock GPUI Component controls. Reuse its panel, header, stepper-free numeric and slider/input layouts for new property families. Keep applicability/ranges in `tool_properties.rs`, defaults in the session adapter and selected-object mutations in identity-checked workspace/domain paths. New controls require a real rendering/persistence path and per-family tests; adding a field or hiding a missing capability does not complete a migration.

### Local planning

Read `docs/planning/README.md` and update the relevant local plan. Skill requests
to publish issues or tickets are adapted to local Markdown; see
`docs/agents/issue-tracker.md`. Use the local states in
`docs/agents/triage-labels.md`.

### Domain docs

This monorepo uses a multi-context domain-document layout. See
`docs/agents/domain.md`.

## Required workflow

1. Inspect `git status` before editing and preserve unrelated user changes.
2. Make the smallest change that satisfies the task; do not revive archived experiments or add speculative infrastructure.
3. Add or update deterministic tests for behavior changes.
4. Run the narrowest relevant checks while iterating, then `pnpm check` and the native crate's `cargo test` before handoff.
5. Review the final diff for generated files, stale references, secrets, and unrelated changes.

Live GUI review of the native app uses an isolated, disposable data root (`BP_NATIVE_DEVELOPMENT=1 BP_GPUI_DATA_DIR=...`) and a uniquely named review bundle; it never modifies an installed application or real profile.

## Commands

- Install: `pnpm install --frozen-lockfile`
- Repository hygiene: `pnpm check:repo`
- Typecheck: `pnpm typecheck`
- Build: `pnpm build`
- Deterministic tests: `pnpm test`
- Required deterministic gate: `pnpm check`
- Native app (from `experiments/gpui-migration/gpui-migration`): `cargo build`, `cargo test`; see that crate's README for the foundation gates and macOS window harness.

## Review guidelines

- Treat lost PDF content, corrupt saves, annotation round-trip failures, renderer or PDF worker crashes, privilege expansion, and platform-specific packaging failures as high priority.
- Preserve import/export compatibility when changing markup models or appearance data.
- Verify platform assumptions against macOS, Windows, and Linux behavior.

## Releases

Keep the release process lean. `pnpm check` and the native tests run locally are the release gate; do not add GitHub-hosted smoke, audit, or rehearsal jobs beyond the native candidate workflow.

- Native releases are published to GitHub Releases with a `SHA256SUMS.txt` by `.github/workflows/build-gpui-stable-candidate.yml`, dispatched from the `v<version>` tag (the signing environment accepts only `v*` tags). Each run builds one tier from `.github/release-targets.json`: run `primary` (Apple silicon macOS, Windows x64, Linux x64) first, then `secondary` (Intel macOS, Windows arm64, Linux arm64); each tier adds its packages and checksums to the same release, which the first tier creates from the version's `CHANGELOG.md` section. macOS jobs package Butter Paper and Butter Paper Beta from one build. macOS packages are Developer ID signed and notarised; Windows and Linux packages are unsigned.
- Release tags must resolve to commits reachable from `main`.
- Never log, copy into artifacts, or commit any signing key or certificate.
- Do not stage, commit, push, open pull requests, alter remote settings, or create issues unless the user explicitly requests it.
