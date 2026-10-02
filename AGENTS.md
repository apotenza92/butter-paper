# Butter Paper repository instructions

## Current architecture

- Butter Paper is a native desktop application written in Rust with GPUI and GPUI Component. It renders and edits PDFs through PDFium in a separate PDF worker process.
- The repository is a Cargo workspace: the app in `crates/butter-paper` (binaries `butter-paper` and `butter-paper-pdf-worker`), the Apache-licensed `ztracing` shim in `crates/ztracing-shim`, and build, packaging and release tasks in `xtask` (`cargo xtask`). The Cloudflare signature relay used by phone signing is a standalone TypeScript project in `services/signature-relay` with its own `package.json` and lockfile; it is the only JavaScript in the repository.
- GPUI and GPUI Component come from Butter Paper's forks (`apotenza92/zed` and `apotenza92/gpui-kit`, branch `butter-paper`), pinned by commit in the workspace `Cargo.toml`. Change them by committing to the fork and moving the pin. Zed's GPL `ztracing` must stay replaced by the shim.
- PDFium is the human-approved chromium/7881 build, published once as the `pdfium-7881` release and pinned by SHA-256 in `xtask/pdfium.json`; `cargo xtask pdfium` fetches it. Its build patches are kept in `docs/pdfium/`.
- The Electron application was removed after 0.0.30, and the migration-era tooling and experiments with 0.1.0. They remain in git history (the Electron tree is tagged `electron-final`); do not revive them. By owner decision, native startup imports no Electron data (`StartupDataPolicy::NativeOnly`).
- Application icons live in `assets/app` (generated artefacts, committed; sources in `assets/icon-source`).

## UI conventions

- Compose standard controls from stock gpui-component controls. The property toolkit in the crate's `src/property_controls.rs` provides the panel, header, numeric and slider/input layouts for property families.
- Preserve Butter Paper's custom AEC tool icons and the Fit Width/Fit Page/Continuous icons at their explicit sizes.
- The PDF/canvas renderers, annotation layers, resize handles, virtualized thumbnails and the two-axis scroll area are domain UI rather than generic controls. Preserve their behaviour and regression coverage unless a task explicitly redesigns them.
- Popups, menus, selects and dialogs must remain keyboard accessible, contained at constrained window sizes and compatible with the application shortcut handler. A press that opens, acts inside or dismisses a popup or dialog must never also start a canvas selection or edit (`overlay_state::press_owned_by_overlay`).
- Use a top-right icon-only X to dismiss a transient panel or expanded subflow. Reserve the word `Cancel` for a modal decision beside a commit or destructive action. Keep destructive item controls hidden until hover or keyboard focus, but always keyboard accessible and separate from the item's primary click target.

## UI lint policy

- `crates/butter-paper/tests/ui_source_policy.rs` is the GPUI source-policy gate and runs with `cargo test`. It checks the stock-tab/close/menu contracts, token usage in inspector/panel/viewer-toolbar/system-theme modules, and embedded measurement scroll ownership. Its colour rule catches direct numeric `rgb`/`rgba`/`hsla` constructor calls; it is a bounded source check, not a Rust parser or complete style analysis. It does not inspect document raster colours, infer semantic roles, or certify rendered geometry/gestures. Native compiled interaction tests and visual review remain required. Extend this gate with regression tests for concrete failures; do not blanket-disable rules or broaden allowances to make a failure pass.
- PDF canvas roles keep paper-relative colours independent of the shell theme. Dynamic annotation colours are document data.

## Sources of truth

- Keep durable repository conventions in this file.
- Keep plans, region briefs, decisions and current work state in local Markdown under `docs/planning/`. Start at `docs/planning/README.md`; update existing files in place. Do not create duplicate GitHub plans, chronological worklogs or agent transcripts.
- Keep disposable output under ignored `target/` directories (tests write scratch files to `crates/butter-paper/target/test-scratch`).
- Do not add machine-specific absolute paths to tracked files.

## Agent skills

### GPUI migration UX review

For native UI changes and acceptance reviews, use the gpui-migration-ux-review skill alongside gpui-component. Always read docs/planning/ux-review.md and the relevant region brief, including if skill discovery is unavailable. Perform its contradiction-seeking review on the actual final screenshots before handoff. Maintain reusable lessons in that checklist and current defects in the region brief; do not duplicate plans or treat build success as visual acceptance.

GPUI properties use the application toolkit in `crates/butter-paper/src/property_controls.rs`, composed from stock GPUI Component controls. Reuse its panel, header, stepper-free numeric and slider/input layouts for new property families. Keep applicability/ranges in `tool_properties.rs`, defaults in the session adapter and selected-object mutations in identity-checked workspace/domain paths. New controls require a real rendering/persistence path and per-family tests; adding a field or hiding a missing capability does not complete a migration.

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
4. Run the narrowest relevant checks while iterating, then `cargo test --workspace` and `cargo xtask check` before handoff (and the relay's `pnpm test` if you changed it).
5. Review the final diff for generated files, stale references, secrets, and unrelated changes.

Live GUI review of the native app uses an isolated, disposable data root (`BP_NATIVE_DEVELOPMENT=1 BP_GPUI_DATA_DIR=...`) and a uniquely named review bundle; it never modifies an installed application or real profile.

## Commands

- Build and test: `cargo build`, `cargo test --workspace`
- Repository checks (paths, versions, workflow pins): `cargo xtask check`
- Run the app: `cargo xtask pdfium` prints the approved libpdfium path; then `BP_NATIVE_DEVELOPMENT=1 BP_PDFIUM_LIBRARY=<path> cargo run`. Real-PDFium tests: `BP_PDFIUM_LIBRARY=<path> cargo test -- --ignored`.
- Package one target: `cargo xtask package --target <triple> [--channel stable,beta] --out <dir>` (macOS signing with `--sign`, as the release workflow does).
- Third-party notices: `cargo xtask notices --target <triple> --out <file>`.
- Signature relay (`services/signature-relay`): `pnpm install --frozen-lockfile`, `pnpm test`, `pnpm typecheck`.

## Review guidelines

- Treat lost PDF content, corrupt saves, annotation round-trip failures, renderer or PDF worker crashes, privilege expansion, and platform-specific packaging failures as high priority.
- Preserve import/export compatibility when changing markup models or appearance data.
- Verify platform assumptions against macOS, Windows, and Linux behavior.

## Releases

Keep the release process lean. `cargo xtask release-check` (which runs the repository checks and `cargo test --workspace` locally) is the release gate; do not add GitHub-hosted smoke, audit, or rehearsal jobs beyond the release workflow and its dry run.

- Release with `cargo xtask version X.Y.Z` (or `X.Y.Z-beta.N`), edit the new `CHANGELOG.md` section, commit and push to `main`, then `cargo xtask release`. That runs `release-check` (repository state, versions, changelog, repository checks, actionlint and cargo-deny when installed, the Homebrew bundle, a macOS packaging dry run, tests) and pushes the tag. `.github/workflows/release.yml` then packages every target in `.github/release-targets.json` with `cargo xtask package` (a beta tag builds Butter Paper Beta for macOS only), publishes one complete release with `SHA256SUMS.txt` and attestations, and asks `apotenza92/homebrew-tap` to update `butter-paper` and `butter-paper@beta`. Its file name is in the tap's registry. Running the workflow by hand from `main` is an unsigned dry run that publishes nothing; use it after changing packaging. Releases are immutable: never edit one; ship a new version. macOS packages are Developer ID signed and notarised; Windows and Linux packages are unsigned.
- Release tags must resolve to commits reachable from `main`.
- Never log, copy into artifacts, or commit any signing key or certificate.
- Do not stage, commit, push, open pull requests, alter remote settings, or create issues unless the user explicitly requests it.
