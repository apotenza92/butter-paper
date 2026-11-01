# Digital signature fixtures

Every certificate, key and identity here is made up for the tests.

- `make_fixtures.py` (pyHanko 0.29) makes the rest of this folder from
  `../text-edit/word-table.pdf`. It writes:
  - a test root (`test-root.cer`);
  - RSA and ECDSA signers (`test-signer-*.p12`, password
    `butter-paper-test`; the `-legacy` copies use 3DES as older exports do),
    and a revoked signer with the root's CRL and OCSP answers;
  - PDFs signed once, visibly, with a timestamp and twice;
  - PDFs changed after signing: a note added, content drawn and a signed
    byte changed.

  Regenerating replaces every key, so the tests assert on names and
  outcomes, not on bytes.
- `make_non_ca_fixture.py` makes `signed-by-a-non-ca.pdf` from the `.p12`
  files: "Jane Doe" at "Big Bank", whose certificate the RSA signer issued
  though it is no CA (an impersonation a checker must not trust).
- `gpo-public-law-118-1.pdf`: Public Law 118-1 from govinfo.gov (a US
  government work in the public domain), signed by the US Government
  Publishing Office in BER, as Acrobat-style signers write it.
- `corpus/` holds cases from an earlier signing project's public validation
  corpus (pyHanko 0.35, signed 2024-01-15, root `corpus/root.pem`). The
  tests compare them with that corpus's pyHanko verdicts at its validation
  time, 2026-08-05T05:00:00Z:

| Case | pyHanko |
| --- | --- |
| `expired-now`, `not-yet-valid` | intact, untrusted: the certificate is out of date without a timestamp |
| `missing-intermediate` | intact, untrusted: the chain stops before a root |
| `malformed-byte-range` | invalid |
| `multiple-signatures` | both intact; the first is followed by the second |
| `duplicate-signature-field` | intact, with an illegal change: a second field named like the signed one |
| `revoked-with-embedded-evidence` | intact, with long-term validation data added later (its CRL revokes the signer) |
| `corrupt-incremental-revision` | its last revision is broken |
| `certified-no-changes` | a certification signature allowing no changes (DocMDP 1); signing it again is refused |
