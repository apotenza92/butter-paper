//! Signing PDFs with the test identities in `tests/fixtures/signatures`,
//! read back by Butter Paper and, where installed, by Poppler's `pdfsig`.

use std::path::PathBuf;

use lopdf::dictionary;

use butter_paper::annotation_model::DecodedRgbaAsset;
use butter_paper::pdf_signatures::{Changes, Integrity, Trust, signatures};
use butter_paper::pdf_signing::{
    Network, SignOptions, SignatureAppearance, SigningError, SigningIdentity, sign_pdf,
    sign_pdf_with,
};

const PASSWORD: &str = "butter-paper-test";

fn fixture(name: &str) -> Vec<u8> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name);
    std::fs::read(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

fn identity(name: &str) -> SigningIdentity {
    SigningIdentity::from_pkcs12(&fixture(&format!("signatures/{name}")), PASSWORD).unwrap()
}

fn test_root() -> Vec<Vec<u8>> {
    vec![fixture("signatures/test-root.cer")]
}

/// `pdfsig`'s verdict on each signature, when it is installed.
fn pdfsig(pdf: &[u8], name: &str) -> Option<Vec<String>> {
    let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/test-scratch/signing");
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join(format!("{name}-{}.pdf", std::process::id()));
    std::fs::write(&path, pdf).unwrap();
    let output = std::process::Command::new("pdfsig")
        .arg(&path)
        .output()
        .ok()?;
    let _ = std::fs::remove_file(&path);
    Some(
        String::from_utf8_lossy(&output.stdout)
            .lines()
            .filter_map(|line| line.trim().strip_prefix("- Signature Validation: "))
            .map(str::to_owned)
            .collect(),
    )
}

#[test]
fn an_rsa_signature_reads_back_intact_and_trusted() {
    let pdf = fixture("text-edit/word-table.pdf");
    let signer = identity("test-signer-rsa.p12");
    assert_eq!(signer.signer_name(), "Test Signer RSA");
    let signed = sign_pdf(
        &pdf,
        &signer,
        &SignOptions {
            reason: Some("Approved".into()),
            location: Some("Melbourne".into()),
            ..SignOptions::default()
        },
    )
    .unwrap();
    assert!(signed.starts_with(&pdf), "the original bytes are untouched");
    let found = signatures(&signed, &test_root());
    assert_eq!(found.len(), 1);
    let signature = &found[0];
    assert_eq!(signature.field, "Signature1");
    assert_eq!(signature.signer, "Test Signer RSA");
    assert_eq!(signature.reason.as_deref(), Some("Approved"));
    assert_eq!(signature.location.as_deref(), Some("Melbourne"));
    assert_eq!(signature.integrity, Integrity::Intact);
    assert_eq!(signature.changes, Changes::None);
    assert_eq!(signature.certificate.trust, Trust::Trusted);
    assert!(signature.certificate.in_date);
    assert_eq!(signature.page, None);
    if let Some(verdicts) = pdfsig(&signed, "rsa") {
        assert_eq!(verdicts, ["Signature is Valid."]);
    }
}

#[test]
fn a_visible_ecdsa_signature_draws_its_image_on_the_page() {
    let pdf = fixture("text-edit/word-table.pdf");
    // A 4 x 2 dark stroke on transparency.
    let mut rgba = vec![0; 4 * 2 * 4];
    for pixel in rgba.chunks_exact_mut(4).take(4) {
        pixel.copy_from_slice(&[17, 24, 39, 255]);
    }
    let image = DecodedRgbaAsset::new(4, 2, rgba).unwrap();
    let signed = sign_pdf(
        &pdf,
        &identity("test-signer-ec.p12"),
        &SignOptions {
            appearance: Some(SignatureAppearance {
                page: 0,
                rect: [300., 60., 540., 120.],
                image: Some(image),
                ..SignatureAppearance::default()
            }),
            ..SignOptions::default()
        },
    )
    .unwrap();
    let found = signatures(&signed, &test_root());
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].signer, "Test Signer EC");
    assert_eq!(found[0].integrity, Integrity::Intact);
    assert_eq!(found[0].page, Some(0));
    if let Some(verdicts) = pdfsig(&signed, "ec") {
        assert_eq!(verdicts, ["Signature is Valid."]);
    }
}

#[test]
fn signing_a_signed_pdf_keeps_the_first_signature_intact() {
    let signed = sign_pdf(
        &fixture("signatures/signed-rsa.pdf"),
        &identity("test-signer-ec.p12"),
        &SignOptions::default(),
    )
    .unwrap();
    let found = signatures(&signed, &test_root());
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
    assert_eq!(second.integrity, Integrity::Intact);
    assert_eq!(second.changes, Changes::None);
    if let Some(verdicts) = pdfsig(&signed, "twice") {
        assert_eq!(verdicts, ["Signature is Valid.", "Signature is Valid."]);
    }
}

/// `pdf` encrypted as a PDF with only an owner password is: it opens
/// without one.
fn owner_password_only(pdf: &[u8], aes: bool) -> Vec<u8> {
    let mut document = lopdf::Document::load_mem(pdf).unwrap();
    let file_id = lopdf::Object::String(
        b"butter-paper-sign".to_vec(),
        lopdf::StringFormat::Hexadecimal,
    );
    document.trailer.set("ID", vec![file_id.clone(), file_id]);
    let permissions = lopdf::Permissions::PRINTABLE | lopdf::Permissions::COPYABLE;
    let version = if aes {
        lopdf::EncryptionVersion::V4 {
            document: &document,
            encrypt_metadata: true,
            crypt_filters: std::collections::BTreeMap::from([(
                b"StdCF".to_vec(),
                std::sync::Arc::new(lopdf::encryption::crypt_filters::Aes128CryptFilter)
                    as std::sync::Arc<dyn lopdf::encryption::crypt_filters::CryptFilter>,
            )]),
            stream_filter: b"StdCF".to_vec(),
            string_filter: b"StdCF".to_vec(),
            owner_password: "owner",
            user_password: "",
            permissions,
        }
    } else {
        lopdf::EncryptionVersion::V2 {
            document: &document,
            owner_password: "owner",
            user_password: "",
            key_length: 128,
            permissions,
        }
    };
    let state = lopdf::EncryptionState::try_from(version).unwrap();
    document.encrypt(&state).unwrap();
    let mut bytes = Vec::new();
    document.save_to(&mut bytes).unwrap();
    bytes
}

#[test]
fn a_pdf_with_only_an_owner_password_is_signed_and_stays_encrypted() {
    for aes in [false, true] {
        let pdf = owner_password_only(&fixture("text-edit/word-table.pdf"), aes);
        let signed = sign_pdf(
            &pdf,
            &identity("test-signer-rsa.p12"),
            &SignOptions {
                reason: Some("Encrypted approval".into()),
                appearance: Some(SignatureAppearance {
                    page: 0,
                    rect: [300., 60., 540., 120.],
                    ..SignatureAppearance::default()
                }),
                ..SignOptions::default()
            },
        )
        .unwrap();
        assert!(signed.starts_with(&pdf));
        let update = &signed[pdf.len()..];
        assert!(
            !update
                .windows(b"Encrypted approval".len())
                .any(|window| window == b"Encrypted approval"),
            "the update's strings are encrypted (aes: {aes})"
        );
        let reopened = lopdf::Document::load_mem(&signed).unwrap();
        assert!(reopened.was_encrypted(), "it still has its password");
        let found = signatures(&signed, &test_root());
        assert_eq!(found.len(), 1, "aes: {aes}");
        assert_eq!(found[0].integrity, Integrity::Intact, "aes: {aes}");
        assert_eq!(found[0].changes, Changes::None, "aes: {aes}");
        assert_eq!(found[0].reason.as_deref(), Some("Encrypted approval"));
        assert_eq!(found[0].page, Some(0));
        if let Some(verdicts) = pdfsig(&signed, if aes { "aes" } else { "rc4" }) {
            assert_eq!(verdicts, ["Signature is Valid."], "aes: {aes}");
        }
    }
}

#[test]
fn a_signature_in_place_of_a_markup_leaves_it_out_of_the_signed_copy() {
    // A stamp named as Butter Paper names markups.
    let mut document = lopdf::Document::load_mem(&fixture("text-edit/word-table.pdf")).unwrap();
    let page_id = *document.get_pages().get(&1).unwrap();
    let stamp = document.add_object(dictionary! {
        "Type" => "Annot",
        "Subtype" => "Stamp",
        "Rect" => vec![300.into(), 60.into(), 540.into(), 120.into()],
        "NM" => lopdf::Object::string_literal("HANDWRITTENSIGNA"),
    });
    let other = document.add_object(dictionary! {
        "Type" => "Annot",
        "Subtype" => "Square",
        "Rect" => vec![10.into(), 10.into(), 40.into(), 40.into()],
        "NM" => lopdf::Object::string_literal("SOMEOTHERMARKUPS"),
    });
    document
        .get_object_mut(page_id)
        .and_then(lopdf::Object::as_dict_mut)
        .unwrap()
        .set("Annots", vec![stamp.into(), other.into()]);
    let mut pdf = Vec::new();
    document.save_to(&mut pdf).unwrap();

    let signed = sign_pdf(
        &pdf,
        &identity("test-signer-rsa.p12"),
        &SignOptions {
            appearance: Some(SignatureAppearance {
                page: 0,
                rect: [300., 60., 540., 120.],
                replaces: Some("HANDWRITTENSIGNA".into()),
                ..SignatureAppearance::default()
            }),
            ..SignOptions::default()
        },
    )
    .unwrap();
    let reopened = lopdf::Document::load_mem(&signed).unwrap();
    let page = reopened.get_dictionary(page_id).unwrap();
    let annotations = page.get(b"Annots").unwrap().as_array().unwrap();
    assert!(!annotations.contains(&stamp.into()), "the stamp is gone");
    assert!(annotations.contains(&other.into()), "other markups stay");
    assert_eq!(annotations.len(), 2, "the other markup and the signature");
    let found = signatures(&signed, &test_root());
    assert_eq!(found[0].integrity, Integrity::Intact);
    assert_eq!(found[0].changes, Changes::None);
}

#[test]
fn a_pdf_certified_to_allow_no_changes_is_not_signed_again() {
    let certified = fixture("signatures/corpus/certified-no-changes.pdf");
    let refused = sign_pdf(
        &certified,
        &identity("test-signer-rsa.p12"),
        &SignOptions::default(),
    );
    assert!(
        matches!(&refused, Err(SigningError::Document(reason)) if reason.contains("certified")),
        "{refused:?}"
    );
}

#[test]
fn legacy_encrypted_certificate_files_open() {
    for name in ["test-signer-rsa-legacy.p12", "test-signer-ec-legacy.p12"] {
        let signer = identity(name);
        let signed = sign_pdf(
            &fixture("text-edit/word-table.pdf"),
            &signer,
            &SignOptions::default(),
        )
        .unwrap();
        assert_eq!(
            signatures(&signed, &test_root())[0].integrity,
            Integrity::Intact
        );
    }
}

#[test]
fn a_wrong_password_or_a_refusing_timestamp_server_says_so() {
    let p12 = fixture("signatures/test-signer-rsa.p12");
    assert!(matches!(
        SigningIdentity::from_pkcs12(&p12, "not the password"),
        Err(SigningError::WrongPassword)
    ));
    assert!(SigningIdentity::from_pkcs12(b"not a certificate", PASSWORD).is_err());
    let refused = sign_pdf_with(
        &fixture("text-edit/word-table.pdf"),
        &identity("test-signer-rsa.p12"),
        &SignOptions {
            timestamp_server: Some("https://timestamp.invalid".into()),
            ..SignOptions::default()
        },
        &Network {
            // TimeStampResp { status { rejection (2) } }
            timestamp: &|_, _| Ok(vec![0x30, 0x05, 0x30, 0x03, 0x02, 0x01, 0x02]),
            revocation: &|_, _| None,
        },
    );
    assert!(matches!(refused, Err(SigningError::Timestamp(_))));
}

#[test]
#[ignore = "asks DigiCert's public timestamp server"]
fn a_signature_timestamped_by_a_public_server_checks_out() {
    let signed = sign_pdf(
        &fixture("text-edit/word-table.pdf"),
        &identity("test-signer-rsa.p12"),
        &SignOptions {
            timestamp_server: Some("http://timestamp.digicert.com".into()),
            ..SignOptions::default()
        },
    )
    .unwrap();
    // DigiCert's authority chains to a root this computer trusts.
    let mut roots = test_root();
    roots.extend(
        butter_paper::pdf_signatures::system_trusted_roots()
            .iter()
            .cloned(),
    );
    let found = signatures(&signed, &roots);
    let timestamp = found[0].timestamp.clone().expect("a timestamp");
    assert!(timestamp.valid, "{timestamp:?}");
    assert!(timestamp.trusted, "{timestamp:?}");
    assert!(timestamp.authority.contains("DigiCert"), "{timestamp:?}");
    if let Some(verdicts) = pdfsig(&signed, "timestamped") {
        assert_eq!(verdicts, ["Signature is Valid."]);
    }
}

/// Writes signed samples to `target/test-scratch/signing/samples` for
/// checking in other validators (pyHanko, Acrobat).
#[test]
#[ignore = "writes samples for manual validation; the timestamped one asks DigiCert"]
fn write_signed_samples_for_manual_review() {
    let directory =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/test-scratch/signing/samples");
    std::fs::create_dir_all(&directory).unwrap();
    let pdf = fixture("text-edit/word-table.pdf");
    let samples = [
        ("rsa.pdf", "test-signer-rsa.p12", None),
        ("ec.pdf", "test-signer-ec.p12", None),
        (
            "rsa-timestamped.pdf",
            "test-signer-rsa.p12",
            Some("http://timestamp.digicert.com".to_owned()),
        ),
    ];
    // A stroke drawn on white, cleaned up as an imported signature is.
    let (width, height) = (400_u32, 120_u32);
    let mut page = vec![255_u8; (width * height * 4) as usize];
    for x in 20..380_u32 {
        let y = (60. + 35. * ((x as f64) / 40.).sin()) as u32;
        for dy in 0..5 {
            let ix = (((y + dy) * width + x) * 4) as usize;
            page[ix..ix + 4].copy_from_slice(&[140, 40, 20, 255]);
        }
    }
    let stroke = butter_paper::image_asset_decode::sanitize_signature_raster(width, height, &page)
        .unwrap()
        .into_asset();
    let with_image = sign_pdf(
        &pdf,
        &identity("test-signer-ec.p12"),
        &SignOptions {
            appearance: Some(SignatureAppearance {
                page: 0,
                rect: [300., 40., 560., 110.],
                image: Some(stroke),
                ..SignatureAppearance::default()
            }),
            ..SignOptions::default()
        },
    )
    .unwrap();
    std::fs::write(directory.join("ec-with-image.pdf"), with_image).unwrap();
    // Owner password only: RC4 and AES, both opening without a password.
    for (name, aes) in [
        ("rsa-encrypted-rc4.pdf", false),
        ("rsa-encrypted-aes.pdf", true),
    ] {
        let encrypted = owner_password_only(&pdf, aes);
        let signed = sign_pdf(
            &encrypted,
            &identity("test-signer-rsa.p12"),
            &SignOptions {
                reason: Some("Review".into()),
                appearance: Some(SignatureAppearance {
                    page: 0,
                    rect: [300., 60., 540., 120.],
                    details_only: true,
                    ..SignatureAppearance::default()
                }),
                ..SignOptions::default()
            },
        )
        .unwrap();
        std::fs::write(directory.join(name), signed).unwrap();
    }
    // Long-term: the test signer's OCSP answer from the fixture, DigiCert's
    // live.
    let ocsp = fixture("signatures/ocsp-rsa-good.der");
    let long_term = sign_pdf_with(
        &pdf,
        &identity("test-signer-rsa.p12"),
        &SignOptions {
            timestamp_server: Some("http://timestamp.digicert.com".into()),
            long_term: true,
            ..SignOptions::default()
        },
        &Network {
            timestamp: &|server, query| {
                let client = reqwest::blocking::Client::new();
                client
                    .post(server)
                    .header("Content-Type", "application/timestamp-query")
                    .body(query.to_vec())
                    .send()
                    .and_then(|response| response.bytes())
                    .map(|bytes| bytes.to_vec())
                    .map_err(|error| error.to_string())
            },
            revocation: &|url, request| {
                if url == "http://ocsp.butter-paper.test" {
                    return Some(ocsp.clone());
                }
                butter_paper::signature_trust::fetch_revocation(url, request)
            },
        },
    )
    .unwrap();
    std::fs::write(directory.join("rsa-long-term.pdf"), long_term).unwrap();
    for (name, p12, timestamp_server) in samples {
        let signed = sign_pdf(
            &pdf,
            &identity(p12),
            &SignOptions {
                reason: Some("Review".into()),
                long_term: timestamp_server.is_some(),
                timestamp_server,
                appearance: Some(SignatureAppearance {
                    page: 0,
                    rect: [300., 60., 540., 120.],
                    ..SignatureAppearance::default()
                }),
                ..SignOptions::default()
            },
        )
        .unwrap();
        std::fs::write(directory.join(name), signed).unwrap();
    }
}

#[test]
fn long_term_signing_keeps_the_revocation_answers_in_the_pdf() {
    use butter_paper::pdf_signatures::Revocation;
    let ocsp = fixture("signatures/ocsp-rsa-good.der");
    let signed = sign_pdf_with(
        &fixture("text-edit/word-table.pdf"),
        &identity("test-signer-rsa.p12"),
        &SignOptions {
            long_term: true,
            ..SignOptions::default()
        },
        &Network {
            timestamp: &|_, _| Err("no timestamp in this test".into()),
            revocation: &|url, request| {
                (url == "http://ocsp.butter-paper.test" && request.is_some()).then(|| ocsp.clone())
            },
        },
    )
    .unwrap();
    // Read with no network: the answer is in the PDF.
    let found = signatures(&signed, &test_root());
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].integrity, Integrity::Intact);
    assert_eq!(
        found[0].changes,
        Changes::Permitted {
            signatures: true,
            other: false
        }
    );
    assert_eq!(found[0].certificate.revocation, Revocation::Good);
    if let Some(verdicts) = pdfsig(&signed, "long-term") {
        assert_eq!(verdicts, ["Signature is Valid."]);
    }
}

/// Signs with the test identities through the system's certificate store:
/// on macOS a throwaway keychain (`BP_TEST_KEYCHAIN`, unlocked with
/// `BP_TEST_KEYCHAIN_PASSWORD`) the test `.p12` files were imported into;
/// on Windows the personal store, after `certutil -user -importpfx` of
/// them.
#[test]
#[ignore = "needs the test identities in a system certificate store"]
fn system_store_identities_sign() {
    use butter_paper::system_certificates::SystemIdentity;
    #[cfg(target_os = "macos")]
    let identities: Vec<SystemIdentity> = butter_paper::system_certificates::keychain_identities(
        std::path::Path::new(&std::env::var("BP_TEST_KEYCHAIN").expect("BP_TEST_KEYCHAIN")),
        &std::env::var("BP_TEST_KEYCHAIN_PASSWORD").expect("BP_TEST_KEYCHAIN_PASSWORD"),
    );
    #[cfg(not(target_os = "macos"))]
    let identities: Vec<SystemIdentity> = butter_paper::system_certificates::system_identities();
    let names: Vec<String> = identities
        .iter()
        .map(|identity| identity.name.clone())
        .collect();
    let mut signed_any = 0;
    for identity in identities {
        if !identity.name.starts_with("Test Signer") || identity.name.contains("Revoked") {
            continue;
        }
        let name = identity.name.clone();
        assert_eq!(identity.issuer, "Butter Paper Test Root", "{name}");
        let signed = sign_pdf(
            &fixture("text-edit/word-table.pdf"),
            &SigningIdentity::from_system(identity),
            &SignOptions::default(),
        )
        .unwrap_or_else(|error| panic!("{name}: {error}"));
        let found = signatures(&signed, &test_root());
        assert_eq!(found[0].signer, name);
        assert_eq!(found[0].integrity, Integrity::Intact, "{name}");
        assert_eq!(found[0].certificate.trust, Trust::Trusted, "{name}");
        signed_any += 1;
    }
    assert!(signed_any >= 2, "the RSA and EC test signers: {names:?}");
}
