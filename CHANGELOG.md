# Changelog

All notable Butter Paper changes are recorded here.

## [0.0.14]

- Updated Electron to the supported 43 release line while preserving macOS 12
  compatibility.
- Refreshed the development and packaging toolchain to patched releases.
- Corrected the packaged TUF dependency graph and patched its glob-matching
  dependency so delegated update metadata can be evaluated safely.
- Removed an obsolete Windows ARM64 NSIS workaround that duplicated the
  executable and Electron runtime libraries inside the installer. The installer
  now reuses its single embedded archive when applying the ARM64 extraction
  fallback.
- Added package-boundary checks for the packaged TUF runtime and a release gate
  that rejects unexpectedly oversized Windows ARM64 installers.
- Preserved isolated stable and beta application identities and updater feeds.
  Stable releases advance both feeds; beta prereleases advance only beta.
- Windows and Linux users upgrading directly from 0.0.11 still need one manual
  install to bootstrap authenticated updates. Existing 0.0.13 installations
  update automatically.

## [0.0.13]

- Added authenticated automatic updates on native Windows ARM64/x64 and Linux
  ARM64/x64 AppImages using an embedded, reviewed TUF root. Windows and Linux
  users upgrading from 0.0.11 must install 0.0.13 manually once to bootstrap
  the new updater; later releases can update automatically.
- Added fail-closed update verification for corrupt, expired, incorrectly
  signed, missing, redirected, mismatched, and path-escaping metadata, while
  preserving any newer TUF root already trusted by an installed application.
- Added native Windows install/update/restart/data-preservation/uninstall and
  Linux AppImage update/restart/data-preservation release gates on matching
  ARM64 and x64 hosts. DEB and RPM upgrades remain managed by the system
  package manager.
- Added an offline-root/three-online-key TUF signing ceremony, protected
  metadata refresh workflow, deterministic release projections, and
  independent public-feed verification.
- Windows installers and Linux packages remain unsigned in this release.
  Published SHA-256 checksums, GitHub provenance, and TUF-authenticated
  automatic-update metadata protect distribution without implying platform
  code signing.
- Added native PDF file registration and a user-controlled Set as Default PDF App command on macOS, Windows, and Linux.
- Opened PDFs delivered at startup, through macOS open-file events, or to an existing application instance.
- Made this changelog the source for GitHub release notes and the automatic updater's in-app release notes.
- Reduced packaged-app overhead by retaining only the English Electron locale, excluding source maps, and keeping renderer-only libraries out of production dependencies.
- Kept Electron E2E, packaged-app smoke, and updater GUI verification off local
  macOS desktops; these checks now run only on disposable GitHub Actions hosts.

## [0.0.11]

- Removed the unused Edit, View, and Document menus from the application menu bar.
- Moved Set Page Scale into the PDF viewer toolbar so preset, custom, and calibrated page scaling remains directly accessible.

## [0.0.10]

- Preserved the native adaptive stable and beta icons introduced for 0.0.7.
- Kept strict signed-package verification compatible with macOS asset catalogs that expose light/dark stacks without separate artwork-layer metadata.

## [0.0.9]

- Preserved the native adaptive stable and beta icons introduced for 0.0.7.
- Accepted both native-vector and explicit full-canvas Icon Composer dimensions when verifying signed macOS packages across supported Apple runners.

## [0.0.8]

- Preserved the native adaptive stable and beta icons introduced for 0.0.7.
- Accepted both explicit and implicit native Icon Composer system-background layers when verifying signed macOS packages across supported Apple runners.

## [0.0.7]

- Added native macOS Icon Composer artwork with separate light and dark stable/beta appearances that fill the complete adaptive icon canvas.
- Added distinct generated light stable/beta icon assets for Windows and Linux while preserving each platform's native package formats.
- Added deterministic icon generation and packaged-catalog verification, and moved native macOS packaging to the macOS 26 runners required by Icon Composer.

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
