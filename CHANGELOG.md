# Changelog

All notable Butter Paper changes are recorded here.

## [0.0.17]

- Standardized annotation interaction chrome across tools: hovering now shows
  faint blue geometry with pale handles, selected objects use dark blue chrome
  with solid yellow handles, and a hovered handle becomes visibly actionable.
- Added direct manipulation of unselected handles without forcing selection;
  clicking a handle still selects its object, and the first click after placing
  an annotation now deselects it instead of creating an accidental duplicate.
- Added CAD-style multi-object selection with Shift toggling, left-to-right
  contained selection, right-to-left crossing selection, freeform lasso,
  explicit add/remove zones, directional blue/green styling, operation badges,
  and cursor-following hotkey guidance.
- Simplified the annotation rails into stable Review and Draw groups while
  preserving resizing, overflow access, tooltips, accessibility, and the custom
  Butter Paper tool icons.
- Restored saved desktop window size and position safely across launches and
  display changes, while retaining the reviewed default and minimum geometry.
- Reworked cloud scallops as continuous cubic lobes with matching sampled hit
  geometry for a cleaner Bluebeam-style appearance and dependable selection.
- Expanded deterministic and hosted Electron coverage for annotation creation,
  hover and handle behavior, multi-selection geometry, window restoration,
  rail layout, keyboard accessibility, and shell overflow.

## [0.0.16]

- Replaced the experimental Butter Canvas surface with native blank PDFs that
  can be created in standard paper sizes, orientations, and page counts, then
  saved, annotated, and reopened through the normal document workflow.
- Refined the Nova desktop shell with integrated closable and reorderable
  document tabs, configurable annotation rails, streamlined tool controls, and
  safer unsaved-document handling.
- Standardized new desktop windows at 1200×800 with a 900×600 minimum while
  preserving accessible menus, popovers, dialogs, scrolling, and constrained
  layouts.
- Refreshed the renderer against the official shadcn Base UI Nova registry,
  moved typography to Geist Variable, and preserved Butter Paper's custom AEC
  and document-view icon geometry.
- Changed automatic-update defaults for new installations to weekly on the
  stable channel and daily on the beta channel. Existing saved preferences are
  preserved.
- Expanded deterministic and hosted Electron coverage for blank-PDF lifecycle,
  document tabs, shell overflow, rail behavior, update scheduling, and
  repository UI-component hygiene.

## [0.0.15]

- Added immediate, accessible in-app feedback for manual update checks,
  including checking, authenticated download progress, up-to-date,
  failure/retry, and ready-to-install states.
- Kept automatic checks quiet until an update is ready, while ensuring every
  manual check ends with visible success, no-update, or error feedback.
- Allowed the update progress window to close without cancelling its background
  download; the ready-to-install prompt still appears when that download
  completes.
- Preserved isolated stable and beta application identities and feeds. This
  stable release advances both products without converting beta installations
  into the stable app.

## [0.0.14]

- Updated Electron to the supported 43 release line while preserving macOS 12
  compatibility.
- Refreshed the development and packaging toolchain to patched releases.
- Corrected the packaged TUF dependency graph and patched its glob-matching
  dependency so delegated update metadata can be evaluated safely.
- Added package-boundary checks that execute delegated TUF path matching from
  the actual packaged application archive on macOS, Windows, and Linux.
- Updated electron-builder to use an NSIS-compatible ARM64 payload filter,
  removing the duplicated Windows executable and DLL workaround and adding
  package-size regression gates.
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
