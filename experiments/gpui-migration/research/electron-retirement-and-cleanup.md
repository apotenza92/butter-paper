# Electron retirement and repository cleanup criteria

Issue: [Define Electron retirement and repository cleanup criteria](https://github.com/apotenza92/butter-paper/issues/55)

## Decision

Remove Electron only after GPUI is already the sole shipped stable application,
the 60-day/two-stable-release rollback window passes, and a requirement-by-
requirement audit proves that Electron is no longer the only implementation or
test of any maintained product contract.

Retirement is a sequenced deletion, not an archive inside the active repository.
Do not keep a second `desktop-electron-old` tree, disabled workflow, commented
configuration, frozen package, or alternate launch command on the default
branch. Preserve source through immutable Git history and a final reviewed tag,
not through dead production code.

Keep long-lived compatibility readers and updater metadata that serve existing
users. They are GPUI product modules, not an excuse to retain Electron runtime
dependencies.

## Retirement entry gate

Every item must have authoritative current evidence:

1. The first GPUI stable release is public on every supported target and the
   second GPUI stable release has passed exact-package, native, updater, and
   publication qualification.
2. At least 60 days have elapsed since the first GPUI stable feed publication.
3. The 24-hour, 72-hour, 7-day, 30-day, and 60-day observation reviews passed.
4. No open P0/P1 migration, document-integrity, security, accessibility,
   renderer, package, updater, or performance incident exists.
5. Public crash/open/save outcomes and cohort evidence remain inside the cutover
   thresholds. Missing denominators are blocked, not silently accepted.
6. Latest-public Electron to GPUI and GPUI N to N+1 replacement pass on macOS,
   Windows, and AppImage ARM64/x64. DEB/RPM package-manager upgrades pass.
7. The emergency higher-version Electron package drill passed at stable day 14
   and before the second GPUI stable release without publication.
8. Stable/beta GPUI identity, runtime-epoch state, file association, signing,
   updater trust, package, Homebrew, install/removal, and coexistence gates pass.
9. The LegacyStateImporter is idempotent and preserves unread legacy state on
   every platform, including secure-signature migration failure.
10. `bp-perf-v2`, native accessibility/IME/display, public corpus, annotation
    round-trip, hostile PDF, save/reopen, and private-local workload or explicit
    waiver gates pass.
11. The GPUI release pipeline has produced and independently verified two public
    stable releases without calling Electron Builder, Forge, Electron,
    electron-updater, React, or the Electron renderer.
12. Product owner explicitly authorizes the retirement sequence and any remote
    branch/tag/workflow changes. Time elapsed does not grant that authority.

Any rollback trigger or new required-target failure resets the affected
observation and blocks retirement.

## Contract coverage ledger

Before deleting a file, map every maintained Electron behavior to exactly one
of these outcomes in the retirement change review:

- **Native owner:** a named GPUI/Rust module interface and deterministic/native
  test prove the behavior.
- **Shared owner:** a still-maintained non-Electron package and test prove the
  behavior because the CLI or another product still consumes it.
- **Compatibility owner:** a GPUI legacy reader, feed format, fixture, or native
  adapter must remain for existing installations/data.
- **Explicit retirement:** an intentionally removed behavior has a product
  decision, user impact, migration/recovery path, and release note.

An unowned, ambiguous, screenshot-only, historical, or manually remembered row
blocks deletion. The ledger belongs in the issue/pull-request review and
machine-enforced allowlists, not a permanent worklog.

The ledger must cover at least:

- application lifecycle, windows, menus, shortcuts, focus, dialogs, drag/drop,
  clipboard, file association, default-PDF flow, theme, full screen, and state;
- every shell region, control state, overlay, constrained layout, and reviewed
  visual/interaction difference;
- tabs, document state, PDF access grants, open/new/import/save/export/reopen,
  dirty close, external changes, temporary files, and publication safety;
- PDF parsing/rendering, page modes, virtual pages/thumbnails, zoom/pan/scroll,
  cache, workers, stale generations, performance diagnostics, and recovery;
- every annotation tool, geometry, hit test, selection, handle, property,
  snapping, measurement, undo/redo, import/export, and round-trip oracle;
- templates, page-scale presets, signatures, camera/local transfer permission,
  secure storage, sanitization, and legacy import;
- stable/beta identity, package formats, signing/notarization, updater trust,
  N-1 replacement, release provenance, publication, and rollback;
- keyboard, IME, native accessibility tree, screen readers, scaling, refresh,
  GPU failure, and all supported architectures/backends.

## What remains after Electron

### Compatibility modules

Keep for at least 24 months after the first GPUI stable release and through the
next supported GPUI major version, whichever is later:

- LegacyStateImporter parsers for reviewed Electron preference, template,
  page-scale, and encrypted recent-signature schemas;
- stable/beta legacy user-data discovery without writes to the legacy source;
- Electron-compatible `latest-mac.yml`, `latest.yml`, `latest-linux.yml`, and
  `latest-linux-arm64.yml` production metadata needed by supported old
  auto-updating installations;
- the Windows/Linux embedded TUF root, sequential root-rotation chain, required
  historical metadata versions, channel/architecture target names, and
  one-time-manual-install release-note fact;
- file association and product-identity values required to replace the old app;
- package-manager upgrade identity for DEB/RPM;
- public release assets, checksums, attestations, notarization records, and
  update-feed history already published.

These modules have native GPUI-facing tests and no Electron imports. At the end
of the support period, removal requires a fresh compatibility decision because
there is no automatic telemetry proving all dormant installations migrated.

### Product fixtures and oracles

Keep indefinitely while their product contracts remain supported:

- generated PDF geometry, document, hostile-input, and annotation fixtures;
- canonical annotation JSON, native PDF dictionary, save/reopen, and rendered
  crop oracles;
- stable/beta identity, asset-name, feed, TUF, legacy-state, and package
  fixtures;
- AEC tool icons, Fit Width/Fit Page/Continuous icons, Geist assets, Nova tokens,
  and reviewed visual geometry now consumed by Butter Paper-owned GPUI modules;
- semantic scenario IDs and accessibility expectations used by the native
  harness.

Do not retain an Electron-only fixture format when a renderer-neutral or native
fixture can preserve the contract.

### Historical record

Keep the final Electron source commit, lockfile, dependency notices, release
configuration, and tests in immutable Git history. When separately authorized,
create a reviewed annotated final-Electron tag that identifies the last public
release, source commit, public asset checksums, toolchain, and recovery notes.

Do not duplicate this history in an archive directory on the default branch.
Do not delete old public GitHub releases or rewrite their assets. The Wayfinder
map and closed decisions remain the durable rationale. Throwaway migration HTML,
stale screenshots, raw performance output, and local machine captures do not
move into production documentation.

## Ordered retirement slices

Each slice is independently reviewable and leaves the repository passing its
full current gate. Do not combine all deletion into one unreviewable change.

### R0 — freeze evidence and prove native defaults

- Complete the contract coverage ledger.
- Run the final emergency Electron build/package drill and retain its manifest
  in authorized release evidence.
- Verify every default developer, test, build, package, release-simulation, and
  documentation command already points to GPUI/native implementations.
- Verify two public GPUI stable releases were created without any Electron path.
- Add repository-hygiene rules that reject new Electron runtime/configuration
  dependencies while allowing only reviewed historical wording and native
  compatibility format names.

R0 changes no public feed and removes no rollback capability.

### R1 — retire active Electron release paths

- Remove Forge development configuration and commands.
- Remove Electron Builder configuration, after-pack scripts, ASAR/native-module
  verification, Electron package-smoke scripts, and Electron-specific package
  size rules after their native replacements own the same contracts.
- Remove electron-updater runtime and Electron-specific updater harnesses after
  native UpdateCoordinator and replacement tests cover every trust/fault case.
- Remove Electron jobs and branches from CI/release workflows. Keep native
  stable/beta, package, updater, TUF, Homebrew, signing, attestation, and public
  publication jobs.
- Preserve standard feed filenames and metadata content through native
  generators; do not remove them because Electron orchestration is gone.

Run a native release-pipeline simulation and all target package manifests before
R1 merges.

### R2 — remove the Electron application shell

- Delete Electron main-process bootstrap/window/menu/IPC implementations,
  preload bridge, context isolation declarations, renderer HTML entry, and
  Electron-only shared protocol.
- Delete the React application shell, Zustand view store, shadcn/Base UI desktop
  controls, CSS renderer, and browser-only hooks after their GPUI owners pass the
  coverage ledger.
- Delete Electron-only Vite/Forge TypeScript configurations and generated
  development preflight/runtime files.
- Remove Electron E2E helpers and Playwright UI tests only after equivalent
  native semantic, accessibility, interaction, screenshot/crop, and package
  scenarios pass on the exact targets.

Move no file to an `old`, `legacy-ui`, or disabled directory. Compatibility
readers are small native modules, not retained Electron shell code.

### R3 — retire Electron document and domain adapters

- Delete Electron-specific PDF access registries, IPC serialization, renderer
  document session/cache coordination, browser image/Canvas/SVG annotation
  painting, and main-process publication wrappers after native modules own the
  contracts.
- Preserve renderer-neutral algorithms only when another active caller exists.
  Port useful geometry, annotation, PDF, or state logic and its deterministic
  tests to the native owner before deletion; do not keep duplicate TypeScript
  implementations for reassurance.
- Keep TypeScript `packages/core` or `packages/pdf` only for actual CLI,
  signature-relay, fixture generation, release tooling, or compatibility
  consumers. Delete or narrow unreachable exports and dependencies based on a
  current import graph, not directory names.

R3 requires canonical PDF/annotation differential tests against the final
Electron reference corpus and the native writer/renderer oracles.

### R4 — prune toolchain and dependency graph

- Remove `electron`, Electron Forge, Electron Builder, electron-updater,
  Electron ASAR/signing helpers, Electron-only native binaries, React/React DOM,
  Zustand, shadcn/Base UI desktop-only dependencies, browser-only PDF.js
  dependencies, Vite renderer plugins, and Playwright if no active non-Electron
  consumer remains.
- Regenerate pnpm and Cargo lockfiles through normal package-manager commands.
- Remove pnpm build allowlists and workspace exceptions needed only by Electron.
- Regenerate target-specific license, source, notice, vulnerability, unsafe, and
  package manifests. Verify no removed runtime remains transitively.
- Remove stale package scripts, environment variables, test hooks, generated
  directories, cache ignores, and documentation commands.

Do not remove Node/pnpm generally if the CLI, relay, website, fixture generators,
or release orchestration still uses them. Retirement means no Electron/React
desktop runtime, not an unrelated language rewrite.

### R5 — final repository and clean-room audit

- Update root and package README files, AGENTS guidance, architecture diagrams,
  contributor setup, release runbooks, security model, and troubleshooting to
  describe only the active native desktop path plus time-bounded compatibility.
- Delete the migration experiment from active production branches after its
  decisions and implementation evidence are absorbed. Git history and the
  closed issue map preserve it.
- Run a clean-room dependency install, fixture generation, format/lint/test,
  native build, all six target compiles, every exact package, package smoke,
  native matrix, `bp-perf-v2`, release simulation, TUF verification, N-1 update,
  source/license inventory, and repository hygiene gate.
- Review the final diff and source tree for generated output, stale references,
  secrets, absolute paths, dead commands, disabled checks, and unrelated work.

R5 is complete only when a new contributor can build, test, package, and trace
the GPUI application without installing Electron or following an Electron-era
instruction.

## Machine-enforced absence audit

After R4, repository hygiene rejects active desktop references to:

- Electron packages and imports (`electron`, `electron-updater`, Forge, Builder,
  ASAR, Electron signing/packager modules);
- runtime types and globals (`BrowserWindow`, `ipcMain`, `ipcRenderer`,
  `contextBridge`, `webContents`, Electron `app`);
- Electron renderer entry/configuration, Vite Electron plugins, ASAR paths,
  Electron test hooks, and Electron executable environment variables;
- React/React DOM, JSX/TSX desktop renderer entries, Zustand desktop stores, and
  browser DOM/canvas/SVG UI code when no active non-desktop consumer exists;
- Electron-only workflow/job/script names and package commands.

Use a narrow reviewed allowlist for:

- LegacyStateImporter schema labels;
- Electron-compatible update metadata format documentation/tests;
- historical changelog entries and the closed migration decision links;
- public-release compatibility statements such as the 0.0.11 manual install.

An allowlist entry names a product/compatibility reason and removal review date.
Do not allow broad directories or generic words that hide new Electron code.

Also prove through package-manager queries and lockfile inspection that no
Electron/React desktop dependency remains transitively. A text search alone is
not proof.

## Test replacement rule

Delete a JavaScript UI test only when all of its assertions map to passed native
evidence at the same or stronger level:

- pure logic -> Rust deterministic module test;
- state/command -> AppModel/DocumentSession interface test;
- control interaction -> `butter_testkit` semantic and input test;
- layout/visual -> matched native capture/crop manifest at required sizes/states;
- accessibility -> native tree assertion and required screen-reader workflow;
- document/PDF -> canonical semantic/native dictionary/crop oracle;
- package/update -> exact native package, installer/helper, trust, and N-1 test;
- performance -> `bp-perf-v2` native metric.

Test count or line coverage is not equivalence. Preserve the behavior and its
failure mode. If a test documents an intentional retired behavior, link the
product decision before deleting it.

Implementation-neutral JavaScript release, fixture, CLI, relay, or website
tests may remain. Rename Electron-era names when the implementation is now
native so searches and ownership stay accurate.

## Post-retirement rollback policy

After R5 and explicit retirement approval:

- GPUI is the only maintained desktop runtime and public package source;
- ordinary defects fix forward with a higher GPUI version;
- release/feed rollback still follows immutable asset and authenticated metadata
  rules;
- the final Electron tag and public releases are recovery/history evidence, not
  an active supported branch;
- reviving Electron would be a new product decision and new qualification, not
  an emergency shortcut around failed native gates.

Remote branch deletion, tag creation, workflow setting changes, or public
documentation updates require separate explicit authority when executed.

## Release blockers

Stop any retirement slice for:

- an unowned contract-ledger row or weaker replacement test;
- rollback window, observation, incident, cohort, package, N-1, accessibility,
  performance, platform, license, or publication evidence that is missing or no
  longer current;
- a supported dormant Electron installation that cannot discover/install a
  current GPUI update because compatibility metadata was removed;
- legacy state/signature import that is destructive, partial, plaintext, or not
  idempotent;
- deletion of a shared CLI/relay/release consumer misclassified as desktop-only;
- an Electron/React desktop dependency or command remaining after the final
  absence audit;
- a clean-room build/package/release that needs an untracked file, stale cache,
  old checkout, private path, or Electron installation;
- loss of published release history, update trust chain, user documents, or
  recovery instructions.

## Current evidence status

- **Passed:** the active Electron/React source tree, dependencies, workspace
  exceptions, Forge/Builder configuration, JavaScript UI/E2E tests, package and
  updater scripts, CI/release workflows, shared packages, and documentation
  references were inventoried to define the retirement sequence.
- **Failed:** the current pinned GPUI dependency graph still fails the separate
  distribution license gate, so retirement cannot start.
- **Blocked:** no public GPUI stable release, second GPUI stable release,
  60-day observation, Candidate ledger, replacement-test mapping, or explicit
  retirement authority exists. macOS captures and the private Hibbeler corpus
  were not transferred to this VPS.
- **Not run:** all retirement entry gates, emergency Electron drills, contract
  coverage mapping, R0-R5 deletion slices, dependency pruning, final absence
  audit, clean-room native build/package/release, device qualification, and
  post-retirement compatibility review.
