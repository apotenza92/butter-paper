# Clean rebuild (0.1.0)

Decision (2026-10-02, user): treat Butter Paper as if it had been started from
scratch in Rust and GPUI. Remove everything Electron-era or left over, move to a
clean layout, tooling and release system, and keep users reaching the new app
by auto-update. Supersedes the release work in
[release-pipeline.md](release-pipeline.md) (its findings still apply).

Choices (user, or recommendations the user deferred to):
- GPUI and gpui-component come from patched forks pinned by commit
  (`apotenza92/zed`, `apotenza92/gpui-kit`); no prepare scripts.
- Packaging, signing and release tooling is a Rust `cargo xtask`.
- The signature relay stays a TypeScript Cloudflare Worker, isolated in
  `services/signature-relay` with its own package.json.
- The never-published TypeScript CLI is dropped rather than ported: it had
  no users, and a console subcommand in the Windows GUI binary needs extra
  console handling for no benefit.
- One final Electron release redirects remaining Electron users to 0.1.0.

## Users and update paths

- New app (0.1.0 onward): built-in updater (stable and macOS Beta) and the
  Homebrew casks.
- Native 0.0.26 (no updater): one manual install, or `brew upgrade`. 0.1.0
  keeps the same settings and data locations, so it carries on from 0.0.26.
- Electron (any version): its feed (`updates` branch) currently leads to
  0.0.30, which migrates to native 0.0.26. Electron 0.0.31, built from
  `v0.0.30` with `nativeMigrationPlan.ts` pinned to 0.1.0's packages, migrates
  straight to 0.1.0.

Binary names and install layout are free to change only before the first
release with the updater ships; 0.0.32 was cancelled for that reason.

## Target layout

```
Cargo.toml                 workspace
crates/butter-paper/       the app (bin butter-paper, bin butter-paper-pdf-worker)
xtask/                     cargo xtask: bundle, sign, notarise, package, release
services/signature-relay/  Cloudflare Worker (TypeScript, own package.json)
assets/                    icons, fonts, licences
docs/planning/             plans (this file)
.github/workflows/         release.yml, pdfium build/approval
```

Product identities stay: `Butter Paper(.app)`, `Butter Paper Beta(.app)`,
`com.butterpaper.desktop(.beta)`, data and settings locations. Executables
become `butter-paper` and `butter-paper-pdf-worker` (Windows `.exe`); the Linux
launcher becomes `butter-paper`.

## Plan

1. [x] Forks: push `8b1497d` + the two GPUI patches and `c27f5d5` + the six
   gpui-component patches to `apotenza92/zed` and `apotenza92/gpui-component`;
   depend on them by `rev`.
2. [x] Move the crate to `crates/butter-paper`; rename binaries; root Cargo
   workspace. Startup keeps `StartupDataPolicy::NativeOnly` (no Electron
   data import, by the earlier owner decision); removing the unused import
   code is a separate cleanup.
3. [x] Delete `apps/cli`, `packages/core`, `packages/pdf` (CLI dropped).
4. [x] Delete leftovers: `experiments/` (performance harness, archive,
   prototypes, research, migration docs), `native/`, Playwright output,
   TypeScript root config; move the phone helper to its own home.
5. [x] `cargo xtask`: PDFium staging, macOS assemble/sign/notarise/zip,
   Windows and Linux packages, Homebrew bundle, release-check, version,
   repository and UI policy checks. Port the tests that guard them.
6. [x] Relay to `services/signature-relay`; repository root has no
   package.json.
7. [x] Updater and install layout use the new names; release workflow and
   PDFium workflows call xtask.
8. [x] AGENTS.md, README, CHANGELOG (0.1.0), planning docs rewritten for the
   clean app.
9. [ ] Gates: cargo test, cargo xtask check, xtask packaging dry runs.
10. [ ] Release 0.1.0; confirm updater feed, checksums, both casks.
11. [ ] Electron 0.0.31 redirect release from `v0.0.30`, pinned to 0.1.0;
    confirm the Electron feed and a migration on a clean machine.

## Decisions made while rebuilding

- PDFium: the approved build (run 36525856484, whose artifact would expire
  2026-10-29) is published once as the immutable `pdfium-7881` release and
  pinned by SHA-256 in `xtask/pdfium.json`; development uses it too. The
  PDFium build and approval workflows were removed; their patches are in
  `docs/pdfium/`. A PDFium upgrade needs a new build pipeline (follow-up).
- Packages no longer carry intermediate receipts; safety comes from pinned
  inputs, binary checks (architecture, minimum macOS), signing, notarisation
  and the updater's own checks, checksums and attestations.
- THIRD_PARTY_NOTICES.md is generated per target from the locked graph
  (`cargo xtask notices`), with standard texts for crates that ship none.
- Butter Paper Beta uses its own icon set.
- `release.yml` has an unsigned dry run (workflow_dispatch) for packaging
  changes.
- Fixed: the updater compared a beta's CFBundleShortVersionString with
  `X.Y.Z-beta.N` and would have rejected every macOS beta update.

## Status

- 2026-10-02: 0.0.32 release run cancelled before publishing (no release
  exists); plan agreed.
