# Butter Paper PDF Signature Core

This directory contains the always-local Java 21 sidecar used by Butter Paper's
professional PDF-signature program. The Phase 1 reader adds read-only PDF
signature inventory, offline cryptographic validation, bounded exact-certificate
local trust decisions, and validated creation of a distinct structurally
unsigned copy. It deliberately does **not** use configured exact certificates
as CA/path anchors, use the network, overwrite a signed original, or enable
general editing of signed PDFs.

## Reproducible inputs

- Microsoft Build of OpenJDK `21.0.12+8-LTS` is the package/toolchain target.
- Apache Maven `3.9.11` is bootstrapped by `./mvnw` after SHA-512 verification.
- DSS `6.4` is imported through its BOM.
- PDFBox is explicitly constrained to `3.0.6`; the Jackson `2.21.5` BOM and
  Bouncy Castle `1.84` runtime artifacts are explicitly constrained.
- Every Maven plugin in this POM is versioned. The Enforcer gate rejects a
  different Java/Maven line, snapshots, dynamic versions, and dependency
  convergence failures.
- CycloneDX `2.9.1` generates `target/pdf-signature-core.cdx.json`.
- `src/license/runtime-cve-scan-input.json` records the exact Microsoft Build
  of OpenJDK version and checksum-pinned archive for all six native targets.
  It points scanners to the CycloneDX application SBOM and deliberately says
  `scanStatus: not-run`; it is an input/provenance record, not a vulnerability
  scan result or a claim that the runtime is free of known CVEs.
- `project.build.outputTimestamp` fixes archive timestamps.
- The complete DSS 6.4 upstream tag source archive used to assemble the
  complete-source artifact is fetched over HTTPS and accepted only at
  137,227,450 bytes with SHA-256
  `5f2421d6bf1c6073aa1e3c1ed4b44d2f058c6d751a4d89dbf326082860b224a4`.
  It is build/release input and is never fetched by the installed sidecar.

Maven Central artifacts are immutable, but that is not a substitute for review.
Package qualification must retain the generated SBOM, resolved dependency tree,
download checksums and notices for the exact release candidate.

## Build and test

Set `JAVA_HOME` to the exact Java 21.0.12 JDK, then run:

```sh
./mvnw test
./mvnw package
```

Build a native package only on the matching host:

```sh
./scripts/build-native-package.sh darwin arm64
```

The six supported invocations are `darwin|win32|linux` crossed with
`arm64|x64`; cross-building is refused. Output goes to
`build/package/<platform>-<arch>/`. `jpackage --type app-image` supplies the
native launcher and the JDK runtime image. The platform-specific launcher path
is declared in `manifest.json`, never guessed by the desktop app.

### Signing-safe package evidence

Schema 2 `manifest.json` hashes immutable jars, notices, the SBOM and
`complete-source-artifact.json`, while listing native binaries and signature
metadata separately. It explicitly describes an `unsigned-build` and remains
stable when downstream platform signing changes Mach-O, PE or signature
metadata.

The runtime package carries the applicable licence and dependency notice files,
the scan-ready Maven CycloneDX SBOM, the six-target Microsoft OpenJDK CVE-scan
input record, and `complete-source-artifact.json`; it does not carry the
137,227,450-byte DSS source archive. The descriptor binds one versioned,
immutable sibling `.tar.gz` complete-source/relink kit by exact filename,
byte length, SHA-256 and designated distribution location. That sibling must be
authenticated and retained at the same designated release location as the
corresponding binary release. Neither the sidecar nor the renderer fetches it
at runtime.

The sibling source artifact contains the pinned DSS archive, Butter Paper's
buildable Java tree, the reconciled dependency inventory and the build/relink
material needed to rebuild the application jar against the unchanged dependency
jars in the paired runtime. It also retains the exact CVE-scan input record and
application SBOM used by the paired runtime packages.
With the exact Java 21 JDK, a recipient can unpack the sibling kit and run its
offline relink procedure without downloading dependencies:

```sh
scripts/rebuild-from-package-source.sh \
  <unpacked-runtime-root> <unpacked-complete-source-root> <new-empty-output>
```

Package construction must run that command as an offline smoke and bind the
resulting sibling artifact into `complete-source-artifact.json`. Replacing a
dependency jar is permitted only after the recipient updates the evidence and
build procedure; that replacement-library workflow is not yet demonstrated.
Redistribution still requires rebuilding and redistributing both bound artifacts,
regenerating `manifest.json` and `post-sign-inventory.json`, then reapplying
every required platform and enclosing-package signature; otherwise the
fail-closed verifier correctly rejects the modified distribution.

After all nested platform signing (including the final nested macOS app seal)
or Windows Authenticode, but before the enclosing Electron application/package
is signed or TUF-hashed, run:

```sh
"$JAVA_HOME/bin/java" -cp target/pdf-signature-core.jar \
  com.butterpaper.signaturecore.PackageManifestWriter post-sign \
  build/package/<platform>-<arch>
```

This re-verifies all immutable components, rejects symlinks, non-regular files,
missing paths and unexpected additions, then hashes every regular component in
`post-sign-inventory.json`. That internal inventory deliberately says
`post-nested-signing-unsealed` and `releaseSealed: false`. Platform signing,
notarization, installer authentication and reviewed TUF hashes are separate
outer evidence; none changes the internal `releaseSealed` claim. Re-signing
requires regenerating the inventory before producing that outer evidence, and
the sibling source artifact must be authenticated by and retained with the
same designated release distribution.

The initial signing qualification accepts only classic PDF xref-table sources.
XRef streams, object streams, hybrid xrefs, linearized sources, and equivalent
serialization modes are rejected before the PKCS#12 identity is unlocked or
the output inode is written. This is an intentional fail-closed boundary: the
current PDFBox/DSS writer can preserve some such inputs, but pinned strict
pyHanko evidence rejects the resulting xref/object-stream revision history.
Those modes require a dedicated allocator/writer qualification across strict
cryptographic and commercial readers before the boundary can be relaxed.

## Development protocol smoke

The executable accepts no process arguments. Requests are NDJSON on stdin and
responses are NDJSON on stdout:

```sh
printf '%s\n' '{"protocolVersion":1,"requestId":"smoke","operation":"handshake","payload":{}}' \
  | "$JAVA_HOME/bin/java" -jar target/pdf-signature-core.jar
```

Diagnostics alone go to scrubbed stderr. Electron main owns file paths and may
send input/output paths only inside stdin payloads; the sidecar never returns or
logs those paths. Validation requires an explicit `onlineValidation: false`.
Online validation remains unsupported and a `true` request fails rather than
silently downgrading. `createUnsignedCopy` writes only into the existing empty
private `0600` inode pre-created by main, rejects inode replacement, and
validates the copy before returning hashes. Main invokes the independent
`inspectUnsignedStructure` operation in a fresh process before recoverable,
no-replace publishing. Visible signature appearances are part of the removed
signed widget, while unrelated page content, metadata, forms and annotations
are preserved and covered by deterministic structural and rendering tests.
On Windows, where Java exposes neither a stable file key nor link count, the
sidecar denies delete sharing and holds a Windows-mandatory exclusive whole-file
lock throughout write/validation/hash; main retains the separate bound handle
and owns the pre-spawn `nlink == 1`, ACL and final
handle-based publication checks.

## Release gate

Historical matching-host proof passed unsigned sidecar packages on all six
native targets in GitHub Actions run `31036932534` at commit `50222a1`: macOS,
Windows and Linux crossed with ARM64 and x64. The downloaded audit retains the
exact runners, package identities, 58 Java tests and 57 focused TypeScript tests
per target, byte-identical source siblings and relink evidence. That run is
historical feasibility evidence, not current release-candidate evidence.

The Strategy B release workflow collects one exact content-hashed
source sibling with the release assets, covers it with `SHA256SUMS` plus GitHub
provenance attestation, binds the signed/notarized macOS artifacts, binds the
Windows/Linux artifacts through the reviewed TUF path, and requires an exact
six-target aggregate. The current-source measurement-only Electron package
sub-gate passed all twelve stable/beta native jobs in GitHub Actions run
`31079099484` at commit `a98099b9137b3e62da0daefd88644420050194f3`.
Its downloaded evidence and the exact Windows/Linux ARM64 VM bundles were
independently re-hashed; both exact VM packages also completed real sidecar and
Electron-client handshakes in clean Parallels clones. Every measurement record
remains explicitly unauthenticated, unsealed and unaccepted. This is neither a
public release nor legal, security, budget or release-candidate approval.

macOS Developer ID signing/notarization, reviewed Windows/Linux TUF sealing,
accepted package-size/performance budgets, updater replacement and an exact
authenticated release-candidate run remain mandatory. Windows Authenticode is
not configured or claimed by the accepted strategy. The dependency inventory
and source descriptor retain `legalApproval: false`, and internal inventories
retain `releaseSealed: false`. Counsel must approve the exact Strategy B
source-delivery, relinking and authentication arrangement through the protected,
commit/version-bound release record before anything ships. Missing or
superseded evidence is never a pass.

The retained SBOM and runtime component record are scan inputs only. Release
qualification still requires a fresh vulnerability scan against the exact
release-candidate artifacts and then a reviewed disposition for every finding;
`scanStatus: not-run` must never be interpreted as a pass. Counsel must also
decide whether distribution of the jlink-created Microsoft OpenJDK runtime
requires additional corresponding-source delivery or written-offer material;
the current Strategy B sibling contains the DSS and Butter Paper source/relink
kit, not the Microsoft OpenJDK source tree.
