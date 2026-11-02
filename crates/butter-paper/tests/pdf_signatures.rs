//! Digital signatures read from PDFs signed by pyHanko
//! (`tests/fixtures/signatures/make_fixtures.py`).

use std::path::PathBuf;

use butter_paper::pdf_signatures::{Changes, Integrity, SignatureReport, Trust, signatures};

fn fixture(name: &str) -> Vec<u8> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/signatures")
        .join(name);
    std::fs::read(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

fn test_root() -> Vec<Vec<u8>> {
    vec![fixture("test-root.cer")]
}

fn only(name: &str) -> SignatureReport {
    let mut found = signatures(&fixture(name), &test_root());
    assert_eq!(found.len(), 1, "{name}: {found:#?}");
    found.remove(0)
}

#[test]
fn an_intact_rsa_signature_names_its_signer_and_chains_to_the_trusted_root() {
    let signature = only("signed-rsa.pdf");
    assert_eq!(signature.field, "Signature1");
    assert_eq!(signature.signer, "Test Signer RSA");
    assert_eq!(
        signature.organization.as_deref(),
        Some("Butter Paper Tests")
    );
    assert_eq!(signature.reason.as_deref(), Some("Approved"));
    assert_eq!(signature.location.as_deref(), Some("Melbourne"));
    assert_eq!(signature.integrity, Integrity::Intact);
    assert_eq!(signature.changes, Changes::None);
    assert_eq!(
        signature.certificate.chain,
        ["Test Signer RSA", "Butter Paper Test Root"]
    );
    assert_eq!(signature.certificate.trust, Trust::Trusted);
    assert!(signature.certificate.in_date);
    assert!(signature.claimed_time.is_some());
    assert_eq!(signature.timestamp, None);
    assert_eq!(signature.page, None, "an invisible signature has no page");
}

#[test]
fn a_smart_card_signature_reads_intact_and_a_self_signed_one_is_not_called_unrevoked() {
    use butter_paper::pdf_signatures::Revocation;
    let signature = only("signed-by-a-smart-card.pdf");
    assert_eq!(signature.signer, "Smart Card Signer");
    assert_eq!(signature.integrity, Integrity::Intact);
    assert_eq!(signature.changes, Changes::None);
    assert_eq!(signature.certificate.trust, Trust::Untrusted);
    // Nothing issued it, so nothing can say it was not revoked.
    assert_eq!(signature.certificate.revocation, Revocation::NotChecked);
}

#[test]
fn without_its_root_trusted_a_signature_is_intact_but_untrusted() {
    let found = signatures(&fixture("signed-rsa.pdf"), &[]);
    assert_eq!(found[0].integrity, Integrity::Intact);
    assert_eq!(found[0].certificate.trust, Trust::Untrusted);
}

#[test]
fn an_ecdsa_signature_with_a_visible_box_is_on_its_page() {
    let signature = only("signed-ec-visible.pdf");
    assert_eq!(signature.signer, "Test Signer EC");
    assert_eq!(signature.integrity, Integrity::Intact);
    assert_eq!(signature.certificate.trust, Trust::Trusted);
    assert_eq!(signature.page, Some(0));
}

#[test]
fn a_changed_signed_byte_is_reported_as_altered() {
    let signature = only("signed-tampered.pdf");
    assert_eq!(signature.integrity, Integrity::Altered);
}

#[test]
fn a_timestamp_is_checked_and_dates_the_signature() {
    let signature = only("signed-timestamped.pdf");
    assert_eq!(signature.integrity, Integrity::Intact);
    let timestamp = signature.timestamp.expect("a timestamp");
    assert!(timestamp.valid);
    assert_eq!(timestamp.authority, "Test Timestamp Authority");
    // pyHanko's claimed time and its timestamp agree to within a minute.
    let claimed = signature.claimed_time.expect("a claimed time");
    assert!(
        (timestamp.time - claimed).abs() < 60,
        "{timestamp:?} {claimed}"
    );
}

#[test]
fn a_note_added_after_signing_is_a_permitted_change() {
    let signature = only("signed-then-noted.pdf");
    assert_eq!(signature.integrity, Integrity::Intact);
    assert_eq!(
        signature.changes,
        Changes::Permitted {
            signatures: false,
            other: true
        }
    );
}

#[test]
fn content_drawn_after_signing_is_a_content_change() {
    let signature = only("signed-then-changed.pdf");
    assert_eq!(signature.integrity, Integrity::Intact);
    assert_eq!(signature.changes, Changes::Content);
}

#[test]
fn a_second_signature_leaves_the_first_intact_with_a_permitted_change() {
    let found = signatures(&fixture("signed-twice.pdf"), &test_root());
    assert_eq!(found.len(), 2, "{found:#?}");
    let first = found.iter().find(|s| s.field == "Signature1").unwrap();
    let second = found.iter().find(|s| s.field == "Signature2").unwrap();
    assert_eq!(first.integrity, Integrity::Intact);
    assert_eq!(
        first.changes,
        Changes::Permitted {
            signatures: true,
            other: false
        }
    );
    assert_eq!(second.signer, "Test Signer EC");
    assert_eq!(second.integrity, Integrity::Intact);
    assert_eq!(second.changes, Changes::None);
}

#[test]
fn unsigned_and_unreadable_pdfs_have_no_signatures() {
    let unsigned = std::fs::read(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/text-edit/word-table.pdf"),
    )
    .unwrap();
    assert!(signatures(&unsigned, &test_root()).is_empty());
    assert!(signatures(b"not a pdf", &test_root()).is_empty());
}

/// Real signed PDFs in `BP_SIGNED_PDF_DIR`, checked against this computer's
/// trusted roots. Prints one anonymous line per signature (no names, and
/// paths only with `BP_SIGNED_PDF_SHOW_NAMES`) and fails on none; it is for reading the results. On macOS it
/// skips online-only cloud files rather than downloading them.
#[test]
#[ignore = "needs BP_SIGNED_PDF_DIR, a local folder of signed PDFs"]
fn real_signed_pdfs() {
    use butter_paper::pdf_signatures::system_trusted_roots;
    let root = PathBuf::from(std::env::var_os("BP_SIGNED_PDF_DIR").expect("BP_SIGNED_PDF_DIR"));
    let mut pending = vec![root];
    let mut files = Vec::new();
    while let Some(directory) = pending.pop() {
        let Ok(entries) = std::fs::read_dir(&directory) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(metadata) = std::fs::symlink_metadata(&path) else {
                continue;
            };
            if metadata.is_dir() {
                pending.push(path);
                continue;
            }
            #[cfg(target_os = "macos")]
            {
                use std::os::macos::fs::MetadataExt as _;
                if metadata.st_flags() & 0x4000_0000 != 0 {
                    continue;
                }
            }
            let pdf = path
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("pdf"));
            if pdf && metadata.len() < 40 * 1024 * 1024 {
                files.push(path);
            }
        }
    }
    // BP_SIGNED_PDF_TRUST: certificate files (PEM or DER) to trust instead.
    let trusted: Vec<Vec<u8>> = std::env::var_os("BP_SIGNED_PDF_TRUST")
        .map(|paths| {
            std::env::split_paths(&paths)
                .map(|path| {
                    let bytes = std::fs::read(&path).expect("a trusted certificate");
                    let text = String::from_utf8_lossy(&bytes);
                    match text.split_once("-----BEGIN CERTIFICATE-----") {
                        Some((_, rest)) => {
                            use base64::Engine as _;
                            let body: String = rest
                                .split("-----END")
                                .next()
                                .unwrap()
                                .split_whitespace()
                                .collect();
                            base64::engine::general_purpose::STANDARD
                                .decode(body)
                                .unwrap()
                        }
                        None => bytes,
                    }
                })
                .collect()
        })
        .unwrap_or_else(|| match std::env::var_os("BP_TRUST_LIST_DIR") {
            // Kept Adobe trust lists (`cargo test ... adobe_trust_lists`).
            Some(directory) => {
                let store =
                    butter_paper::signature_trust::TrustStore::new(PathBuf::from(directory));
                store.refresh().expect("Adobe's trust lists");
                store.roots()
            }
            None => system_trusted_roots().to_vec(),
        });
    let mut signed = 0;
    for path in files {
        let Ok(bytes) = std::fs::read(&path) else {
            continue;
        };
        if !bytes.windows(10).any(|window| window == b"/ByteRange") {
            continue;
        }
        signed += 1;
        let started = std::time::Instant::now();
        let found = signatures(&bytes, &trusted);
        if std::env::var_os("BP_SIGNED_PDF_SHOW_NAMES").is_some() {
            println!("file {signed:>3} is {}", path.display());
        }
        for signature in &found {
            if std::env::var_os("BP_SIGNED_PDF_SHOW_NAMES").is_some() {
                println!("file {signed:>3} chain {:?}", signature.certificate.chain);
            }
            println!(
                "file {signed:>3}: {:?} | {:?} | {:?} | in date {} | chain {} | {:?} | timestamp {:?} | doc-ts {} | {} ms",
                signature.integrity,
                signature.changes,
                signature.certificate.trust,
                signature.certificate.in_date,
                signature.certificate.chain.len(),
                signature.certificate.revocation,
                signature
                    .timestamp
                    .as_ref()
                    .map(|timestamp| (timestamp.valid, timestamp.trusted)),
                signature.document_timestamp,
                started.elapsed().as_millis()
            );
        }
        if found.is_empty() {
            println!("file {signed:>3}: /ByteRange but no signature fields read");
        }
    }
    println!("{signed} signed PDFs");
}

/// The earlier signing project's corpus, checked at its validation time
/// against its root (see the fixtures README for pyHanko's verdicts).
mod corpus {
    use super::*;
    use butter_paper::pdf_signatures::signatures_at;

    /// 2026-08-05T05:00:00Z.
    const VALIDATED_AT: i64 = 1_785_906_000;

    fn root() -> Vec<Vec<u8>> {
        use base64::Engine as _;
        let pem = String::from_utf8(fixture("corpus/root.pem")).unwrap();
        let body: String = pem
            .lines()
            .filter(|line| !line.starts_with("-----"))
            .collect();
        vec![
            base64::engine::general_purpose::STANDARD
                .decode(body)
                .unwrap(),
        ]
    }

    fn checked(case: &str) -> Vec<SignatureReport> {
        signatures_at(
            &fixture(&format!("corpus/{case}.pdf")),
            &root(),
            VALIDATED_AT,
        )
    }

    #[test]
    fn out_of_date_certificates_are_not_in_date_without_a_timestamp() {
        for case in ["expired-now", "not-yet-valid"] {
            let found = checked(case);
            assert_eq!(found.len(), 1, "{case}");
            assert_eq!(found[0].integrity, Integrity::Intact, "{case}");
            assert_eq!(found[0].certificate.trust, Trust::Trusted, "{case}");
            assert!(!found[0].certificate.in_date, "{case}");
        }
    }

    #[test]
    fn a_chain_missing_its_intermediate_is_untrusted() {
        let found = checked("missing-intermediate");
        assert_eq!(found[0].integrity, Integrity::Intact);
        assert_eq!(found[0].certificate.trust, Trust::Untrusted);
    }

    #[test]
    fn a_malformed_byte_range_is_altered() {
        assert_eq!(
            checked("malformed-byte-range")[0].integrity,
            Integrity::Altered
        );
    }

    #[test]
    fn a_field_named_like_the_signed_one_is_a_disallowed_change() {
        let found = checked("duplicate-signature-field");
        assert_eq!(found[0].integrity, Integrity::Intact);
        assert_eq!(found[0].changes, Changes::Content);
    }

    #[test]
    fn later_validation_data_counts_as_signature_data() {
        let found = checked("revoked-with-embedded-evidence");
        assert_eq!(found[0].integrity, Integrity::Intact);
        assert_eq!(
            found[0].changes,
            Changes::Permitted {
                signatures: true,
                other: false
            }
        );
    }

    #[test]
    fn a_broken_last_revision_leaves_the_earlier_signature_intact() {
        let found = checked("corrupt-incremental-revision");
        let intact: Vec<_> = found
            .iter()
            .filter(|signature| signature.integrity == Integrity::Intact)
            .collect();
        assert_eq!(intact.len(), 1, "{found:#?}");
        assert!(
            found
                .iter()
                .any(|signature| signature.integrity == Integrity::Altered),
            "the broken revision's signature is reported"
        );
    }

    #[test]
    fn revocation_after_a_timestamped_signature_leaves_it_good() {
        use butter_paper::pdf_signatures::Revocation;
        // Signed and timestamped 2024-01-15; its CRL and OCSP evidence
        // revoke the signer on 2026-08-05, after it signed.
        let found = checked("revoked-with-embedded-evidence");
        assert!(
            found[0]
                .timestamp
                .as_ref()
                .is_some_and(|timestamp| timestamp.valid)
        );
        assert_eq!(found[0].certificate.revocation, Revocation::Good);
    }

    #[test]
    fn two_signatures_are_both_intact() {
        let found = checked("multiple-signatures");
        assert_eq!(found.len(), 2);
        assert!(
            found
                .iter()
                .all(|signature| signature.integrity == Integrity::Intact)
        );
        assert!(
            found
                .iter()
                .all(|signature| signature.certificate.trust == Trust::Trusted)
        );
    }
}

#[test]
fn verdicts_rank_invalid_over_unverified_over_valid() {
    use butter_paper::pdf_signatures::{Verdict, document_verdict};
    let roots = test_root();
    let verdict = |name: &str| document_verdict(&signatures(&fixture(name), &roots));
    assert_eq!(verdict("signed-rsa.pdf"), Some(Verdict::Valid));
    assert_eq!(verdict("signed-then-noted.pdf"), Some(Verdict::Valid));
    assert_eq!(verdict("signed-then-changed.pdf"), Some(Verdict::Invalid));
    assert_eq!(verdict("signed-tampered.pdf"), Some(Verdict::Invalid));
    assert_eq!(
        document_verdict(&signatures(&fixture("signed-rsa.pdf"), &[])),
        Some(Verdict::Unverified)
    );
    assert_eq!(document_verdict(&[]), None);
}

#[test]
fn a_real_acrobat_style_ber_signature_reads_and_is_intact() {
    // Public Law 118-1 from govinfo.gov, signed by the US Government
    // Publishing Office in BER (indefinite lengths), as Acrobat writes.
    let found = signatures(&fixture("gpo-public-law-118-1.pdf"), &[]);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].integrity, Integrity::Intact);
    assert_eq!(found[0].changes, Changes::None);
    assert_eq!(
        found[0].certificate.chain,
        [
            "U.S. Government Publishing Office",
            "DigiCert Document Signing CA",
            "DigiCert Assured ID Root CA"
        ]
    );
    // DigiCert's document-signing root is in Adobe's list, not in every
    // system store: trusted only with the root given.
    assert_eq!(found[0].certificate.trust, Trust::Untrusted);
}

/// Revocation, answered by the fixtures' OCSP responses and CRL at the
/// addresses the test certificates name.
mod revocation {
    use super::*;
    use butter_paper::pdf_signatures::{Revocation, Verdict, signatures_checked};

    const OCSP_URL: &str = "http://ocsp.butter-paper.test";
    const CRL_URL: &str = "http://crl.butter-paper.test/root.crl";

    fn now() -> i64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64
    }

    fn checked(name: &str, ocsp: Option<&str>, crl: bool) -> SignatureReport {
        let fetch = |url: &str, body: Option<&[u8]>| -> Option<Vec<u8>> {
            match (url, body) {
                (OCSP_URL, Some(_)) => ocsp.map(fixture),
                (CRL_URL, None) if crl => Some(fixture("test-root.crl")),
                _ => None,
            }
        };
        let mut found = signatures_checked(&fixture(name), &test_root(), now(), Some(&fetch));
        assert_eq!(found.len(), 1);
        found.remove(0)
    }

    #[test]
    fn an_ocsp_revoked_signer_makes_the_signature_invalid() {
        let signature = checked("signed-revoked.pdf", Some("ocsp-revoked.der"), false);
        assert!(matches!(
            signature.certificate.revocation,
            Revocation::Revoked { .. }
        ));
        assert_eq!(signature.integrity, Integrity::Intact);
        assert_eq!(signature.verdict(), Verdict::Invalid);
    }

    #[test]
    fn a_good_ocsp_answer_leaves_the_signature_valid() {
        let signature = checked("signed-rsa.pdf", Some("ocsp-rsa-good.der"), false);
        assert_eq!(signature.certificate.revocation, Revocation::Good);
        assert_eq!(signature.verdict(), Verdict::Valid);
    }

    #[test]
    fn the_revocation_list_answers_when_ocsp_does_not() {
        let revoked = checked("signed-revoked.pdf", None, true);
        assert!(matches!(
            revoked.certificate.revocation,
            Revocation::Revoked { .. }
        ));
        let good = checked("signed-rsa.pdf", None, true);
        assert_eq!(good.certificate.revocation, Revocation::Good);
    }

    #[test]
    fn an_answer_for_another_certificate_is_not_taken() {
        // The revoked signer's OCSP answer does not speak for the RSA signer.
        let signature = checked("signed-rsa.pdf", Some("ocsp-revoked.der"), false);
        assert_eq!(signature.certificate.revocation, Revocation::NotChecked);
        assert_eq!(signature.verdict(), Verdict::Valid);
    }

    #[test]
    fn without_lookups_revocation_is_not_checked() {
        let found = signatures(&fixture("signed-revoked.pdf"), &test_root());
        assert_eq!(found[0].certificate.revocation, Revocation::NotChecked);
    }
}
