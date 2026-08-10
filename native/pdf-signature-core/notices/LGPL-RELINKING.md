# DSS LGPL-2.1 relinking and source-delivery strategy

Butter Paper starts PDF Signature Core as a separate executable over stdin and
stdout. Project-owned MIT code is compiled into `pdf-signature-core.jar`; DSS,
PDFBox, Jackson and every transitive dependency remain separate jars inside the
native app image. They are not shaded, modified or encrypted. The supplied
proof rebuilds Butter Paper's application jar against the unchanged jars in
the paired runtime. A recipient can replace compatible library jars and
run/repackage the sidecar, but must also update the evidence and build
procedure; a replacement-library relink has not yet been demonstrated.

For each distributable release candidate, retain and ship one bound pair. The
runtime package must carry:

1. the exact SBOM and package manifest with hashes;
2. every applicable copyright and licence text;
3. the Microsoft runtime legal/third-party notice files; and
4. `complete-source-artifact.json`, which binds an exact versioned, immutable
   sibling `.tar.gz` by filename, byte length, SHA-256 and designated
   distribution location.

The sibling `.tar.gz` must contain the complete checksum-pinned upstream DSS
6.4 tag archive and the Butter Paper sidecar source/build material needed to
relink against a compatible modified DSS library, including the Java source,
the dependency inventory and offline rebuild procedure. The supplied procedure
uses the unchanged separate dependency jars from the paired runtime. The
sibling artifact must be
authenticated and retained at the same designated release location as the
corresponding binary artifact. It is never fetched by Butter Paper at runtime.

Release verification must fail closed if the sibling artifact is unavailable,
its hash or length differs, its distribution binding is absent, or it is not
covered by the required release authentication and retention evidence. The
runtime package's internal `releaseSealed: false` remains unchanged; platform
signing, notarization, installer signatures and TUF hashes are separate outer
evidence for the bound distribution pair.

This engineering control does not conclude that the legal obligations have
been satisfied. The dependency inventory remains `legalApproval: false`, and
counsel must approve the exact LGPL source-delivery and relinking arrangement
before shipping. Local builds, native proof runs and unpublished artifacts are
not release publication or legal approval. No proprietary trust-list data or
commercial application binaries are fetched or redistributed.
