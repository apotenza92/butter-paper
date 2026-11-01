//! Digitally signing a PDF: a PAdES baseline signature (B-B, or B-T with a
//! timestamp) appended to the saved file as an incremental update, so
//! everything before it, including earlier signatures, stays byte for byte.
//!
//! The signature is a detached CAdES (CMS) signature over the file's bytes
//! outside the signature itself, written here in DER with the signed
//! attributes PAdES requires: content type, message digest and the signing
//! certificate's hash. Each signed file is read back with
//! [`crate::pdf_signatures`] before it is returned.

use std::io::Write as _;

use der::{Decode as _, Encode as _, asn1::ObjectIdentifier};
use lopdf::{Dictionary, Document, Object, ObjectId, Stream, StringFormat, dictionary};
use signature_sha2::{Digest as _, Sha256};

use crate::annotation_model::DecodedRgbaAsset;
use crate::pdf_signatures::{Changes, Integrity, children, signatures, tlv};

const DATA: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.113549.1.7.1");
const SIGNED_DATA: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.113549.1.7.2");
const CONTENT_TYPE: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.113549.1.9.3");
const MESSAGE_DIGEST: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.113549.1.9.4");
const SIGNING_CERTIFICATE_V2: ObjectIdentifier =
    ObjectIdentifier::new_unwrap("1.2.840.113549.1.9.16.2.47");
const TIMESTAMP_TOKEN: ObjectIdentifier =
    ObjectIdentifier::new_unwrap("1.2.840.113549.1.9.16.2.14");
const SHA256: ObjectIdentifier = ObjectIdentifier::new_unwrap("2.16.840.1.101.3.4.2.1");
const SHA256_WITH_RSA: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.113549.1.1.11");
const ECDSA_WITH_SHA256: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.10045.4.3.2");
const ECDSA_WITH_SHA384: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.10045.4.3.3");

/// Room reserved for the signature, in bytes (twice that in hex). A
/// signature with a four-certificate chain and a timestamp fits easily.
const SIGNATURE_SPACE: usize = 24 * 1024;
/// A byte-range number's placeholder: wide enough for any file this signs.
const RANGE_PLACEHOLDER: i64 = 9_999_999_999;
const RANGE_WIDTH: usize = 10;

#[derive(Debug, PartialEq, Eq)]
pub enum SigningError {
    /// The certificate file's password is wrong.
    WrongPassword,
    /// Not a certificate file, or it holds no private key with its
    /// certificate.
    NoSigningKey,
    /// A key this app cannot sign with.
    UnsupportedKey,
    /// The PDF cannot be signed (encrypted, unreadable or too large).
    Document(String),
    /// The timestamp server failed.
    Timestamp(String),
    /// The signed file did not read back as intact: nothing is written.
    SelfCheck(String),
    /// The system's key store refused to sign (cancelled, or a wrong PIN).
    System(String),
}

impl std::fmt::Display for SigningError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::WrongPassword => {
                write!(formatter, "The password is not right for this certificate.")
            }
            Self::NoSigningKey => write!(
                formatter,
                "This file holds no certificate with its private key. Choose a .p12 or .pfx file \
                 exported with its private key."
            ),
            Self::UnsupportedKey => write!(
                formatter,
                "This certificate's key type can't be used to sign. RSA and ECDSA (P-256, P-384) \
                 keys can."
            ),
            Self::Document(reason) => write!(formatter, "This PDF can't be signed: {reason}."),
            Self::Timestamp(reason) => {
                write!(
                    formatter,
                    "The timestamp server could not be used: {reason}."
                )
            }
            Self::System(reason) => {
                write!(
                    formatter,
                    "Signing with the certificate didn't work: {reason}."
                )
            }
            Self::SelfCheck(reason) => write!(
                formatter,
                "The signature did not check out, so the PDF was left unchanged: {reason}."
            ),
        }
    }
}

impl std::error::Error for SigningError {}

enum SigningKey {
    Rsa(Box<rsa::RsaPrivateKey>),
    P256(p256::ecdsa::SigningKey),
    P384(p384::ecdsa::SigningKey),
    /// A key the operating system holds and signs with.
    System {
        key: Box<dyn crate::system_certificates::SystemKey>,
        rsa: bool,
    },
}

/// A private key and its certificate chain, signer first.
pub struct SigningIdentity {
    key: SigningKey,
    chain: Vec<Vec<u8>>,
}

impl SigningIdentity {
    /// The identity in a PKCS#12 (`.p12`/`.pfx`) file.
    pub fn from_pkcs12(bytes: &[u8], password: &str) -> Result<Self, SigningError> {
        use p12_keystore::{KeyStore, Pkcs12ImportPolicy};
        let store = KeyStore::from_pkcs12(bytes, password, Pkcs12ImportPolicy::Relaxed).map_err(
            |error| {
                let message = error.to_string().to_ascii_lowercase();
                if message.contains("mac") || message.contains("password") {
                    SigningError::WrongPassword
                } else {
                    SigningError::NoSigningKey
                }
            },
        )?;
        let (_, chain) = store
            .private_key_chain()
            .ok_or(SigningError::NoSigningKey)?;
        let key = signing_key(chain.key().as_der())?;
        let public = key.public_key_der()?;
        let certificates: Vec<Vec<u8>> = chain
            .certs()
            .iter()
            .map(|certificate| certificate.as_der().to_vec())
            .collect();
        // The signer's certificate is the one for this key; the rest are
        // its issuers.
        let signer = certificates
            .iter()
            .position(|certificate| subject_public_key(certificate).as_deref() == Some(&public[..]))
            .ok_or(SigningError::NoSigningKey)?;
        let mut ordered = vec![certificates[signer].clone()];
        ordered.extend(
            certificates
                .iter()
                .enumerate()
                .filter(|(index, _)| *index != signer)
                .map(|(_, certificate)| certificate.clone()),
        );
        Ok(Self {
            key,
            chain: ordered,
        })
    }

    /// An identity from the system's certificate store.
    pub fn from_system(identity: crate::system_certificates::SystemIdentity) -> Self {
        let rsa = identity
            .chain
            .first()
            .is_some_and(|certificate| crate::pdf_signatures::certificate_is_rsa(certificate));
        Self {
            key: SigningKey::System {
                key: identity.key,
                rsa,
            },
            chain: identity.chain,
        }
    }

    /// The signer's name, from their certificate.
    pub fn signer_name(&self) -> String {
        x509_cert::Certificate::from_der(&self.chain[0])
            .ok()
            .and_then(|certificate| {
                let subject = certificate.tbs_certificate().subject().clone();
                subject
                    .common_name()
                    .ok()
                    .flatten()
                    .map(|name| name.value().into_owned())
            })
            .unwrap_or_default()
    }

    /// The certificates, signer first.
    pub fn chain(&self) -> &[Vec<u8>] {
        &self.chain
    }

    fn sign(&self, message: &[u8]) -> Result<(ObjectIdentifier, Vec<u8>), SigningError> {
        self.sign_digest(&Sha256::digest(message))
    }

    fn sign_digest(&self, digest: &[u8]) -> Result<(ObjectIdentifier, Vec<u8>), SigningError> {
        use p256::ecdsa::signature::hazmat::PrehashSigner as _;
        match &self.key {
            SigningKey::Rsa(key) => key
                .sign(rsa::Pkcs1v15Sign::new::<Sha256>(), digest)
                .map(|signature| (SHA256_WITH_RSA, signature))
                .map_err(|_| SigningError::UnsupportedKey),
            SigningKey::P256(key) => {
                let signature: p256::ecdsa::Signature = key
                    .sign_prehash(digest)
                    .map_err(|_| SigningError::UnsupportedKey)?;
                Ok((ECDSA_WITH_SHA256, signature.to_der().as_bytes().to_vec()))
            }
            SigningKey::P384(key) => {
                // SHA-256 for every signature keeps one digest throughout;
                // ECDSA over P-384 accepts it.
                let signature: p384::ecdsa::Signature = key
                    .sign_prehash(digest)
                    .map_err(|_| SigningError::UnsupportedKey)?;
                let _ = ECDSA_WITH_SHA384;
                Ok((ECDSA_WITH_SHA256, signature.to_der().as_bytes().to_vec()))
            }
            SigningKey::System { key, rsa } => key
                .sign_sha256(digest)
                .map(|signature| {
                    (
                        if *rsa {
                            SHA256_WITH_RSA
                        } else {
                            ECDSA_WITH_SHA256
                        },
                        signature,
                    )
                })
                .map_err(SigningError::System),
        }
    }
}

/// A certificate file's identity signs as a system's would.
impl crate::system_certificates::SystemKey for SigningIdentity {
    fn sign_sha256(&self, digest: &[u8]) -> Result<Vec<u8>, String> {
        self.sign_digest(digest)
            .map(|(_, signature)| signature)
            .map_err(|error| error.to_string())
    }
}

impl SigningKey {
    fn public_key_der(&self) -> Result<Vec<u8>, SigningError> {
        use p256::pkcs8::EncodePublicKey as _;
        let der = match self {
            Self::Rsa(key) => {
                use rsa::pkcs8::EncodePublicKey as _;
                key.to_public_key()
                    .to_public_key_der()
                    .map(|der| der.as_bytes().to_vec())
            }
            Self::P256(key) => key
                .verifying_key()
                .to_public_key_der()
                .map(|der| der.as_bytes().to_vec()),
            Self::P384(key) => key
                .verifying_key()
                .to_public_key_der()
                .map(|der| der.as_bytes().to_vec()),
            Self::System { .. } => return Err(SigningError::UnsupportedKey),
        };
        der.map_err(|_| SigningError::UnsupportedKey)
    }
}

fn signing_key(pkcs8: &[u8]) -> Result<SigningKey, SigningError> {
    use p256::pkcs8::DecodePrivateKey as _;
    if let Ok(key) = <rsa::RsaPrivateKey as rsa::pkcs8::DecodePrivateKey>::from_pkcs8_der(pkcs8) {
        return Ok(SigningKey::Rsa(Box::new(key)));
    }
    if let Ok(key) = p256::ecdsa::SigningKey::from_pkcs8_der(pkcs8) {
        return Ok(SigningKey::P256(key));
    }
    if let Ok(key) = p384::ecdsa::SigningKey::from_pkcs8_der(pkcs8) {
        return Ok(SigningKey::P384(key));
    }
    Err(SigningError::UnsupportedKey)
}

/// A certificate's SubjectPublicKeyInfo, as DER.
fn subject_public_key(certificate: &[u8]) -> Option<Vec<u8>> {
    let certificate = x509_cert::Certificate::from_der(certificate).ok()?;
    certificate
        .tbs_certificate()
        .subject_public_key_info()
        .to_der()
        .ok()
}

/// Where a visible signature goes and how it looks.
#[derive(Default)]
pub struct SignatureAppearance {
    /// Zero-based page index.
    pub page: u32,
    /// The box in PDF user space: left, bottom, right, top.
    pub rect: [f64; 4],
    /// The handwritten signature drawn in the box, if any.
    pub image: Option<DecodedRgbaAsset>,
    /// Without an image, only the details, not Acrobat's large name
    /// beside them.
    pub details_only: bool,
    /// The markup (by its `/NM`) the signature takes the place of: it is
    /// left out of the page in the signed copy.
    pub replaces: Option<String>,
}

#[derive(Default)]
pub struct SignOptions {
    pub reason: Option<String>,
    pub location: Option<String>,
    /// A visible signature; `None` signs invisibly.
    pub appearance: Option<SignatureAppearance>,
    /// An RFC 3161 timestamp server, for a timestamped (PAdES B-T)
    /// signature.
    pub timestamp_server: Option<String>,
    /// Also keep the certificates' revocation data in the PDF (PAdES
    /// B-LT), so the signature can be checked after they expire.
    pub long_term: bool,
}

/// How signing reaches the network: a timestamp server (the server and
/// the DER query, returning the DER response) and certificates' OCSP
/// responders and revocation lists (a URL and the request to post).
pub struct Network<'a> {
    pub timestamp: &'a dyn Fn(&str, &[u8]) -> Result<Vec<u8>, String>,
    pub revocation: crate::pdf_signatures::Fetch<'a>,
}

/// `pdf` signed by `identity`: the same bytes with a signature appended.
pub fn sign_pdf(
    pdf: &[u8],
    identity: &SigningIdentity,
    options: &SignOptions,
) -> Result<Vec<u8>, SigningError> {
    sign_pdf_with(
        pdf,
        identity,
        options,
        &Network {
            timestamp: &request_timestamp,
            revocation: &crate::signature_trust::fetch_revocation,
        },
    )
}

/// [`sign_pdf`] reaching the network through `network`.
pub fn sign_pdf_with(
    pdf: &[u8],
    identity: &SigningIdentity,
    options: &SignOptions,
    network: &Network<'_>,
) -> Result<Vec<u8>, SigningError> {
    let document = |reason: &str| SigningError::Document(reason.to_owned());
    let source = Document::load_mem(pdf).map_err(|_| document("it could not be read"))?;
    if source.is_encrypted() {
        return Err(document("it needs a password to open"));
    }
    if certified_without_changes(&source) {
        return Err(document(
            "it is certified to allow no changes, so another signature would break that",
        ));
    }
    if source.xref_start == 0 {
        return Err(document("its cross-reference table could not be read"));
    }
    let (mut bytes, field_name) = append_signature_fields(pdf, &source, identity, options)?;

    // The placeholders, in the appended update only.
    let tail = pdf.len();
    let range_pattern = format!("{RANGE_PLACEHOLDER} {RANGE_PLACEHOLDER} {RANGE_PLACEHOLDER}");
    let range_at = find(&bytes[tail..], range_pattern.as_bytes())
        .map(|at| at + tail)
        .ok_or_else(|| document("the signature space could not be reserved"))?;
    let mut contents_pattern = vec![b'<'];
    contents_pattern.extend(std::iter::repeat_n(b'0', SIGNATURE_SPACE * 2));
    contents_pattern.push(b'>');
    let contents_at = find(&bytes[tail..], &contents_pattern)
        .map(|at| at + tail)
        .ok_or_else(|| document("the signature space could not be reserved"))?;
    let gap_start = contents_at;
    let gap_end = contents_at + contents_pattern.len();
    let ranges = [0, gap_start, gap_end, bytes.len() - gap_end];
    let mut range_text = String::new();
    for (index, value) in ranges[1..].iter().enumerate() {
        if index > 0 {
            range_text.push(' ');
        }
        range_text.push_str(&format!("{value:<RANGE_WIDTH$}"));
    }
    if range_text.len() != range_pattern.len() {
        return Err(document("it is too large to sign"));
    }
    bytes[range_at..range_at + range_text.len()].copy_from_slice(range_text.as_bytes());

    let mut hasher = Sha256::new();
    hasher.update(&bytes[..gap_start]);
    hasher.update(&bytes[gap_end..]);
    let document_digest = hasher.finalize().to_vec();
    let (cms, token) = signed_data(identity, &document_digest, options, network.timestamp)?;
    if cms.len() > SIGNATURE_SPACE {
        return Err(document(
            "its signature is larger than the space reserved for it",
        ));
    }
    let mut hex = Vec::with_capacity(SIGNATURE_SPACE * 2);
    for byte in &cms {
        write!(hex, "{byte:02X}").expect("writing to a vector");
    }
    bytes[gap_start + 1..gap_start + 1 + hex.len()].copy_from_slice(&hex);

    // Read it back as any reader would.
    let found = signatures(&bytes, &[]);
    let signed = found
        .iter()
        .find(|signature| signature.field == field_name)
        .ok_or_else(|| SigningError::SelfCheck("the signature was not found".into()))?;
    if signed.integrity != Integrity::Intact || signed.changes != Changes::None {
        return Err(SigningError::SelfCheck(format!(
            "{:?}, {:?}",
            signed.integrity, signed.changes
        )));
    }
    if options.timestamp_server.is_some()
        && !signed
            .timestamp
            .as_ref()
            .is_some_and(|timestamp| timestamp.valid)
    {
        return Err(SigningError::SelfCheck(
            "its timestamp does not check out".into(),
        ));
    }
    if !options.long_term {
        return Ok(bytes);
    }
    let mut certificates = identity.chain.clone();
    if let Some(token) = &token {
        certificates.extend(crate::pdf_signatures::signed_data_certificates(token));
    }
    let with_data = append_validation_data(&bytes, &certificates, network.revocation)?;
    let found = signatures(&with_data, &[]);
    let kept = found
        .iter()
        .find(|signature| signature.field == field_name)
        .is_some_and(|signature| {
            signature.integrity == Integrity::Intact
                && matches!(
                    signature.changes,
                    Changes::Permitted {
                        signatures: true,
                        other: false
                    }
                )
        });
    if !kept {
        return Err(SigningError::SelfCheck(
            "its validation data disturbed the signature".into(),
        ));
    }
    Ok(with_data)
}

/// A document security store (`/DSS`) appended to the signed `pdf`: the
/// certificates and, for each below its root, an OCSP response or else a
/// revocation list from its issuer.
fn append_validation_data(
    pdf: &[u8],
    certificates: &[Vec<u8>],
    fetch: crate::pdf_signatures::Fetch<'_>,
) -> Result<Vec<u8>, SigningError> {
    use crate::pdf_signatures::{issued_by, ocsp_request_for, revocation_urls, self_signed};
    let document = |reason: &str| SigningError::Document(reason.to_owned());
    let mut unique: Vec<Vec<u8>> = Vec::new();
    for certificate in certificates {
        if !unique.contains(certificate) {
            unique.push(certificate.clone());
        }
    }
    let mut ocsps = Vec::new();
    let mut crls = Vec::new();
    for certificate in &unique {
        if self_signed(certificate) {
            continue;
        }
        let Some(issuer) = unique
            .iter()
            .find(|issuer| *issuer != certificate && issued_by(certificate, issuer))
        else {
            continue;
        };
        let (ocsp_urls, crl_urls) = revocation_urls(certificate);
        let request = ocsp_request_for(certificate, issuer);
        let answer = request
            .as_deref()
            .and_then(|request| ocsp_urls.iter().find_map(|url| fetch(url, Some(request))));
        match answer {
            Some(response) => ocsps.push(response),
            None => {
                if let Some(crl) = crl_urls.iter().find_map(|url| fetch(url, None))
                    && !crls.contains(&crl)
                {
                    crls.push(crl);
                }
            }
        }
    }

    let source = Document::load_mem(pdf).map_err(|_| document("it could not be read"))?;
    let mut previous = Document::new();
    previous.version = source.version.clone();
    previous.max_id = source.max_id;
    previous.xref_start = source.xref_start;
    previous.reference_table.cross_reference_type = source.reference_table.cross_reference_type;
    for key in [b"Root".as_slice(), b"Info", b"ID"] {
        if let Ok(value) = source.trailer.get(key) {
            previous.trailer.set(key, value.clone());
        }
    }
    let mut update = lopdf::IncrementalDocument::create_from(pdf.to_vec(), previous);
    let new = &mut update.new_document;
    new.max_id = source.max_id;
    let mut streams = |items: &[Vec<u8>]| -> Vec<Object> {
        items
            .iter()
            .map(|item| {
                let mut stream = Stream::new(Dictionary::new(), item.clone());
                let _ = stream.compress();
                new.add_object(stream).into()
            })
            .collect()
    };
    let mut store = dictionary! { "Type" => "DSS", "Certs" => streams(&unique) };
    if !ocsps.is_empty() {
        store.set("OCSPs", streams(&ocsps));
    }
    if !crls.is_empty() {
        store.set("CRLs", streams(&crls));
    }
    let store_id = new.add_object(store);
    let catalog_id = source
        .trailer
        .get(b"Root")
        .and_then(Object::as_reference)
        .map_err(|_| document("it has no catalog"))?;
    let mut catalog = source
        .get_dictionary(catalog_id)
        .map_err(|_| document("it has no catalog"))?
        .clone();
    catalog.set("DSS", store_id);
    new.objects.insert(catalog_id, Object::Dictionary(catalog));
    encrypt_update(&source, &mut update, None)?;
    let mut bytes = Vec::with_capacity(pdf.len() + 32 * 1024);
    update
        .save_to(&mut bytes)
        .map_err(|_| document("its validation data could not be written"))?;
    Ok(bytes)
}

/// The signature field, its widget and the signature dictionary with
/// placeholders, appended to `pdf`; and the field's name.
fn append_signature_fields(
    pdf: &[u8],
    source: &Document,
    identity: &SigningIdentity,
    options: &SignOptions,
) -> Result<(Vec<u8>, String), SigningError> {
    let document = |reason: &str| SigningError::Document(reason.to_owned());
    let mut previous = Document::new();
    previous.version = source.version.clone();
    previous.max_id = source.max_id;
    previous.xref_start = source.xref_start;
    previous.reference_table.cross_reference_type = source.reference_table.cross_reference_type;
    for key in [b"Root".as_slice(), b"Info", b"ID"] {
        if let Ok(value) = source.trailer.get(key) {
            previous.trailer.set(key, value.clone());
        }
    }
    let mut update = lopdf::IncrementalDocument::create_from(pdf.to_vec(), previous);
    let new = &mut update.new_document;
    new.max_id = source.max_id;

    let catalog_id = source
        .trailer
        .get(b"Root")
        .and_then(Object::as_reference)
        .map_err(|_| document("it has no catalog"))?;
    let pages = source.get_pages();
    let page_index = options
        .appearance
        .as_ref()
        .map_or(0, |appearance| appearance.page);
    let page_id = *pages
        .get(&(page_index + 1))
        .ok_or_else(|| document("the page for the signature is missing"))?;

    let field_name = unused_field_name(source);
    let name = identity.signer_name();
    let mut signature = dictionary! {
        "Type" => "Sig",
        "Filter" => "Adobe.PPKLite",
        "SubFilter" => "ETSI.CAdES.detached",
        "ByteRange" => vec![
            Object::Integer(0),
            Object::Integer(RANGE_PLACEHOLDER),
            Object::Integer(RANGE_PLACEHOLDER),
            Object::Integer(RANGE_PLACEHOLDER),
        ],
        "Contents" => Object::String(vec![0; SIGNATURE_SPACE], StringFormat::Hexadecimal),
        "M" => Object::string_literal(crate::pdf_engine::pdf_date_now()),
        "Prop_Build" => dictionary! {
            "App" => dictionary! { "Name" => "ButterPaper" },
        },
    };
    if !name.is_empty() {
        signature.set(
            "Name",
            Object::String(pdf_text(&name), StringFormat::Literal),
        );
    }
    if let Some(reason) = options
        .reason
        .as_deref()
        .filter(|reason| !reason.is_empty())
    {
        signature.set(
            "Reason",
            Object::String(pdf_text(reason), StringFormat::Literal),
        );
    }
    if let Some(location) = options
        .location
        .as_deref()
        .filter(|location| !location.is_empty())
    {
        signature.set(
            "Location",
            Object::String(pdf_text(location), StringFormat::Literal),
        );
    }
    let signature_id = new.add_object(signature);

    let (rect, appearance) = match &options.appearance {
        Some(appearance) => {
            let [left, bottom, right, top] = appearance.rect;
            let stream = appearance_stream(new, appearance, &name);
            (
                vec![left.into(), bottom.into(), right.into(), top.into()],
                new.add_object(stream),
            )
        }
        None => {
            let empty = Stream::new(
                dictionary! {
                    "Type" => "XObject",
                    "Subtype" => "Form",
                    "BBox" => vec![0.into(), 0.into(), 0.into(), 0.into()],
                },
                Vec::new(),
            );
            (
                vec![0.into(), 0.into(), 0.into(), 0.into()],
                new.add_object(empty),
            )
        }
    };
    let widget_id = new.add_object(dictionary! {
        "Type" => "Annot",
        "Subtype" => "Widget",
        "FT" => "Sig",
        "T" => Object::String(pdf_text(&field_name), StringFormat::Literal),
        "V" => signature_id,
        // Printed and locked.
        "F" => 132,
        "Rect" => rect,
        "P" => page_id,
        "AP" => dictionary! { "N" => appearance },
    });

    // The page lists the widget among its annotations.
    let mut page = source
        .get_dictionary(page_id)
        .map_err(|_| document("the page for the signature is missing"))?
        .clone();
    let replaced = options
        .appearance
        .as_ref()
        .and_then(|appearance| appearance.replaces.as_deref());
    let keep = |item: &Object| {
        let Some(replaced) = replaced else {
            return true;
        };
        let annotation = match item {
            Object::Reference(id) => source.get_dictionary(*id).ok(),
            Object::Dictionary(annotation) => Some(annotation),
            _ => None,
        };
        annotation
            .and_then(|annotation| annotation.get(b"NM").ok())
            .and_then(crate::pdf_signatures::text)
            .is_none_or(|name| name != replaced)
    };
    match page.get(b"Annots").ok().cloned() {
        Some(Object::Reference(list_id)) => {
            let mut list = source
                .get_object(list_id)
                .and_then(Object::as_array)
                .map_err(|_| document("the page's annotation list is unreadable"))?
                .clone();
            list.retain(keep);
            list.push(widget_id.into());
            new.objects.insert(list_id, Object::Array(list));
        }
        Some(Object::Array(mut list)) => {
            list.retain(keep);
            list.push(widget_id.into());
            page.set("Annots", list);
        }
        _ => page.set("Annots", vec![widget_id.into()]),
    }
    new.objects.insert(page_id, Object::Dictionary(page));

    // The form lists the field, and says the document is signed and
    // must be saved by appending.
    let mut catalog = source
        .get_dictionary(catalog_id)
        .map_err(|_| document("it has no catalog"))?
        .clone();
    let mut catalog_changed = false;
    match catalog.get(b"AcroForm").ok().cloned() {
        Some(Object::Reference(form_id)) => {
            let mut form = source
                .get_dictionary(form_id)
                .map_err(|_| document("its form is unreadable"))?
                .clone();
            add_field(source, new, &mut form, widget_id)?;
            new.objects.insert(form_id, Object::Dictionary(form));
        }
        Some(Object::Dictionary(mut form)) => {
            add_field(source, new, &mut form, widget_id)?;
            catalog.set("AcroForm", form);
            catalog_changed = true;
        }
        _ => {
            let form = dictionary! {
                "Fields" => vec![widget_id.into()],
                "SigFlags" => 3,
            };
            let form_id = new.add_object(form);
            catalog.set("AcroForm", form_id);
            catalog_changed = true;
        }
    }
    if catalog_changed {
        new.objects.insert(catalog_id, Object::Dictionary(catalog));
    }

    encrypt_update(source, &mut update, Some(signature_id))?;
    let mut bytes = Vec::with_capacity(pdf.len() + SIGNATURE_SPACE * 2 + 16 * 1024);
    update
        .save_to(&mut bytes)
        .map_err(|_| document("the signature could not be written"))?;
    Ok((bytes, field_name))
}

/// Whether a certification signature (DocMDP) allows no changes at all
/// (permission 1), as Acrobat then refuses another signature.
fn certified_without_changes(document: &Document) -> bool {
    let resolve = |object: &Object| -> Option<Object> {
        match object {
            Object::Reference(id) => document.get_object(*id).ok().cloned(),
            other => Some(other.clone()),
        }
    };
    let Some(Object::Dictionary(signature)) = document
        .catalog()
        .ok()
        .and_then(|catalog| catalog.get(b"Perms").ok().and_then(resolve))
        .and_then(|perms| match perms {
            Object::Dictionary(perms) => perms.get(b"DocMDP").ok().and_then(resolve),
            _ => None,
        })
    else {
        return false;
    };
    let Some(Object::Array(references)) = signature.get(b"Reference").ok().and_then(resolve) else {
        // DocMDP without transform parameters defaults to permission 2.
        return false;
    };
    references.iter().filter_map(resolve).any(|reference| {
        let Object::Dictionary(reference) = reference else {
            return false;
        };
        reference
            .get(b"TransformMethod")
            .ok()
            .and_then(|method| method.as_name().ok())
            == Some(b"DocMDP".as_slice())
            && reference
                .get(b"TransformParams")
                .ok()
                .and_then(resolve)
                .and_then(|params| match params {
                    Object::Dictionary(params) => {
                        params.get(b"P").ok().and_then(|p| p.as_i64().ok())
                    }
                    _ => None,
                })
                == Some(1)
    })
}

/// For a PDF with only an owner password: the update's objects encrypted
/// under the file's own key, as a reader expects, except the signature's
/// `/Contents`, which the standard keeps in the clear.
fn encrypt_update(
    source: &Document,
    update: &mut lopdf::IncrementalDocument,
    signature: Option<ObjectId>,
) -> Result<(), SigningError> {
    let Some(state) = source.encryption_state.as_ref() else {
        return Ok(());
    };
    let unreadable = || SigningError::Document("its encryption could not be kept".into());
    let encrypt_id = state.encrypt_object_id().ok_or_else(unreadable)?;
    let new = &mut update.new_document;
    for (&id, object) in new.objects.iter_mut() {
        let contents = (Some(id) == signature)
            .then(|| object.as_dict_mut().ok()?.remove(b"Contents"))
            .flatten();
        lopdf::encryption::encrypt_object(state, id, object).map_err(|_| unreadable())?;
        if let Some(contents) = contents
            && let Ok(dictionary) = object.as_dict_mut()
        {
            dictionary.set("Contents", contents);
        }
    }
    new.trailer.set("Encrypt", encrypt_id);
    Ok(())
}

fn add_field(
    source: &Document,
    new: &mut Document,
    form: &mut Dictionary,
    widget_id: ObjectId,
) -> Result<(), SigningError> {
    form.set("SigFlags", 3);
    match form.get(b"Fields").ok().cloned() {
        Some(Object::Reference(list_id)) => {
            let mut list = source
                .get_object(list_id)
                .and_then(Object::as_array)
                .map_err(|_| SigningError::Document("its form's field list is unreadable".into()))?
                .clone();
            list.push(widget_id.into());
            new.objects.insert(list_id, Object::Array(list));
        }
        Some(Object::Array(mut list)) => {
            list.push(widget_id.into());
            form.set("Fields", list);
        }
        _ => form.set("Fields", vec![widget_id.into()]),
    }
    Ok(())
}

/// "Signature1", "Signature2"…, the first no field uses.
fn unused_field_name(document: &Document) -> String {
    let used: std::collections::HashSet<String> = document
        .objects
        .values()
        .filter_map(|object| match object {
            Object::Dictionary(dictionary) => dictionary.get(b"T").ok(),
            _ => None,
        })
        .filter_map(crate::pdf_signatures::text)
        .collect();
    (1..)
        .map(|number| format!("Signature{number}"))
        .find(|name| !used.contains(name))
        .expect("an unused field name")
}

/// PDF text: plain ASCII stays as is; anything else is UTF-16 with a byte
/// order mark.
fn pdf_text(text: &str) -> Vec<u8> {
    if text.is_ascii() {
        return text.as_bytes().to_vec();
    }
    let mut bytes = vec![0xFE, 0xFF];
    for unit in text.encode_utf16() {
        bytes.extend_from_slice(&unit.to_be_bytes());
    }
    bytes
}

/// The visible signature: the handwritten signature on the left (or across
/// the box without text room), and "Digitally signed by" with the name and
/// date on the right.
fn appearance_stream(
    document: &mut Document,
    appearance: &SignatureAppearance,
    name: &str,
) -> Stream {
    let [left, bottom, right, top] = appearance.rect;
    let (width, height) = ((right - left).abs().max(1.), (top - bottom).abs().max(1.));
    let mut content = String::new();
    let mut xobjects = Dictionary::new();
    let text_left = match &appearance.image {
        Some(image) => {
            let image_id = image_xobject(document, image);
            xobjects.set("Signature", image_id);
            let area = if width > height * 2.5 {
                width * 0.55
            } else {
                width
            };
            let scale =
                (area / f64::from(image.width_px())).min(height / f64::from(image.height_px()));
            let (drawn_width, drawn_height) = (
                f64::from(image.width_px()) * scale,
                f64::from(image.height_px()) * scale,
            );
            let x = (area - drawn_width) / 2.;
            let y = (height - drawn_height) / 2.;
            content.push_str(&format!(
                "q {drawn_width:.3} 0 0 {drawn_height:.3} {x:.3} {y:.3} cm /Signature Do Q\n"
            ));
            (area < width).then_some(area + 4.)
        }
        // Acrobat's standard text: the name large on the left, when the box
        // has room beside the details.
        None if !appearance.details_only && width > height * 1.6 && !name.is_empty() => {
            let area = width * 0.5;
            let text = ascii(name);
            let size = ((area - 8.) / (text.len().max(1) as f64 * 0.55))
                .min(height * 0.6)
                .clamp(4., 48.);
            let y = (height - size * 0.7) / 2.;
            content.push_str(&format!(
                "BT /Helv {size:.2} Tf 0 g 1 0 0 1 4 {y:.3} Tm ({}) Tj ET\n",
                escape(&text)
            ));
            Some(area + 4.)
        }
        None => Some(4.),
    };
    if let Some(text_left) = text_left {
        let date = crate::pdf_engine::pdf_date_now();
        let date = crate::pdf_signatures::pdf_date(&date)
            .map(|seconds| {
                let offset = crate::pdf_engine::local_utc_offset_seconds(seconds);
                let local = crate::pdf_engine::format_pdf_date(seconds, offset);
                // D:YYYYMMDDHHmmSS+HH'mm' → YYYY.MM.DD HH:mm:SS +HH'mm'
                let digits = &local[2..];
                format!(
                    "{}.{}.{} {}:{}:{} {}",
                    &digits[0..4],
                    &digits[4..6],
                    &digits[6..8],
                    &digits[8..10],
                    &digits[10..12],
                    &digits[12..14],
                    &digits[14..]
                )
            })
            .unwrap_or_default();
        let lines = [
            "Digitally signed by".to_owned(),
            ascii(name),
            format!("Date: {date}"),
        ];
        let room = width - text_left - 4.;
        let longest = lines.iter().map(String::len).max().unwrap_or(1) as f64;
        // Helvetica averages about half an em per character.
        let size = (room / (longest * 0.5))
            .min(height / (lines.len() as f64 * 1.25))
            .clamp(3., 12.);
        let leading = size * 1.2;
        let block = leading * lines.len() as f64;
        let mut y = (height + block) / 2. - size;
        content.push_str("BT /Helv ");
        content.push_str(&format!("{size:.2} Tf 0 g\n"));
        for line in &lines {
            content.push_str(&format!(
                "1 0 0 1 {text_left:.3} {y:.3} Tm ({}) Tj\n",
                escape(line)
            ));
            y -= leading;
        }
        content.push_str("ET\n");
    }
    let font = document.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Helvetica",
        "Encoding" => "WinAnsiEncoding",
    });
    let mut resources = dictionary! { "Font" => dictionary! { "Helv" => font } };
    if !xobjects.is_empty() {
        resources.set("XObject", xobjects);
    }
    Stream::new(
        dictionary! {
            "Type" => "XObject",
            "Subtype" => "Form",
            "BBox" => vec![0.into(), 0.into(), width.into(), height.into()],
            "Resources" => resources,
        },
        content.into_bytes(),
    )
}

/// The signature image: RGB with its alpha as a soft mask.
fn image_xobject(document: &mut Document, image: &DecodedRgbaAsset) -> ObjectId {
    let mut rgb = Vec::with_capacity(image.rgba().len() / 4 * 3);
    let mut alpha = Vec::with_capacity(image.rgba().len() / 4);
    for pixel in image.rgba().chunks_exact(4) {
        rgb.extend_from_slice(&pixel[..3]);
        alpha.push(pixel[3]);
    }
    let compress = |bytes: &[u8]| {
        let mut encoder =
            flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
        encoder.write_all(bytes).expect("writing to a vector");
        encoder.finish().expect("writing to a vector")
    };
    let mask = document.add_object(Stream::new(
        dictionary! {
            "Type" => "XObject",
            "Subtype" => "Image",
            "Width" => image.width_px(),
            "Height" => image.height_px(),
            "ColorSpace" => "DeviceGray",
            "BitsPerComponent" => 8,
            "Filter" => "FlateDecode",
        },
        compress(&alpha),
    ));
    document.add_object(Stream::new(
        dictionary! {
            "Type" => "XObject",
            "Subtype" => "Image",
            "Width" => image.width_px(),
            "Height" => image.height_px(),
            "ColorSpace" => "DeviceRGB",
            "BitsPerComponent" => 8,
            "Filter" => "FlateDecode",
            "SMask" => mask,
        },
        compress(&rgb),
    ))
}

fn ascii(text: &str) -> String {
    text.chars()
        .map(|character| if character.is_ascii() { character } else { '?' })
        .collect()
}

fn escape(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for character in text.chars() {
        if matches!(character, '(' | ')' | '\\') {
            escaped.push('\\');
        }
        escaped.push(character);
    }
    escaped
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

// DER, written by hand: the signed attributes must be signed exactly as
// they are encoded.

fn der(tag: u8, content: &[u8]) -> Vec<u8> {
    let mut out = vec![tag];
    let length = content.len();
    if length < 0x80 {
        out.push(length as u8);
    } else {
        let bytes: Vec<u8> = length
            .to_be_bytes()
            .into_iter()
            .skip_while(|byte| *byte == 0)
            .collect();
        out.push(0x80 | bytes.len() as u8);
        out.extend(bytes);
    }
    out.extend_from_slice(content);
    out
}

fn sequence(parts: &[Vec<u8>]) -> Vec<u8> {
    der(0x30, &parts.concat())
}

/// A SET OF, its elements in DER order.
fn set(parts: &[Vec<u8>]) -> Vec<u8> {
    let mut sorted = parts.to_vec();
    sorted.sort();
    der(0x31, &sorted.concat())
}

fn oid(oid: ObjectIdentifier) -> Vec<u8> {
    oid.to_der().expect("an object identifier encodes")
}

fn algorithm(identifier: ObjectIdentifier, null_parameters: bool) -> Vec<u8> {
    if null_parameters {
        sequence(&[oid(identifier), vec![0x05, 0x00]])
    } else {
        sequence(&[oid(identifier)])
    }
}

fn attribute(kind: ObjectIdentifier, value: Vec<u8>) -> Vec<u8> {
    sequence(&[oid(kind), set(&[value])])
}

/// The signer's issuer and serial number, as their certificate encodes
/// them.
fn issuer_and_serial(certificate: &[u8]) -> Option<(Vec<u8>, Vec<u8>)> {
    let (certificate, _) = tlv(certificate)?;
    let (tbs, _) = tlv(certificate.content)?;
    let parts = children(tbs.content)?;
    let offset = usize::from(parts.first()?.tag == 0xA0);
    let serial = parts.get(offset)?.raw.to_vec();
    let issuer = parts.get(offset + 2)?.raw.to_vec();
    Some((issuer, serial))
}

/// The CMS SignedData (detached) for a document whose signed bytes hash to
/// `document_digest`.
/// The CMS, and the timestamp token in it.
fn signed_data(
    identity: &SigningIdentity,
    document_digest: &[u8],
    options: &SignOptions,
    timestamp: &dyn Fn(&str, &[u8]) -> Result<Vec<u8>, String>,
) -> Result<(Vec<u8>, Option<Vec<u8>>), SigningError> {
    let signer = &identity.chain[0];
    let (issuer, serial) = issuer_and_serial(signer).ok_or(SigningError::NoSigningKey)?;
    let certificate_hash = Sha256::digest(signer).to_vec();
    // ESSCertIDv2 with the default hash (SHA-256) left out, and the
    // issuer and serial number.
    let issuer_serial = sequence(&[sequence(&[der(0xA4, &issuer)]), serial.clone()]);
    let signing_certificate = sequence(&[sequence(&[sequence(&[
        der(0x04, &certificate_hash),
        issuer_serial,
    ])])]);
    let attributes = vec![
        attribute(CONTENT_TYPE, oid(DATA)),
        attribute(MESSAGE_DIGEST, der(0x04, document_digest)),
        attribute(SIGNING_CERTIFICATE_V2, signing_certificate),
    ];
    let signed_attributes = set(&attributes);
    let (signature_algorithm, signature) = identity.sign(&signed_attributes)?;
    let mut signer_info = vec![
        der(0x02, &[1]),
        sequence(&[issuer, serial]),
        algorithm(SHA256, false),
        {
            let mut implicit = signed_attributes.clone();
            implicit[0] = 0xA0;
            implicit
        },
        algorithm(signature_algorithm, signature_algorithm == SHA256_WITH_RSA),
        der(0x04, &signature),
    ];
    let mut kept_token = None;
    if let Some(server) = &options.timestamp_server {
        let token = timestamp_token(server, &signature, timestamp)?;
        kept_token = Some(token.clone());
        let unsigned = set(&[attribute(TIMESTAMP_TOKEN, token)]);
        let mut implicit = unsigned;
        implicit[0] = 0xA1;
        signer_info.push(implicit);
    }
    let certificates: Vec<u8> = identity.chain.concat();
    let signed = sequence(&[
        der(0x02, &[1]),
        set(&[algorithm(SHA256, false)]),
        sequence(&[oid(DATA)]),
        der(0xA0, &certificates),
        set(&[sequence(&signer_info)]),
    ]);
    Ok((
        sequence(&[oid(SIGNED_DATA), der(0xA0, &signed)]),
        kept_token,
    ))
}

/// A timestamp authority's token over `signature` (RFC 3161).
fn timestamp_token(
    server: &str,
    signature: &[u8],
    timestamp: &dyn Fn(&str, &[u8]) -> Result<Vec<u8>, String>,
) -> Result<Vec<u8>, SigningError> {
    let imprint = Sha256::digest(signature).to_vec();
    let mut nonce = [0_u8; 8];
    getrandom::fill(&mut nonce).map_err(|error| SigningError::Timestamp(error.to_string()))?;
    // Positive, and minimal as DER needs: no leading zero byte.
    nonce[0] = (nonce[0] & 0x7F) | 0x40;
    let query = sequence(&[
        der(0x02, &[1]),
        sequence(&[algorithm(SHA256, false), der(0x04, &imprint)]),
        der(0x02, &nonce),
        // certReq: the token includes the authority's certificate.
        vec![0x01, 0x01, 0xFF],
    ]);
    let response = timestamp(server, &query).map_err(SigningError::Timestamp)?;
    let (response, _) =
        tlv(&response).ok_or_else(|| SigningError::Timestamp("an unreadable reply".into()))?;
    let parts = children(response.content)
        .ok_or_else(|| SigningError::Timestamp("an unreadable reply".into()))?;
    let status = parts
        .first()
        .and_then(|status| children(status.content))
        .and_then(|status| status.first().map(|value| value.content.to_vec()))
        .ok_or_else(|| SigningError::Timestamp("an unreadable reply".into()))?;
    // granted (0) or grantedWithMods (1)
    if !matches!(status.as_slice(), [0] | [1]) {
        return Err(SigningError::Timestamp(
            "the server refused the request".into(),
        ));
    }
    let token = parts
        .get(1)
        .ok_or_else(|| SigningError::Timestamp("the reply has no timestamp".into()))?
        .raw
        .to_vec();
    // A token for another request (or altered on the way) would be kept in
    // the PDF and then fail every check.
    if !crate::pdf_signatures::timestamp_answers(&token, signature, &nonce) {
        return Err(SigningError::Timestamp(
            "its timestamp doesn't match this signature".into(),
        ));
    }
    Ok(token)
}

/// Posts a timestamp query (RFC 3161 over HTTP).
fn request_timestamp(server: &str, query: &[u8]) -> Result<Vec<u8>, String> {
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|error| error.to_string())?;
    let response = client
        .post(server)
        .header("Content-Type", "application/timestamp-query")
        .body(query.to_vec())
        .send()
        .and_then(reqwest::blocking::Response::error_for_status)
        .map_err(|error| error.to_string())?;
    response
        .bytes()
        .map(|bytes| bytes.to_vec())
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn der_lengths_and_sets_encode_canonically() {
        assert_eq!(der(0x04, &[1, 2]), vec![0x04, 0x02, 1, 2]);
        let long = der(0x04, &[0; 300]);
        assert_eq!(&long[..4], &[0x04, 0x82, 0x01, 0x2C]);
        assert_eq!(
            set(&[vec![0x02, 0x01, 9], vec![0x02, 0x01, 1]]),
            vec![0x31, 0x06, 0x02, 0x01, 1, 0x02, 0x01, 9]
        );
    }

    #[test]
    fn non_ascii_text_is_utf16() {
        assert_eq!(pdf_text("Sam"), b"Sam".to_vec());
        assert_eq!(pdf_text("Zoë")[..2], [0xFE, 0xFF]);
    }
}
