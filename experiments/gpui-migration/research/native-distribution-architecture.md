# Native distribution architecture for the GPUI replacement

Issue: [Choose the packaging, signing, updater, and product-identity architecture](https://github.com/apotenza92/butter-paper/issues/53)

## Decision

Replace Electron packaging and update implementations without changing Butter
Paper's public product contract. The final GPUI stable and beta applications
take over the existing product identities, package names, file associations,
release asset names, channels, and update-feed roots. Internal and development
GPUI builds use isolated identities and can never replace a public installation.

Put distribution behind a deep `butter_platform` module. Product and document
code sees a small interface for product identity, package provenance, update
status, staged replacement, and recovery. Platform adapters own bundle/package
layout, signing verification, installer invocation, atomic replacement, and
native registration.

Electron remains the rollback product until the cutover gates pass. The two
runtimes never write the same mutable application-state schema during that
period.

## Canonical product identities

The release manifest is the single source of truth. Packaging scripts, native
application metadata, updater metadata, release asset contracts, file
associations, Homebrew metadata, and tests consume generated values from it.
Do not duplicate identity strings in unrelated scripts.

| Field | Stable | Beta |
| --- | --- | --- |
| Product name | `Butter Paper` | `Butter Paper Beta` |
| macOS bundle ID | `com.butterpaper.desktop` | `com.butterpaper.desktop.beta` |
| macOS bundle name | `Butter Paper.app` | `Butter Paper Beta.app` |
| Windows display/executable name | `Butter Paper` | `Butter Paper Beta` |
| Linux package/executable/desktop stem | `butter-paper` | `butter-paper-beta` |
| Release asset prefix | `Butter-Paper` | `Butter-Paper-Beta` |
| Channel | `stable` | `beta` |
| Feed root | `/stable/{platform}/{arch}` | `/beta/{platform}/{arch}` |
| Default automatic check | weekly | daily |

Stable and beta remain separate installed products with distinct application
IDs, package names, user-data directories, caches, updater state, file
registration, and feeds. They may be installed and run together.

Development identity includes source revision, branch, dirty-state fingerprint,
and checkout ID in visible diagnostics. Internal GPUI builds use a non-public
application ID, product name, user-data root, updater-disabled flag, and file
association policy. They must fail closed if packaging metadata is incomplete.

## Runtime-epoch state isolation and migration

The public product owns one platform user-data root per stable/beta identity,
but Electron and GPUI use separate runtime-epoch subdirectories during rollback:

- the existing Electron files remain the immutable `electron-v1` migration
  source, even when legacy versions stored them at the user-data root;
- GPUI writes only to a versioned `gpui-v1` subdirectory;
- updater trust/cache, thumbnails, raster cache, crash markers, preferences,
  templates, and recent signatures are separated by channel and runtime epoch;
- user PDFs remain ordinary user-owned files and are never copied into the
  application-state migration.

On first public GPUI launch, a one-time **LegacyStateImporter adapter** reads a
snapshot of allowlisted Electron state and writes a new GPUI transaction. It
never modifies or deletes the Electron source. It records source identity,
schema version, imported fields, rejected fields, and completion without file
paths, document content, or secrets.

Import only reviewed preferences, templates, page-scale presets, and recent
signature records. Native secure-storage adapters must decrypt legacy encrypted
signature blobs using the same platform account context, validate and sanitize
the decoded image, then re-encrypt it under a GPUI-specific key label. If secure
storage is unavailable, leave the legacy blob untouched and report signatures
as not imported; do not downgrade to plaintext.

The importer is idempotent and all-or-nothing. A crash before its commit leaves
no partial GPUI state. A newer importer may read an older GPUI schema, but the
Electron rollback application continues to read only its unchanged legacy
state. This preserves rollback without forcing both runtimes to share schemas.

After the stable rollback window closes, a separately authorized cleanup may
offer to remove legacy caches. Never remove user documents, templates,
signatures, or legacy state automatically as part of an update.

## Package and asset contract

Preserve these public formats on ARM64 and x64:

- macOS: signed/notarized/stapled DMG for installation and ZIP for updater and
  Homebrew use;
- Windows: signed per-user NSIS installer with custom install-directory support;
- Linux: AppImage, DEB, and RPM;
- Homebrew: stable and beta casks that install the exact public macOS ZIP.

Native packaging scripts replace Electron Builder, but must emit the same
reviewed asset names and metadata contracts unless a versioned release decision
changes them. They generate packages from one already-qualified Rust binary,
embedded fonts/icons/licenses, the channel manifest, the reviewed update trust
root where required, and a complete third-party notice inventory.

Every package contains:

- exact channel, version, architecture, application ID, feed, and update-target
  metadata;
- the reviewed public TUF root on Windows and Linux AppImage;
- no private signing or TUF key material;
- a package manifest containing hashes of native executables, libraries,
  resources, and notices;
- a signed or checksummed minimal replacement helper where that platform needs
  an out-of-process swap;
- deterministic diagnostic metadata with no source paths or secrets.

DEB and RPM upgrades remain package-manager controlled. The application never
self-replaces those installations. It may report that an update exists and name
the owning package manager, but it must not download or install an AppImage over
a DEB/RPM installation.

## Deep updater module

The `butter_platform` **UpdateCoordinator module** exposes this conceptual
interface:

```text
status() -> UpdateStatus
check(policy) -> CheckResult
download(candidate) -> StagedCandidate
apply(staged, restart_permission) -> ReplacementResult
recover() -> RecoveryResult
```

The interface includes channel, current/candidate version, artifact identity,
progress, required restart, disabled reason, and scrubbed error category. It
does not expose network clients, filesystem paths, platform installer flags,
TUF objects, or signing libraries to shell or document modules.

Internal seams have two real adapters:

1. **TrustVerifier adapter** authenticates metadata and the exact artifact.
2. **Replacement adapter** validates the staged candidate again, closes the app
   only after dirty-document policy permits it, replaces the application, and
   relaunches or restores the prior runnable artifact.

The replacement helper has no network access and receives only a staged
candidate handle, signed manifest, current-install identity, and one-use
authorization token. It revalidates channel, version, architecture, package
identity, hashes, and native signature before mutation. Use the least privilege
available; do not request elevation for a per-user update unless the existing
installation genuinely requires it.

Only one check, download, or replacement transaction may own a channel's update
state. Update state is revisioned and crash-safe. UI status is an observation of
the module; closing a panel cannot cancel or corrupt the transaction.

## Trust and metadata by platform

### macOS

Continue the HTTPS channel feed and `latest-mac.yml` transition contract so the
latest Electron application can install the first GPUI ZIP. The feed declares
channel, version, architecture-specific ZIP, byte count, SHA-512, and release
notes.

The GPUI TrustVerifier requires all of the following before replacement:

- HTTPS production feed with no credential, query, fragment, redirect to an
  unapproved origin, or test override;
- expected stable/beta channel and a strictly newer semantic version;
- expected ZIP byte count and SHA-512;
- safe archive expansion with no path escape, special file, or unexpected
  top-level bundle;
- exact bundle ID, architecture, executable, and product name;
- valid Developer ID signature from the reviewed Team ID, hardened runtime,
  expected entitlements, successful strict code-sign verification, and accepted
  notarization/Gatekeeper assessment.

Persist the highest accepted version and reject downgrade/replay. Loopback HTTP
is allowed only in an explicit disposable update-test mode. Native N-1
replacement is required on ARM64 and x64.

### Windows and Linux AppImage

Preserve The Update Framework (TUF) trust model and the existing standard
`latest*.yml` target names. The GPUI updater replaces the Electron local-proxy
implementation but not its security contract.

- Embed the reviewed public root and initialize a channel/architecture-specific
  trusted metadata directory with mode-limited permissions.
- Perform sequential root rotation and verify timestamp, snapshot, and targets
  signatures, thresholds, expiry, versions, lengths, and hashes.
- Reject rollback, freeze, mix-and-match, redirect, channel, architecture,
  target-name, and unexpected-origin attacks.
- Authenticate the exact `latest.yml`, `latest-linux.yml`, or
  `latest-linux-arm64.yml` target with TUF before parsing it.
- Verify the referenced artifact byte count and SHA-512. Windows also verifies
  Authenticode publisher, PE architecture, NSIS identity, and no web installer.
  Linux verifies AppImage architecture, executable metadata, package identity,
  and mode.
- Production repositories use HTTPS. The only override is an explicit
  loopback-IPv4 test repository in disposable update-test mode. A direct feed
  override never bypasses TUF.

The offline root private key remains offline. The `update-signing` environment
holds only distinct targets, snapshot, and timestamp keys and permits the
reviewed tag policy plus scheduled metadata refresh from `main`. No private key
may appear in packages, logs, artifacts, cache, or updater state.

### DEB and RPM

The release artifact is checksummed and attested, and its architecture,
dependency, GLIBC, desktop, MIME, identity, and notice contracts are verified.
Installation and upgrade ownership belongs to apt/dpkg or the RPM package
manager. If Butter Paper later publishes a signed apt or RPM repository, that
repository and its offline/online key policy require a separate decision.

## Replacement transaction and local rollback

Download into a channel-specific staging directory on the destination
filesystem. A staged candidate is immutable and content-addressed. Before the
application exits, require:

- complete TrustVerifier result;
- no active save publication or unresolved dirty-document decision;
- available disk space for the candidate, one previous runnable artifact, and
  rollback journal;
- current installed identity and expected source version;
- one-use replacement authorization.

The Replacement adapter writes an fsync'd journal, preserves the current
runnable artifact, installs or atomically renames the candidate, relaunches it,
and waits for a signed/local health handshake. Health requires process start,
native window, product identity, user-data schema open, updater state open, and
the smallest generated PDF open/render check. It does not open a private user
document automatically.

If mutation fails before relaunch, restore the prior artifact immediately. If
the new process fails health or crashes before writing the marker, the helper
offers or performs the reviewed same-version transaction rollback without
changing the public feed. Preserve logs and both artifact hashes. Once health
passes and the rollback window expires, remove only the staged package and
prior application copy; keep user state.

Windows may invoke the exact signed NSIS installer behind this transaction.
macOS replaces the bundle using the reviewed out-of-process helper. AppImage
atomically replaces the AppImage path while preserving executable mode. The
implementation must prove recovery through power-loss/fault injection on a
disposable virtual machine before release.

This local transaction rollback is not a public product downgrade. Public feed
rollback follows the rules below.

## Electron-to-GPUI transition

The first GPUI candidate for each channel is a normal higher-version package in
the existing feed:

1. The latest public Electron application downloads and installs the GPUI
   package using its existing updater path.
2. The GPUI app takes the same public application/package identity, creates its
   separate runtime-epoch state, and imports the allowlisted legacy snapshot.
3. Subsequent GPUI versions use the native UpdateCoordinator.
4. Electron rollback remains available as a separately identified emergency
   package and as source for a corrected higher-version public release. It does
   not share GPUI mutable state.

Prove native N-1 replacement from the latest actual public Electron package to
the exact GPUI candidate on macOS ARM64/x64, Windows ARM64/x64, and AppImage
ARM64/x64. Then prove GPUI N to GPUI N+1 using the same feed and trust roots.

Windows and Linux 0.0.11 packages have no updater bootstrap. The first
TUF-enabled release notes must state that those users need a one-time manual
install. Synthetic N-1 tests do not change that public fact. After the bootstrap
release, native N-1 replacement is mandatory.

DEB and RPM prove package-manager upgrade from the latest public Electron
package to the GPUI package with the same package identity, then GPUI N to N+1.

## Release provenance and publication

Keep the existing tag and channel model:

- stable tags are `vX.Y.Z` and build both stable and beta variants;
- beta tags are `vX.Y.Z-beta.N` and build beta only;
- root and desktop/native package versions match the tag;
- the tag commit is reachable from the `main` default branch;
- stable and beta use separate protected release environments and feeds.

For every target/channel/architecture:

1. Build once on the native runner from the tagged source.
2. Generate dependency/notices and package manifests.
3. Sign, notarize where applicable, and verify the exact package.
4. Run package smoke and native N-1 qualification against that exact hash.
5. Assemble the complete release asset set, SHA256SUMS, Homebrew bundle, feed
   metadata, and provenance manifest.
6. Attest the checksummed asset set.
7. TUF-sign the exact Windows and Linux metadata in the protected signing
   environment, continuing role versions from prior public metadata.
8. Create or verify a draft GitHub release. Never replace an existing asset.
9. Publish the release, independently download every public asset, and verify
   name, size, digest, channel classification, and complete asset contract.
10. Only then seal and atomically publish the corresponding updater-feed bytes
    to the `updates` branch. Preserve `.nojekyll`.
11. Independently verify that public feed bytes equal the sealed bundle and that
    every referenced public target passes trust verification.

Published release assets and authenticated metadata are immutable. A failed
publication stops before feed mutation when possible.

## Public rollback and correction

Never enable updater downgrades, reuse a version, replace a published asset, or
rewrite authenticated metadata to point at different bytes with the same
version.

- For a bad unpublished candidate, discard it and build a new version/hash.
- For a bad public application release, publish a corrected higher version. If
  GPUI must be withdrawn, package the maintained Electron rollback product as
  that higher version after it passes the same release gates.
- For a feed-only publication error, restore the exact prior feed commit when
  this does not contradict an already published target; otherwise publish
  corrected higher-version metadata.
- For compromised online TUF keys, follow the reviewed root roles and recovery
  procedure. Do not use the offline root key in the normal release workflow.
- For a compromised root or signing identity, stop publication and follow a
  separately reviewed incident process.

The beta/stable cutover ticket defines cohort and product rollback triggers.
This architecture ensures rollback never requires unsafe downgrade or shared
mutable runtime state.

## Required qualification

### Deterministic

- manifest generation and every stable/beta identity mapping;
- incomplete/mismatched metadata fails closed;
- settings and LegacyStateImporter transaction/fault tests;
- archive/path, hash, channel, architecture, version, publisher, and package
  rejection tests;
- TUF root rotation, threshold, expiry, rollback, freeze, mix-and-match,
  redirect, wrong-channel, wrong-target, and corrupted-state tests;
- replacement journal and every fault point before/after swap and health;
- release asset, feed, tag, environment, attestation, and immutable-publication
  contracts;
- no private keys, secrets, source paths, or private document data in packages
  and evidence.

### Native package

- clean install, association launch, normal Open/Edit/Save/Reopen, update check,
  download, dirty-document restart block, replacement, health, relaunch,
  recovery, and uninstall/removal;
- stable/beta coexistence and state/feed isolation;
- latest public Electron N-1 to first GPUI, then GPUI N to N+1;
- interrupted download, disk full, permission denial, corrupt staging, signature
  failure, installer failure, process crash, helper crash, health failure, power
  loss, offline, proxy, and clock-skew behavior;
- exact ARM64 and x64 packages under the native qualification matrix.

macOS updater replacement runs only on disposable GitHub Actions runners or a
dedicated disposable virtual machine because it replaces bundles and system
registration. Windows NSIS and Linux update replacement also use disposable
native runners or virtual machines. Ordinary local device testing may inspect
and smoke an isolated exact package but must not replace the installed public
application.

## Release blockers

Block packaging or publication for:

- unresolved GPUI/runtime license or source inventory;
- any identity, channel, architecture, user-data isolation, association, or
  package-format mismatch;
- missing or invalid native signature, notarization, TUF verification, notice,
  checksum, attestation, or provenance;
- unsafe archive or replacement behavior, incomplete local rollback, corrupt
  legacy-state import, or loss of user documents/state;
- failed latest-public Electron-to-GPUI N-1 proof on any updater-capable target;
- pretending the pre-bootstrap Windows/Linux population can auto-update;
- public assets that differ from the verified build or a feed published before
  independent public-asset verification;
- an attempt to overwrite public assets, reuse a version, enable downgrade, or
  expose private keys.

## Current evidence status

- **Passed:** current product metadata, Electron Builder targets, stable/beta
  channel behavior, updater policies, TUF verifier, release asset contract,
  native package smokes, N-1 workflows, protected signing/publication order, and
  durable repository rules were inspected to define the replacement seams.
- **Failed:** the current pinned GPUI dependency graph still fails the separate
  distribution license gate.
- **Blocked:** no distributable GPUI candidate exists while that license gate is
  unresolved. Current macOS visual captures and the private Hibbeler corpus were
  not transferred, but neither is needed to decide this distribution seam.
- **Not run:** native GPUI package construction, signatures, notarization,
  LegacyStateImporter, UpdateCoordinator, TUF client, replacement helper,
  Electron-to-GPUI N-1, GPUI-to-GPUI N+1, fault injection, device/package,
  updater, and publication qualification.
