# Packaging and releases

Read this before packaging, changing release automation or publishing a release.
Authorization is governed by [AGENTS.md](../../AGENTS.md); these commands do not
provide permission to commit, push or publish.

## Package and notices

- Package one target: `cargo xtask package --target <triple> [--channel stable,beta] --out <dir>`.
- macOS signing uses `--sign`, as in the release workflow.
- Third-party notices: `cargo xtask notices --target <triple> --out <file>`.
- User-facing package names stay Butter Paper or Butter Paper Beta, without
  version or architecture suffixes.

## Publish an authorized release

Test while working: `cargo test --workspace` and `cargo xtask check`. Releasing
runs no tests; do not add a repeated test gate to the release command.

1. Run `cargo xtask version X.Y.Z` (or `X.Y.Z-beta.N`).
2. Edit the new `CHANGELOG.md` section.
3. Commit and push to `main` within the authorized release scope.
4. Run `cargo xtask release`.

The release command checks that `main` is pushed and clean, versions agree, the
changelog contains the section and the tag is new, then pushes the tag. Release
tags must resolve to commits reachable from `main`.

`.github/workflows/release.yml` packages every target in
`.github/release-targets.json` with `cargo xtask package`. A beta tag builds
Butter Paper Beta for macOS only. It publishes one complete release with
`SHA256SUMS.txt` and attestations, then asks `apotenza92/homebrew-tap` to update
`butter-paper` and `butter-paper@beta`; the bundle file name is recorded in the
tap registry. Verify publication and the affected downstream distributions
before calling the release complete.

A manual workflow run from `main` is an unsigned dry run that publishes
nothing; use it after changing packaging. macOS releases are Developer ID
signed and notarised; Windows and Linux packages are unsigned.

## Microsoft Store

Windows release jobs also build `Butter-Paper-Windows-x64.msix` and
`Butter-Paper-Windows-arm64.msix` (`--store-out`). They are run artifacts
named `microsoft-store-windows-*`, not release assets; the Store signs them.
Its identity in Partner Center is `xtask/templates/windows/store-identity.json`
and the listing text is [docs/store/listing.md](../store/listing.md).

After a stable release, in Partner Center (Apps and games > Butter Paper):

1. Download both artifacts: `gh run download <run> -p 'microsoft-store-*'`.
2. Start an update submission, replace the packages with both MSIX files and
   submit it for certification.

The Store copy needs the Microsoft Visual C++ runtime framework package,
which the Store installs with it.

Releases are immutable: never edit one; ship a new version. Never log, copy into
artifacts or commit a signing key or certificate.
