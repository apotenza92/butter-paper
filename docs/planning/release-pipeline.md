# Release pipeline

Decision (2026-10-02, user): plan and complete the whole pipeline before the
next release (0.0.32). Goals: safety, speed, convenience, automation.

## Goals

- Safety: packages come from a tagged commit on `main` and the human-approved
  PDFium build; macOS is Developer ID signed and notarised; every asset is
  checksummed; the Homebrew bundle is attested; releases are immutable once
  published; users never see a partial release.
- Speed: a release takes about 20 minutes, and packaging mistakes fail locally
  in about a minute rather than 10–30 minutes into CI.
- Convenience: `pnpm release:version X.Y.Z`, then `pnpm release`; nothing typed
  into GitHub, nothing uploaded by hand.
- Automation: the in-app updater (six targets, Butter Paper Beta on macOS)
  and both Homebrew casks update from the release.

## Constraints (verified 2026-10-02)

- Immutable releases are enabled: a release is published once and can never
  gain or replace assets. One version is one complete release.
- `release-signing` and `homebrew-dispatch` environments accept only `v*`
  tags.
- The tap (`apotenza92/homebrew-tap`) accepts an attested
  `homebrew-publication.tar.gz` from a tag-push run of the workflow its
  registry names (`.github/workflows/release.yml`), with all architectures
  for both channels; a stable release also advances the Beta cask. Registry
  `minimum_macos` must equal the bundle's (native app: 13.0; registry: 12.0).
- Beta tags are `vX.Y.Z-beta.N` (tap, other apps); the native updater only
  parses `vX.Y.Z`. Shipped binaries carry the parser, so 0.0.32 must accept
  beta tags.
- The PDFium approval artifact (run 36525856484) expires 2026-10-29.
- Build times (0.0.26): Linux ~10 min, Windows ~19, Apple silicon ~14, Intel
  macOS ~30 on Intel runners. No caching.

## Plan

1. [x] Updater parses `vX.Y.Z-beta.N` (stable sorts above its betas); beta
   builds report their beta version.
2. [x] One `release.yml`, triggered by pushing `vX.Y.Z` or `vX.Y.Z-beta.N`:
   - prepare: tag on `main`, version files match the tag, CHANGELOG section,
     build number `(M*1e6+m*1e3+p)*1e5 + (beta ? N : 90000)` (as Macsimize),
     PDFium approval from repository variables, target matrix (stable: six
     targets; beta: macOS only, Beta identity only).
   - package: Intel macOS cross-compiled on the Apple silicon runner; each
     macOS job packages Stable and Beta from one build; Cargo download cache.
   - aggregate receipts; publish: draft with every asset, `SHA256SUMS.txt`
     and the attested Homebrew bundle, verify, then publish once.
   - homebrew: dispatch the tap with the existing dispatcher app.
   Replaces `build-gpui-stable-candidate.yml`.
3. [x] Tap registry: `minimum_macos` 13.0.
4. [x] `pnpm release:version` bumps every version file and checksum;
   `pnpm release:check` (version consistency, changelog, PDFium approval
   expiry, actionlint, macOS input-preparation dry run, `pnpm check`) and
   `pnpm release` (check, then tag and push).
5. [x] AGENTS.md release section; repository variables for PDFium approval.
6. [ ] Release 0.0.32 through it; confirm the updater feed, checksums and both
   casks.

## Status

- 2026-10-02: tiered publishing was tried and withdrawn (incompatible with
  immutable releases and Homebrew); its unreleased `v0.0.32` tag was deleted
  before any release was created. Its validation runs passed every package
  build: macOS Apple silicon (Stable and Beta, 15.6 min), Windows x64 (Git
  Bash fix, 18 min) and arm64, Linux x64 and arm64 (10 min).
- 2026-10-02: tap registry commit `d37e41c` sets `minimum_macos` 13.0; the
  generated bundles pass the tap's own validator offline (stable and beta);
  `BP_PDFIUM_APPROVAL_RUN_ID`/`_ATTEMPT` set to 36525856484/1.
- Next: release 0.0.32 with `pnpm release` (step 6).
