# Butter Paper repository instructions

## Architecture and boundaries

- Butter Paper is a native Rust application using GPUI and GPUI Component. PDFium renders and edits PDFs in a separate PDF worker process.
- The Cargo workspace contains `crates/butter-paper` (app and PDF worker), the Apache-licensed `crates/ztracing-shim`, and `xtask` (`cargo xtask`). The signature relay in `services/signature-relay` is a separate TypeScript project with its own package manifest and lockfile.
- GPUI and GPUI Component use `apotenza92/zed` and `apotenza92/gpui-kit`, branch `butter-paper`. GPUI patches are pinned in the root `Cargo.toml`; GPUI Component and its assets are pinned together in `crates/butter-paper/Cargo.toml`. Permanent fork changes require a fork commit and updated pins. Keep Zed's GPL `ztracing` replaced by the Apache shim.
- PDFium is the approved chromium/7881 build, published as `pdfium-7881` and pinned by SHA-256 in `xtask/pdfium.json`. Fetch it with `cargo xtask pdfium`; build patches live in `docs/pdfium/`.
- Electron and migration experiments remain in git history; `electron-final` is reference-only. Do not revive them. Native startup imports no Electron data (`StartupDataPolicy::NativeOnly`).
- Generated application icons live in `assets/app`; their sources live in `assets/icon-source`.

## UI conventions

- Compose standard controls from stock gpui-component controls and use semantic theme tokens. For property controls, reuse `property_controls.rs` and read [the property-control guide](docs/agents/property-controls.md) before adding or changing a property family.
- Preserve custom AEC icons and the Fit Width/Fit Page/Continuous icons at their explicit sizes.
- Canvas renderers, annotation layers, resize handles, virtualized thumbnails and the two-axis scroll area are domain UI. Preserve their behaviour and regression coverage unless the task explicitly redesigns them.
- Popups, menus, selects and dialogs must remain keyboard accessible, contained at constrained window sizes and compatible with application shortcuts. An overlay press must never also start a canvas selection or edit (`overlay_state::press_owned_by_overlay`).
- Dismiss transient panels and expanded subflows with a top-right icon-only X. Reserve `Cancel` for a decision prompt beside a commit or destructive action. Hide destructive item controls until hover or keyboard focus, keeping them keyboard accessible and separate from the primary click target.
- Keep PDF paper-relative colours independent of the shell theme; annotation colours are document data.
- Before changing native UI or accepting its appearance, read [the UI policy](docs/agents/ui-policy.md) and [the UX review checklist](docs/planning/ux-review.md). Use the gpui-component and gpui-migration-ux-review skills. Build success is not visual acceptance.

## Sources of truth

- Keep core repository conventions here and detailed procedures in the linked `docs/agents/` guides.
- Start at [docs/planning/README.md](docs/planning/README.md). Update existing open work, decisions and behaviour contracts in place; remove finished backlog items. Do not create duplicate GitHub plans, chronological worklogs, evidence trails or agent transcripts.
- Read [decisions.md](docs/planning/decisions.md) for product decisions, [interactions.md](docs/planning/interactions.md) when changing canvas behaviour, and [pdf-format.md](docs/planning/pdf-format.md) when changing markup storage or appearance data. Surface conflicts rather than silently overriding these contracts.
- Adapt skill requests for issues or tickets to local Markdown using [the issue-tracker guide](docs/agents/issue-tracker.md).
- Keep disposable output under ignored `target/` directories; tests use `crates/butter-paper/target/test-scratch`. Never add machine-specific absolute paths to tracked files.

## Workflow and checks

1. Inspect `git status` before editing; preserve unrelated user changes.
2. Make the smallest change that satisfies the task. Avoid speculative infrastructure and archived experiments.
3. Add or update deterministic tests for behaviour changes. Run the narrowest relevant checks while iterating.
4. Apply the checks below before handoff, then review the final diff for generated files, stale references, secrets and unrelated changes.

| Change | Required checks |
| --- | --- |
| Rust code, Rust dependencies or build/packaging logic | `cargo test --workspace` and `cargo xtask check` (repository checks, rustfmt and clippy with warnings denied). |
| PDF rendering, editing, storage or worker behaviour | Also run the relevant real-PDFium tests: `BP_PDFIUM_LIBRARY=<approved-library> cargo test <test-filter> -- --ignored`. Report any unavailable platform or runtime verification. |
| Native UI | Also perform native interaction and visual review using [native-review.md](docs/agents/native-review.md) and the UX checklist. Inspect the actual final screenshots. |
| Signature relay | In `services/signature-relay`, run `pnpm test` and `pnpm typecheck`; install with `pnpm install --frozen-lockfile` if needed. |
| Documentation only | Check factual references, relative links and `git diff --check`; compilation and GUI review are unnecessary. |

- Build with `cargo build`. CI is defined in `.github/workflows/ci.yml`; formatting-only commits are listed in `.git-blame-ignore-revs`.
- Treat lost PDF content, corrupt saves, annotation round-trip failures, renderer/worker crashes, privilege expansion and platform packaging failures as high priority. Preserve import/export compatibility and verify affected platform assumptions.
- Before launching any development or review build, read [native-review.md](docs/agents/native-review.md). Use an isolated profile and unique review bundle; preserve unsaved work and verify only the intended testing version is running.
- Before packaging or releasing, read [releases.md](docs/agents/releases.md). Releases are immutable; never expose signing keys or certificates.

## Authorization

- Continue local edits, tests and disposable review within the requested scope without asking again for already authorized work.
- Do not stage, commit, push, open pull requests, alter remote settings or create issues unless the user explicitly requests it. Release publication also requires an explicit release request.
- For a pinned dependency fork, prepare and test a focused patch first. If its commit/push has not been authorized, present that concrete patch and request approval for the fork action before changing the permanent pin. Do not substitute a tracked local path override or modify Cargo's cached checkout.
