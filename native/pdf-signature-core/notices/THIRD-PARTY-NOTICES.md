# PDF Signature Core third-party notices

This package keeps the Butter Paper application jar and all dependency jars as
separate, unmodified files. The exact component inventory, versions, hashes and
declared licences are in the packaged CycloneDX SBOM and `manifest.json`.
The complete-source sibling's `generated/dependency-inventory.json` reconciles
every SBOM component to its hashed runtime jar, checksum-verified full licence
text, source URL, and retained jar `META-INF/LICENSE`, `NOTICE`, or
`DEPENDENCIES` files. The runtime deliberately does not copy that
source-delivery inventory.

The runtime package carries `complete-source-artifact.json` instead of embedding
the complete upstream DSS 6.4 tag source archive. That descriptor binds one
versioned, immutable sibling `.tar.gz` complete-source/relink kit by exact
filename, byte length, SHA-256 and designated distribution location. The
sibling contains the pinned upstream source archive, Butter Paper relink source
and build material, and the reconciled dependency inventory. The supplied
offline proof rebuilds Butter Paper's application jar against the unchanged
separate dependency jars from the paired runtime. A recipient may provide and
relink compatible replacements, but must also update the evidence and build
procedure; that replacement-library workflow is not yet demonstrated. It must be authenticated
and retained at the same designated location as the corresponding binary, and
Butter Paper never fetches it at runtime. Missing, altered, unretained or
unauthenticated sibling-source evidence fails release verification.

The principal signature stack is European Commission Digital Signature
Services (DSS) 6.4, including `dss-pades-pdfbox` and `dss-token`, licensed
LGPL-2.1. Its source
release is available from the upstream `esig/dss` project and Maven Central
source artifacts. Apache PDFBox 3.0.6 is Apache-2.0. Jackson 2.21.5 is
Apache-2.0. The Microsoft Build of OpenJDK 21.0.12 runtime has its own GPLv2
with Classpath Exception and third-party notices within the runtime image.
The packaged `sbom/runtime-cve-scan-input.json` identifies the exact six
checksum-pinned Microsoft JDK build archives used as native runtime inputs and
retains `scanStatus: not-run`. It is scan provenance, not a passed CVE scan.

This notice is an engineering inventory, not legal advice or legal approval.
Before release, the exact resolved SBOM must be reviewed, all required licence
texts/notices and corresponding-source delivery must be confirmed, and counsel
must accept the separate-process/relinking strategy. The inventory deliberately
continues to report `legalApproval: false`. Local/native proof is not release
publication or legal approval. The runtime package's internal
`releaseSealed: false` is also unchanged; platform signing, notarization,
installer authentication and TUF hashes are separate outer evidence for the
bound runtime/source pair.

The DSS POM names the GNU Lesser General Public License and links LGPL 2.1,
but does not express an SPDX `-only` or `-or-later` choice. The inventory calls
this evidence `LGPL-2.1-text` and deliberately leaves that ambiguity for review.
The current complete-source sibling does not contain the Microsoft OpenJDK
source tree. Counsel must determine whether the shipped jlink runtime requires
additional corresponding-source delivery or written-offer material and approve
that exact release arrangement before distribution.

Primary source locations:

- DSS 6.4: https://github.com/esig/dss/tree/6.4
- DSS Maven sources: https://repo.maven.apache.org/maven2/eu/europa/ec/joinup/sd-dss/
- PDFBox 3.0.6: https://github.com/apache/pdfbox/tree/3.0.6
- Jackson Databind 2.21.5: https://github.com/FasterXML/jackson-databind/tree/jackson-databind-2.21.5
- Microsoft OpenJDK source: https://github.com/microsoft/openjdk-jdk21u
