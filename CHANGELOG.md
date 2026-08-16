# Changelog

All notable Butter Paper changes are recorded here.

## [0.0.24]

### Startup performance

- Made blank and PDF launches faster by delaying non-essential updater,
  registration, template, signature, and PDF services until they are needed.
- Reused inspected PDF bytes during document opening to avoid reading the same
  file twice, while keeping PDF preparation available for direct file launches.

### Templates and verification

- Improved long template-name layout and kept the last-template preview within
  constrained windows.
- Added detailed startup timing diagnostics and expanded deterministic and
  packaged checks for blank documents, direct PDF launches, and lazy runtime
  boundaries.

## [0.0.23]

### Rendering and navigation

- Added adaptive rendering that responds to display refresh rate, available
  memory, processor capacity, render latency, and PDF.js backlog pressure.
- Improved page sharpness during normal scrolling while reducing background
  prefetch and high-quality work when the device or document is under load.
- Added direct arrow-key viewport scrolling, Page Up and Page Down scrolling,
  Home and End document navigation, and primary-modifier page navigation.

### Signatures and document controls

- Improved phone signature drawing controls and added context-menu deletion for
  individual recent signatures.
- Kept template and snapping popovers within constrained windows and removed
  obsolete blank-PDF settings compatibility code.

### Release delivery

- Reduced duplicate native release checks and artifact transfers, parallelized
  post-publication verification, and added non-publishing release simulations.
- Added an exact-commit candidate promotion simulation that verifies asset
  names, checksums, attestations, and update-feed sealing without publishing.

## [0.0.22]

### Templates and document workflow

- Added a reusable blank PDF template library with built-in and custom
  templates, PDF import, live previews, last-template reuse, and direct access
  from the document tab bar.
- Added cloud-storage source detection and opening progress for large PDFs.
  Improved safe save and Save As handling around opaque file access grants.
- Improved page thumbnails, blank-page previews, document opening feedback,
  constrained-window layout, and menu and rail controls.

### Markup tools and properties

- Added redaction placement, locked-markup handling, calibration-line feedback,
  dimension-increment snapping, and more consistent selection hit zones.
- Expanded tool properties with bundled standard annotation fonts, live text
  appearance updates, compact colour controls, persistent sortable colour
  presets, and construction-grid feedback.
- Improved annotation import and export fidelity for appearance, geometry,
  rotation, text, line endings, images, snapshots, and page content.

### Verification

- Added deterministic coverage for templates, saving, storage progress,
  calibration, colour presets, PDF round trips, phone-transfer limits, and
  packaged drag-and-drop behavior.
- Allowed explicitly isolated local macOS GUI checks while keeping updater
  replacement tests restricted to disposable GitHub Actions runners.

## [0.0.21]

- Introduced the Butter Paper brand identity with Trace Teal, Markup Coral,
  Studio Violet, Vellum, and Carbon across the repository and download page.
- Replaced the stable and beta application icons with the exact faceted origami
  butterfly geometry and channel colours derived from the master palette.
- Optically sized the transparent icon artwork to fill 80% of the canvas for
  clear native presentation on macOS, Windows, and Linux.

## [0.0.20]

### Signatures and image appearances

- Added a complete visual-signature workflow for drawing a signature, typing a
  signature with the bundled Allura face, importing a PNG or JPEG, or capturing
  a signature with a camera. Signatures remain ordinary image appearances and
  do not add certificate signing or claim cryptographic document approval.
- Added automatic paper removal, ink normalization, edge smoothing, transparent
  cropping, image-dimension limits, and isolated image decoding before a
  signature can enter the document or recent-signature store.
- Added up to five encrypted recent signatures with move-to-front reuse,
  explicit per-item deletion, clear-all handling, corruption checks, bounded
  operations, safe temporary files, and fail-closed behavior when secure system
  storage is unavailable.
- Added the end-to-end encrypted phone-transfer client and a disabled-by-default
  Cloudflare relay package with short-lived capability URLs, authenticated
  encryption, one-time payload acknowledgement, strict size and attempt limits,
  restrictive browser headers, and rate limiting. Public builds keep phone
  transfer unavailable until a reviewed HTTPS relay origin is configured.
- Added narrowly scoped macOS camera entitlement and usage descriptions. The
  runtime permission policy accepts only main-frame video from the application
  renderer and rejects microphone and unrelated permission requests.

### Markup creation and direct manipulation

- Reworked rectangle, ellipse, arc, line, polygon, polyline, pen, highlight,
  image, cloud, Cloud+, and callout placement around consistent click, drag,
  completion, cancellation, and post-placement selection behavior.
- Added stable three-point circular arc creation and direct arc hit testing,
  movement, and resizing. Added Shift-constrained circles and orthogonal segment
  placement without incorrectly constraining arc bulge points.
- Improved polygon and polyline node placement, Enter and Escape completion,
  start-node closure, screen-space cloud closure, multiline compound-markup
  growth, and native Cloud+ leader restoration.
- Added transient move and resize previews so pointer cancellation discards an
  edit and a completed drag creates only one document revision.
- Improved overlapping-markup targeting with topmost hit testing, direct handle
  interaction, double-click property opening, and selection behavior that stays
  consistent between the visible preview and committed edit.
- Added a tool cursor that combines a crosshair with the active markup glyph,
  and kept navigation cursors separate from drawing and direct manipulation.

### Snapping and tool properties

- Added acquired tracking points, horizontal and vertical tracking paths,
  virtual intersections, edge projections, PDF-content intersections, and
  precise-role priority within a viewport-pixel snap tolerance.
- Added equal-width, equal-height, equal-gap, and repeated-spacing snapping for
  new, moved, and resized annotations, with dedicated guide bars and source
  highlighting.
- Separated snapping behavior from guide visibility. Users can keep snapping
  active while hiding all guides or selected guide types, and real resize nodes
  remain visible when snapping is disabled.
- Added per-tool default property storage and reset controls. New annotations
  receive the active tool defaults without changing existing markups.
- Refined tool property panels, snap settings, confirmation controls, keyboard
  focus, and constrained-window scrolling with standard shadcn/Base UI controls.

### Blank PDFs and page controls

- Expanded blank PDF creation with A5 through A0 ordering, portrait and
  landscape controls, custom dimensions, page count, background color, and an
  exact aspect-ratio preview.
- Added grid, dot, lined, isometric, and triangle paper with configurable
  spacing and alphabetized pattern colors. Generated patterns use bounded PDF
  content and reject settings that would create excessive output.
- Improved page-scale presets, calibration interactions, thumbnail actions,
  pattern round trips, page rotations, and native image/snapshot appearance
  reuse.
- Improved thumbnail sizing for landscape and portrait pages and aligned the
  annotation overlay with capped preview geometry.

### Desktop shell and document workflow

- Added a native-height custom title bar with integrated window state,
  macOS full-screen behavior, menu-bar visibility control, and accessible
  stable and beta window identities.
- Added application zoom shortcuts, protected tab and window close shortcuts,
  macOS quit handling, document Home and End navigation, neighboring-page
  navigation, and canvas copy, paste, and select-all commands.
- Replaced blocking dirty-document prompts with anchored save and discard
  confirmations for tab closing and other local destructive actions.
- Improved toolbar, rail, tab, menu, popover, sidebar, and page-scale behavior
  at constrained sizes. Tool shortcuts remain available from non-editing
  controls without stealing keys from text fields, overlays, or system chords.
- Opened dropped PDFs in new tabs through an operating-system-backed path grant
  instead of relying on the removed renderer file-path property.
- Added macOS application registration repair that removes stale stable and beta
  registrations before registering the current installed application.

### Brand, packaging, and verification

- Replaced the quill artwork with the reviewed glass-document icon: a warm
  trace-paper slab, monochrome document content, a revision cloud, and a callout.
  Stable uses ruby-red annotation glass and beta uses violet annotation glass.
- Added canonical 1024 px light and dark artwork, reproducible Blender scene
  sources, adaptive macOS layers, and regenerated Windows ICO, macOS ICNS,
  Linux PNG, README, and download-page assets from one checked pipeline.
- Added third-party license notices for Signature Pad, Allura, QRCode, and its
  transitive QR dependency, and included the notice file in desktop packages.
- Expanded release verification for camera entitlements and usage strings,
  adaptive raster icon sources, dropped-file authorization, app registration,
  signature isolation, encrypted storage, and the disabled production relay
  boundary.
- Added broad deterministic coverage for markup placement, overlap handling,
  snapping, tool defaults, signatures, blank-paper patterns, window controls,
  menus, keyboard navigation, and packaging. The complete release gate now
  passes 995 deterministic application and relay tests.
- Windows and Linux users upgrading directly from 0.0.11 still need one manual
  installation to bootstrap authenticated updates. Later TUF-enabled releases
  continue to update automatically.

## [0.0.19]

- Made CAD View an explicit opt-in while preserving safe snapshot restoration
  when the feature is unavailable.
- Stabilized toolbar tooltip behaviour across pointer and keyboard interaction,
  including constrained popover and menu controls.
- Replaced the stable and beta icon artwork with the approved textured quill
  and faceted ink-pot mark, regenerated for macOS, Windows, Linux, and the
  download page, with dark artwork using the exact light-artwork black-to-white
  contour swap.
- Added deterministic checks for the icon geometry, channel palettes, release
  asset contract, download-page assets, Homebrew publication, and non-signing
  release boundary.

## [0.0.18]

- Redesigned the PDF workspace with clearer toolbar and rail organisation,
  resizable property panels, concise control hints, constrained menus, and
  consistent Base UI Nova controls.
- Expanded annotation workflows with improved snapping and visible snap
  markers, richer property controls, context menus, page rotation, refined
  selection and markup geometry, and stronger Cloud, Cloud+, Callout, and
  measurement behaviour.
- Added a complete blank-PDF workflow with reusable page settings and integrated
  it with document tabs, thumbnails, open, Save As, and normal PDF.js rendering.
- Removed the unused native Rust/PDFium renderer so the desktop app has one
  supported PDF.js rendering path, fewer native build requirements, and no
  dormant backend selector.
- Hardened renderer file access with owner-scoped document and Save As
  capabilities instead of renderer-supplied privileged paths.
- Added development source freshness checks, exact build provenance in the
  development window title, stale-renderer rejection, and repeatable desktop
  performance measurements for empty, one-page, and 100-page workloads.
- Replaced the stable and beta application icons with refreshed,
  channel-specific artwork across macOS, Windows, and Linux, simplifying the
  composition to one centred, faceted pen, and added a public download page
  with deterministic copy and asset checks.
- Expanded deterministic and hosted GUI coverage for the redesigned shell,
  properties, snapping, context menus, blank PDFs, page layouts, accessibility,
  constrained windows, and application provenance.

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
