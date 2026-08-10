# Protocol version 1
Transport is UTF-8 newline-delimited JSON over stdin/stdout. Stdout contains
protocol envelopes only. The process may read multiple requests for contract
testing; Electron uses one short-lived process per operation.
Request:
```json
{"protocolVersion":1,"requestId":"request-1","operation":"handshake","payload":{}}
```
Terminal response:
```json
{"engineVersion":"0.1.0","event":"result","ok":true,"operation":"handshake","protocolVersion":1,"requestId":"request-1","result":{}}
```
Errors use `event: "error"` and `error: {"code": "...", "message": "..."}`.
Human console strings are never part of the contract.
Operations:
- `handshake` and `version`: deterministic engine versions, limits and honest
  capability flags. Offline signature reading/validation and structurally
  unsigned-copy creation are enabled; signing, online validation, LTV, and
  mutation of a signed source remain disabled.
- `inspect`: accepts main-process-only `inputPath`; rejects symlinks,
  non-regular inputs and inputs over 512 MiB; returns SHA-256, byte size and
  bounded marker observations. It always returns `structuralOnly: true` and
  `validationPerformed: false`.
- `validate`: requires main-process-only absolute `inputPath` and an explicit
  `onlineValidation` boolean. `true` is rejected with
  `ONLINE_VALIDATION_UNSUPPORTED`; it is never downgraded to offline. Offline
  validation captures system UTC once by default and returns that canonical
  instant as `validationTime` with `validationTimeProvenance` set to
  `observed-system-utc`. Deterministic reference callers may instead send
  `validationClock:{"mode":"fixed-reference","instant":"2026-08-05T00:00:00Z"}`
  over stdin. The fixed instant must be canonical UTC, no earlier than
  1900-01-01, and no later than system UTC observed while parsing the request;
  malformed, ambiguous, extra-field, and future references fail with
  `INVALID_VALIDATION_CLOCK`. The override is not read from environment
  variables or process arguments and must remain a privileged main-process
  test/reference facility rather than a renderer control. Certificate status
  uses exactly the reported validation instant.
  validation inventories fields, widgets, signature dictionaries, byte ranges,
  revisions, transforms, certificates, timestamps, and independent validation
  axes. `inventory.validationEvidence` is a separate structural-only inventory
  of the Catalog `DSS` and `VRI`: it reports DSS/VRI presence, bounded structural
  collection counts, embedded-stream observations, and at most 1,024 sorted VRI
  entries with bounded key references plus SHA-256 hashes of the full reference.
  Each collection distinguishes reference count, embedded-object count,
  malformed-entry count, and inspection completeness. `structureStatus`,
  `inventoryComplete`, and `limitExceeded` fail honestly for wrong object types,
  non-canonical VRI SHA-1 key names, and more than 4,096 inspected evidence
  references. Counts and presence never claim that a certificate, OCSP response,
  CRL, or VRI association is decoded, valid, current, or cryptographically bound;
  no evidence body is returned. Validation never returns an aggregate `valid`
  conclusion, never loads a system
  trust store, and configures no AIA, OCSP, or CRL network source. Optional
  `trustPolicy` has `{policyId, policyVersion, configurationSha256,
  exactCertificateAnchors:[{sha256Fingerprint,derBase64}]}`. It accepts at most
  16 exact certificates, 32 KiB DER each and 512 KiB total; verifies strict
  lowercase fingerprints, DER parsing, uniqueness and the canonical
  configuration hash; and never treats one as a CA for descendants. Invalid
  policy input fails instead of falling back to empty trust. Responses bind the
  policy fields and sorted configured fingerprints without returning DER.
- `createUnsignedCopy`: accepts main-process-only absolute `inputPath` and a
  distinct existing empty `0600` regular `outputPath` inode inside a `0700`
  private workspace, both through stdin. It rejects symlinks, unsafe permissions
  and multi-link outputs where the platform exposes link counts; records the
  output identity; writes/truncates that same inode without replacement; and
  proves the identity is unchanged afterward. It reads the original
  immutably, removes signed fields/widgets, their signature values and locks,
  Catalog `Perms/DocMDP`, and `DSS/VRI` material in a full-rewrite copy, then
  reopens and validates the private output. It returns input/output hashes,
  bounded removal metrics and zero-only structural postchecks, but never paths.
  A visible signature widget and its appearance are intentionally removed; a
  deterministic render test proves other page content remains pixel-identical
  outside that widget while unrelated metadata, forms and annotations survive.
  It refuses an unsigned input, source changes, and files over the input limit.
  A failed write is truncated back to zero only while the inode identity still
  matches. The caller independently validates and performs final no-replace
  publication; the signed original is never overwritten.
  On Windows, the JDK does not expose a file key or link count: the sidecar
  therefore holds `NOSHARE_DELETE` plus an exclusive whole-file lock from its
  pre-write recheck through structural validation and hashing. The delete-share
  rule blocks path replacement and Windows' mandatory byte-range locking blocks
  concurrent writes without conflicting with main's already-open handle.
  Electron main must keep its independently validated
  private handle open, require `nlink == 1` before spawn, and publish/read back
  through that handle; the sidecar never substitutes a path or timestamp as an
  identity. A failure does not path-truncate an identity-less Windows target.
- `inspectUnsignedStructure`: independently reopens a main-owned `inputPath` in
  a fresh process and returns its SHA-256, `structurallyReadable: true`, and
  bounded counts for byte-range markers, signature dictionaries, signed fields,
  DocMDP, FieldMDP, and DSS/VRI. It returns no paths and does not rely on a
  preceding create result.
- `cancel`: acknowledges whether a target is active. Current work is synchronous,
  so it returns `not-running` and tells main that process cancellation is the
  effective cancellation boundary.
Limits before/while decoding include a 1 MiB line, JSON depth 32, JSON string
length 256 KiB, 1,024 aggregate container entries, safe request IDs of at most
128 characters, paths of at most 4,096 characters, 512 MiB inputs and 4,096
reported markers of each kind.
Opening/inspecting a file does not prove it contains a signature. An offline
`validate` result can establish signed-byte integrity and can report an explicit
exact-certificate local decision, but that decision is not certificate-path or
legal trust. Embedded revocation and timestamp evidence are reported only when
their cryptographic bindings verify; online/current status remains unavailable.
## Experimental framed protocol version 2
Protocol version 1 remains unchanged for Phase 1 read/validation operations.
Secret-bearing Phase 3 proof operations use a separate one-request-per-process
transport. It is not a network protocol and opens no listener.
Wire format, all unsigned lengths in network byte order:
```text
"BPS2"
uint32 JSON header byte length
strict UTF-8 JSON header
for each declared frame:
  uint32 frame byte length
  exact raw frame bytes
```
The process requires EOF immediately after the final frame. A response is
`BPS2`, one bounded length, and one structured JSON result/error envelope.
There are no binary response frames. Cancellation is the short-lived process
termination boundary.
The header has exactly:
```json
{
  "protocolVersion": 2,
  "requestId": "request-1",
  "operation": "inspectPkcs12",
  "payload": {},
  "frames": [{
    "id": "pkcs12",
    "kind": "pkcs12",
    "byteLength": 4096,
    "sha256": "lowercase-sha256",
    "sensitive": true
  }]
}
```
Only fixed `pkcs12` and optional `appearance` frame identifiers/kinds exist.
Each is at most 16 MiB, total frame bytes are at most 32 MiB, the JSON header is
at most 1 MiB, and no request has more than three frames. Length/hash mismatch,
truncation, duplicate/unknown frames, trailing bytes, malformed UTF-8/JSON and
container-limit violations fail before the operation runs. Protocol, frame and
field numeric values are accepted only as bounded JSON integers; fractional
values are never truncated. Every accepted frame buffer is cleared on success
or on any later frame/read/dispatch failure. Each operation payload is a closed
schema: missing and undeclared properties are rejected before file or provider
access, so passwords and private-key material are never accepted as JSON fields.
Operations are:
- `handshake`: reports the experimental operations, PAdES Baseline B and
  PKCS#12 provider. `certificateSign`, `certify`, `signatureFieldCreate`,
  `signatureIncrementalWrite`, `signedIncrementalEdit`, `timestamp`, and
  `onlineValidation` remain `false` until their external phase gates pass.
- `inspectPkcs12`: accepts one encrypted PKCS#12 frame. The sidecar owns the
  secure `JPasswordField` prompt. The password stays in a mutable Java `char[]`
  and never enters Electron renderer/main memory, stdin/stdout, argv, env or
  logs. Public certificate descriptors are returned; no path, password, alias,
  private key, PFX bytes or password hash is returned.
- `addSignatureField`: accepts no frames, an immutable source snapshot path/hash,
  a distinct precreated empty private output inode, stable new field name,
  optional visible widget in unrotated PDF default user space, and optional
  `all`/`include`/`exclude` lock. Signed sources are rejected. Output must retain
  the exact input prefix.
- `sign`: accepts PKCS#12 and optional PNG/JPEG appearance frames, SHA-256/384/512,
  PAdES Baseline B, bounded metadata, and an existing or new field. A new field
  is prepared incrementally before DSS signs it. New fields on already-signed
  inputs and locked/signed existing fields are rejected. Image dimensions are
  bounded through `ImageReader` metadata before raster decode. Visible requests
require one effective widget and an appearance frame; invisible requests
require no effective widget. Existing visible widgets must have finite positive
geometry inside the page CropBox, a supported page rotation and a bounded
positive UserUnit before the identity is unlocked.
- `certify`: same secret/appearance boundary as `sign`, but only on an unsigned,
  unambiguous source. Permission values map exactly to DocMDP P=1/P=2/P=3.
- `postvalidateSignedMutation`: accepts no frames and no identity, password or
  appearance material. Its closed payload contains only `inputPath`,
  `outputPath`, `expectedInputSha256`, `expectedOutputSha256` and
  `expectedFieldName`. A new one-shot sidecar process reopens both private files,
  proves their hashes and exact prefix relationship, requires exactly one added
  signature bound to the named field, preserves every prior signature, validates
  the direct `/ByteRange` and `/Contents` gap, signature policy and visible
  appearance structure, and independently verifies every signature with
  DSS/PDFBox. The narrow result contains only the two hashes, field name, fixed
  successful postcheck booleans, `independentProcess: true`, and validator ID
  `pdf-signature-core-v1-validate-plus-main-prefix`. It remains an experimental
  proof operation: all production signing and incremental-write handshake
  capabilities stay `false`.
`sign` and `certify` use DSS 6.4 `Pkcs12SignatureToken`,
`PAdESService.getDataToSign`, token `sign`, and `signDocument`. SHA-1/weak keys,
timestamps, online validation, PKCS#11, OS stores and remote signing are not
available through this protocol.
The source is never writable. Mutation output must be an existing empty `0600`
regular single-link inode in a private `0700` workspace (or the independently
verified Windows ACL equivalent). The sidecar holds an exclusive lock, writes
that same inode without replacement, forces it, and proves the source identity
and SHA-256 are unchanged. Accepted outputs must preserve every input byte as
an exact prefix, add exactly one signature, retain prior signature byte ranges
and CMS values, cover the final output except the new `/Contents` gap, contain
the requested field/certificate and cryptographically validate through a fresh
DSS/PDFBox parse. The gap must be the direct hex `/Contents` value in the exact
xref-resolved added signature object, whose direct `/ByteRange` value must match;
PDF strings, comments, names, arrays and nested dictionaries are tokenized rather
than substring-scanned. Exactly one
field may bind the added signature COS object, and exactly one DSS signature
with that byte range must carry the requested certificate. Visible output is
reopened to verify its page, rotation, rectangle and a bounded nonempty normal
appearance Form XObject after filter decoding (or a bounded appearance-state
dictionary whose `/AS` selects a valid Form XObject), including resources and
BBox. The added DocMDP and FieldMDP transforms must exactly match the requested
permission and signed-field lock, including their raw type, version, action and
field list. This is structural validation;
cross-renderer visual interoperability remains a separate evidence gate. Prior
DocMDP and FieldMDP decisions are reconstructed from the exact cryptographically
intact signed revisions and uniquely cross-mapped between PDFBox and DSS. The
current Catalog must retain the exact authoritative `/Perms/DocMDP` designation;
removed, substituted, malformed, conflicting, unsigned-trailing, or indeterminate
policy revisions fail closed rather than trusting mutable current field `/Lock`
dictionaries. Failure truncates only the same proven output inode.
The initial signing qualification additionally accepts only classic xref-table
sources. XRef streams, object streams, hybrid xrefs, linearized sources, and
other unsupported serialization modes fail closed with
`SOURCE_SERIALIZATION_UNSUPPORTED` before identity unlock or output creation.
All framed errors have a stable uppercase code and the same generic message.
Provider/JCA exception messages, filenames, paths, identity aliases, certificate
names, document bytes and appearance bytes are never diagnostic output.
