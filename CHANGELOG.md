# Changelog

All notable Butter Paper changes are recorded here.

## [0.0.6]

- Canonicalized temporary macOS executable paths before detecting an updater-started replacement process.

## [0.0.5]

- Accepted the conventional standalone argument separator in the native macOS updater harness across pnpm forwarding behaviors.

## [0.0.4]

- Hardened draft release staging with numeric release IDs and exact remote size and SHA-256 verification before publication.
- Preserved hidden static update-feed files in sealed publication artifacts.
- Updated Homebrew cask rendering, auditing, installation, and cleanup for current Homebrew behavior.

## [0.0.3]

- Passed the selected stable or beta identity through every native package verifier.
- Restored the Windows ARM64 executable and Electron runtime libraries after NSIS archive extraction so the native assisted installer contains the complete packaged application.
- Expanded manual package CI to exercise both release channels at native package boundaries before tagging.

## [0.0.2]

- Corrected native release packaging for macOS OpenSSL 3 signing, canonical Linux architecture filenames, Windows ARM64 installation readiness, and slower hosted beta launches.

## [0.0.1]

- Added the first cross-platform Butter Paper desktop release for native Apple Silicon, Intel Mac, Windows ARM64/x64, and Linux ARM64/x64 packages.
- Added separate stable and beta application identities, data directories, branding, and release channels so both variants can coexist.
- Added signed and notarised macOS packages with configurable automatic update checks, dirty-document protection, channel isolation, and native N-1 verification.
- Added deterministic package, release-asset, checksum, updater, accessibility, and document-persistence coverage while preserving the custom AEC, Fit Width, Fit Page, Continuous, and Butter Canvas icons.
- Adopted the maintained shadcn/ui Base UI and Rhea component conventions for standard desktop controls.
