# Butter Paper repository instructions

## Current architecture

- Butter Paper is a pnpm monorepo with an Electron/React desktop app, a CLI, and shared `core` and `pdf` packages.
- The normal renderer uses PDF.js. The native Rust/PDFium render core under `native/pdfium-render-core` is an opt-in backend selected with `BP_DESKTOP_RENDER_BACKEND=pdfium`.
- Electron Forge is used only for the development server. Electron Builder owns packaging and release configuration.
- The renderer uses official shadcn/ui components with Base UI primitives and the Nova style. `apps/desktop/components.json` is the configuration source of truth.

## UI conventions

- Use components from `apps/desktop/src/renderer/src/components/ui` for standard controls. Add or refresh them with the official shadcn CLI; do not copy registry source by hand.
- Keep `style: "base-nova"`, Base UI (`base`), Lucide, Geist Variable, and the existing Butter Paper domain tokens unless a task explicitly changes them.
- Compose Base UI with `render`, never Radix `asChild`. Do not add Radix dependencies, APIs, CSS variables, or state selectors.
- Keep reviewed exceptions to standard shadcn composition in `apps/desktop/src/renderer/src/components/domain-ui`. Every file there must be explicitly allowlisted by `scripts/check-repository-hygiene.mjs` and explain why an official component cannot provide the behaviour. Default to generated Nova styling; any custom visual treatment must be individually allowlisted with a product or accessibility reason and scoped tests.
- Preserve Butter Paper's custom AEC tool icons and Fit Width/Fit Page/Continuous icons. Their explicit `size-*` classes prevent shadcn descendant SVG defaults from replacing their geometry.
- The PDF/canvas renderers, annotation layers, resize handles, virtualized thumbnails, and two-axis `CustomScrollArea` are domain UI rather than generic controls. Preserve their behavior and regression coverage unless a task explicitly redesigns them.
- Portaled menus, popovers, tooltips, selects, and dialogs must remain keyboard accessible, contained at constrained window sizes, and compatible with the application shortcut handler.

## Sources of truth

- Keep durable repository conventions in this file.
- Keep changing work state in GitHub issues and pull requests, not tracked plans, handoffs, memory files, worklogs, or agent transcripts.
- Keep disposable output under ignored directories such as `test-results/`, `playwright-report/`, package `dist/` folders, `.vite/`, `release/`, and native `target/` folders.
- Do not add machine-specific absolute paths to tracked files.

## Required workflow

1. Inspect `git status` before editing and preserve unrelated user changes.
2. Make the smallest change that satisfies the task; do not revive archived experiments or add speculative infrastructure.
3. Add or update deterministic tests for behavior changes.
4. Run the narrowest relevant checks while iterating, then `pnpm check` before handoff.
5. Review the final diff for generated files, stale references, secrets, and unrelated changes.

Do not run Playwright Electron E2E, the packaged desktop GUI smoke test, or the
macOS updater harness on a local macOS desktop. Their launch boundaries reject
local macOS execution. Run GUI coverage only on disposable GitHub Actions
runners through manual CI and focused release verification.

## Commands

- Install: `pnpm install --frozen-lockfile`
- Repository hygiene: `pnpm check:repo`
- Typecheck: `pnpm typecheck`
- Build: `pnpm build`
- Deterministic tests: `pnpm test`
- Required deterministic gate: `pnpm check`
- Targeted Electron E2E: manual GitHub Actions workflow only
- Full Electron E2E: manual GitHub Actions workflow only
- Desktop development: `pnpm dev:desktop`
- Desktop package smoke: release or manual GitHub Actions workflow only

Do not update Playwright snapshots unless the task intentionally changes reviewed UI output.

## Review guidelines

- Treat lost PDF content, corrupt saves, annotation round-trip failures, renderer crashes, preload/IPC privilege expansion, and platform-specific packaging failures as high priority.
- Keep Electron context isolation intact. Do not expose filesystem or process access directly to the renderer.
- Preserve import/export compatibility when changing markup models or appearance data.
- Verify platform assumptions against macOS, Windows, and Linux behavior represented in CI.
- Stable and beta are explicitly maintained as separate products with distinct application IDs, package names, user-data directories, updater caches, and feeds.
- macOS updates require Developer ID signing plus native N-1 verification. Windows NSIS and Linux AppImage updates additionally require the embedded reviewed TUF root, successful deterministic rejection tests, and native N-1 replacement on matching ARM64/x64 hosts. DEB and RPM upgrades remain package-manager controlled.
- The TUF root private key stays offline. `update-signing` holds only distinct targets, snapshot, and timestamp private keys and must permit the `v*` release tag policy plus `main` for scheduled metadata refresh. Never log, copy into artifacts, or commit any private key.
- Windows and Linux 0.0.11 packages have no updater bootstrap. Release notes for the first TUF-enabled release must state that a one-time manual install is required; do not pretend synthetic N-1 coverage changes that public compatibility fact.
- Production updater repositories use HTTPS. Loopback HTTP is allowed only with `BP_UPDATE_TEST_MODE=1`; direct non-macOS feed overrides must never bypass TUF.
- Update-feed publication occurs only inside the approved stable/beta release environment after the exact public release assets have been independently downloaded and verified. Preserve `.nojekyll`, never replace published assets, and publish a corrected higher version or restore the prior feed commit for rollback.
- Release tags must resolve to commits reachable from the repository's `main` default branch.
- `.github/workflows/ci.yml` is manually dispatchable and reusable by the tag-only release workflow; routine pushes and pull requests do not start GitHub-hosted CI.
- `MACOS_UPDATER_BOOTSTRAP_TAG` is a one-time exact tag in the channel's updater-verification environment. Use it only when that channel has no prior public package; remove it after the bootstrap release and never advance it to bypass N-1 tests.
- Do not stage, commit, push, open pull requests, alter remote settings, or create issues unless the user explicitly requests it.
