//! Attacks a signature checker must not fall for: each makes a signed PDF
//! look validly signed though it was changed, back-dated or signed by an
//! impostor.

use std::io::Write as _;
use std::path::PathBuf;

use butter_paper::pdf_signatures::{
    Changes, Integrity, Revocation, Trust, signatures, signatures_at,
};
use butter_paper::pdf_signing::{Network, SignOptions, SigningIdentity, sign_pdf, sign_pdf_with};
use lopdf::{Document, Object, Stream, dictionary};

fn fixture(name: &str) -> Vec<u8> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/signatures")
        .join(name);
    std::fs::read(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

fn test_root() -> Vec<Vec<u8>> {
    vec![fixture("test-root.cer")]
}

fn signer() -> SigningIdentity {
    SigningIdentity::from_pkcs12(&fixture("test-signer-rsa.p12"), "butter-paper-test").unwrap()
}

/// A one-page PDF drawing `text`, its content stream object, and the
/// stream's byte offset.
fn one_page(text: &str) -> (Vec<u8>, u32) {
    let mut document = Document::with_version("1.7");
    let pages_id = document.new_object_id();
    let font_id = document.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Helvetica",
    });
    let content_id = document.add_object(Stream::new(
        dictionary! {},
        format!("BT /F1 24 Tf 72 700 Td ({text}) Tj ET").into_bytes(),
    ));
    let page_id = document.add_object(dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "Contents" => content_id,
        "Resources" => dictionary! { "Font" => dictionary! { "F1" => font_id } },
    });
    document.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => vec![page_id.into()],
            "Count" => 1,
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
        }),
    );
    let catalog_id = document.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    document.trailer.set("Root", catalog_id);
    let mut bytes = Vec::new();
    document.save_to(&mut bytes).unwrap();
    (bytes, content_id.0)
}

/// `pdf` with an incremental update `change` makes to its objects.
fn append(pdf: &[u8], change: impl FnOnce(&Document, &mut Document)) -> Vec<u8> {
    let source = Document::load_mem(pdf).unwrap();
    let mut update = lopdf::IncrementalDocument::create_from(pdf.to_vec(), source.clone());
    change(&source, &mut update.new_document);
    let mut bytes = Vec::new();
    update.save_to(&mut bytes).unwrap();
    bytes
}

fn signed(pdf: &[u8]) -> Vec<u8> {
    sign_pdf(pdf, &signer(), &SignOptions::default()).unwrap()
}

fn changes(pdf: &[u8]) -> Changes {
    let found = signatures(pdf, &test_root());
    assert_eq!(found.len(), 1, "{found:#?}");
    assert_eq!(found[0].integrity, Integrity::Intact);
    found[0].changes.clone()
}

#[test]
fn pointing_a_page_back_at_its_unsigned_content_is_a_change() {
    // Drawn one way, then changed and signed.
    let (original, content) = one_page("ORIGINAL");
    let old_offset = original
        .windows(format!("{content} 0 obj").len())
        .position(|window| window == format!("{content} 0 obj").as_bytes())
        .unwrap();
    let edited = append(&original, |_, new| {
        new.objects.insert(
            (content, 0),
            Object::Stream(Stream::new(
                dictionary! {},
                b"BT /F1 24 Tf 72 700 Td (SIGNED VERSION) Tj ET".to_vec(),
            )),
        );
    });
    let mut pdf = signed(&edited);
    assert_eq!(changes(&pdf), Changes::None);

    // An update whose cross-reference points the content back at the
    // bytes before the signed revision: nothing new is written after the
    // signature but this table.
    let document = Document::load_mem(&pdf).unwrap();
    let root = document
        .trailer
        .get(b"Root")
        .unwrap()
        .as_reference()
        .unwrap();
    let start = pdf.len();
    write!(
        pdf,
        "xref\n0 1\n0000000000 65535 f \n{content} 1\n{old_offset:010} 00000 n \ntrailer\n\
         << /Size {} /Root {} 0 R /Prev {} >>\nstartxref\n{start}\n%%EOF\n",
        document.max_id + 1,
        root.0,
        document.xref_start
    )
    .unwrap();
    assert_eq!(changes(&pdf), Changes::Content);
}

#[test]
fn a_crop_box_the_pages_inherit_is_a_change() {
    let (original, _) = one_page("Pay 100");
    let pdf = append(&signed(&original), |source, new| {
        let pages_id = source
            .catalog()
            .unwrap()
            .get(b"Pages")
            .unwrap()
            .as_reference()
            .unwrap();
        let mut pages = source.get_dictionary(pages_id).unwrap().clone();
        pages.set(
            "CropBox",
            vec![0.into(), 400.into(), 612.into(), 792.into()],
        );
        new.objects.insert(pages_id, Object::Dictionary(pages));
    });
    assert_eq!(changes(&pdf), Changes::Content);
}

#[test]
fn an_open_action_added_to_the_catalog_is_a_change() {
    let (original, _) = one_page("Read me");
    let pdf = append(&signed(&original), |source, new| {
        let catalog_id = source.trailer.get(b"Root").unwrap().as_reference().unwrap();
        let mut catalog = source.get_dictionary(catalog_id).unwrap().clone();
        catalog.set(
            "OpenAction",
            dictionary! { "S" => "JavaScript", "JS" => Object::string_literal("app.alert(1)") },
        );
        new.objects.insert(catalog_id, Object::Dictionary(catalog));
    });
    assert_eq!(changes(&pdf), Changes::Content);
}

#[test]
fn a_note_added_after_signing_is_a_permitted_change() {
    let (original, _) = one_page("Note me");
    let pdf = append(&signed(&original), |source, new| {
        let page_id = *source.get_pages().get(&1).unwrap();
        let note_id = new.add_object(dictionary! {
            "Type" => "Annot",
            "Subtype" => "Text",
            "Rect" => vec![10.into(), 10.into(), 30.into(), 30.into()],
            "Contents" => Object::string_literal("Looks good"),
        });
        let mut page = source.get_dictionary(page_id).unwrap().clone();
        let mut annotations = match page.get(b"Annots") {
            Ok(Object::Array(list)) => list.clone(),
            _ => Vec::new(),
        };
        annotations.push(note_id.into());
        page.set("Annots", annotations);
        new.objects.insert(page_id, Object::Dictionary(page));
    });
    assert_eq!(
        changes(&pdf),
        Changes::Permitted {
            signatures: false,
            other: true
        }
    );
}

#[test]
fn a_certificate_issued_by_one_that_is_not_a_ca_is_not_trusted() {
    // "Jane Doe" at "Big Bank", issued by the RSA test signer's own
    // (end-entity) certificate under the trusted test root.
    let found = signatures(&fixture("signed-by-a-non-ca.pdf"), &test_root());
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].signer, "Jane Doe");
    assert_eq!(found[0].integrity, Integrity::Intact);
    assert_eq!(found[0].certificate.trust, Trust::Untrusted);
}

#[test]
fn only_a_trusted_timestamp_authority_dates_a_signature() {
    let pdf = fixture("signed-timestamped.pdf");
    let trusted = &signatures(&pdf, &test_root())[0];
    let timestamp = trusted.timestamp.as_ref().unwrap();
    assert!(timestamp.valid && timestamp.trusted);
    // An authority no trusted root vouches for proves nothing about when.
    let untrusted = &signatures(&pdf, &[])[0];
    let timestamp = untrusted.timestamp.as_ref().unwrap();
    assert!(timestamp.valid, "the token itself is intact");
    assert!(!timestamp.trusted);
}

#[test]
fn a_good_revocation_answer_counts_only_while_it_is_current() {
    // The signer's OCSP answer says "good" from October 2026 until 2046.
    let ocsp = fixture("ocsp-rsa-good.der");
    let (original, _) = one_page("Long term");
    let pdf = sign_pdf_with(
        &original,
        &signer(),
        &SignOptions {
            long_term: true,
            ..SignOptions::default()
        },
        &Network {
            timestamp: &|_, _| Err("no timestamp in this test".into()),
            revocation: &|_, request| request.is_some().then(|| ocsp.clone()),
        },
    )
    .unwrap();
    let year = 365 * 24 * 60 * 60;
    let in_2030 = 1_893_456_000;
    assert_eq!(
        signatures_at(&pdf, &test_root(), in_2030)[0]
            .certificate
            .revocation,
        Revocation::Good
    );
    // Long after it, the answer is stale and the status unknown.
    assert_eq!(
        signatures_at(&pdf, &test_root(), in_2030 + 17 * year)[0]
            .certificate
            .revocation,
        Revocation::NotChecked
    );
}

#[test]
fn a_decompression_bomb_in_the_validation_data_is_not_unpacked() {
    let (original, _) = one_page("Bomb");
    let mut zeros = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::best());
    let block = vec![0u8; 1 << 20];
    for _ in 0..128 {
        zeros.write_all(&block).unwrap();
    }
    let bomb = zeros.finish().unwrap();
    let pdf = append(&signed(&original), |source, new| {
        let mut stream = Stream::new(dictionary! { "Filter" => "FlateDecode" }, bomb);
        stream.allows_compression = false;
        let certificate = new.add_object(stream);
        let store = new.add_object(dictionary! { "Certs" => vec![certificate.into()] });
        let catalog_id = source.trailer.get(b"Root").unwrap().as_reference().unwrap();
        let mut catalog = source.get_dictionary(catalog_id).unwrap().clone();
        catalog.set("DSS", store);
        new.objects.insert(catalog_id, Object::Dictionary(catalog));
    });
    let started = std::time::Instant::now();
    let found = signatures(&pdf, &test_root());
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].integrity, Integrity::Intact);
    assert!(started.elapsed().as_secs() < 10, "{:?}", started.elapsed());
}
