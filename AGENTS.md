# Butter Paper repository instructions

## Current architecture

- Butter Paper is a pnpm monorepo with an Electron/React desktop app, a CLI, and shared `core` and `pdf` packages.
- The normal renderer uses PDF.js. The native Rust/PDFium render core under `native/pdfium-render-core` is an opt-in backend selected with `BP_DESKTOP_RENDER_BACKEND=pdfium`.
- Electron Forge is used only for the development server. Electron Builder owns packaging and release configuration.
- The renderer currently uses Tailwind CSS plus project CSS and React components. Do not assume another component system is configured.

## Sources of truth

- Keep durable repository conventions in this file.
- Keep changing work state in GitHub issues and pull requests, not tracked plans, handoffs, memory files, worklogs, or agent transcripts.
- Keep disposable output under ignored directories such as `test-results/`, `playwright-report/`, package `dist/` folders, `.vite/`, `release/`, and native `target/` folders.
- Do not add machine-specific absolute paths to tracked files.

## Required workflow

1. Inspect `git status` before editing and preserve unrelated user changes.
2. Make the smallest change that satisfies the task; do not revive archived experiments or add speculative infrastructure.
3. Add or update deterministic tests for behavior changes.
4. Run the narrowest relevant checks while iterating, then the required repository checks before handoff.
5. Review the final diff for generated files, stale references, secrets, and unrelated changes.

## Commands

- Install: `pnpm install --frozen-lockfile`
- Repository hygiene: `pnpm check:repo`
- Typecheck: `pnpm typecheck`
- Build: `pnpm build`
- Deterministic tests: `pnpm test`
- Electron E2E: `pnpm test:e2e`
- Desktop development: `pnpm dev:desktop`
- Desktop package smoke: `pnpm --dir apps/desktop package` then `pnpm test:package:desktop`
- Full local gate: `pnpm check`

Do not update Playwright snapshots unless the task intentionally changes reviewed UI output.

## Review guidelines

- Treat lost PDF content, corrupt saves, annotation round-trip failures, renderer crashes, preload/IPC privilege expansion, and platform-specific packaging failures as high priority.
- Keep Electron context isolation intact. Do not expose filesystem or process access directly to the renderer.
- Preserve import/export compatibility when changing markup models or appearance data.
- Verify platform assumptions against macOS, Windows, and Linux behavior represented in CI.
- Do not stage, commit, push, open pull requests, alter remote settings, or create issues unless the user explicitly requests it.
