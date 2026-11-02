//! Digital signatures in a PDF: who signed, whether the signed bytes still
//! match, what changed after signing, and whether the signer's certificate
//! chains to a trusted root.
//!
//! The CMS structures are walked as raw DER and their parts decoded on their
//! own, so a signature is checked against the exact bytes its signer signed
//! (re-encoding a decoded `SET OF` can reorder it).

use std::collections::{BTreeSet, HashSet};

use der::{Decode as _, Encode as _, asn1::ObjectIdentifier};
use lopdf::{Dictionary, Document, Object, ObjectId};
use x509_cert::Certificate;

const SIGNED_DATA: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.113549.1.7.2");
const MESSAGE_DIGEST: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.113549.1.9.4");
const SIGNING_TIME: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.113549.1.9.5");
const TIMESTAMP_TOKEN: ObjectIdentifier =
    ObjectIdentifier::new_unwrap("1.2.840.113549.1.9.16.2.14");
const TST_INFO: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.113549.1.9.16.1.4");
const SUBJECT_KEY_IDENTIFIER: ObjectIdentifier = ObjectIdentifier::new_unwrap("2.5.29.14");
const AUTHORITY_INFORMATION_ACCESS: ObjectIdentifier =
    ObjectIdentifier::new_unwrap("1.3.6.1.5.5.7.1.1");
const CRL_DISTRIBUTION_POINTS: ObjectIdentifier = ObjectIdentifier::new_unwrap("2.5.29.31");
const OCSP_ACCESS: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.3.6.1.5.5.7.48.1");
const OCSP_BASIC: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.3.6.1.5.5.7.48.1.1");
/// Adobe's revocation archive, a signed attribute (adbe-revocationInfoArchival).
const ADOBE_REVOCATION_ARCHIVE: ObjectIdentifier =
    ObjectIdentifier::new_unwrap("1.2.840.113583.1.1.8");

const SHA1: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.3.14.3.2.26");
const SHA256: ObjectIdentifier = ObjectIdentifier::new_unwrap("2.16.840.1.101.3.4.2.1");
const SHA384: ObjectIdentifier = ObjectIdentifier::new_unwrap("2.16.840.1.101.3.4.2.2");
const SHA512: ObjectIdentifier = ObjectIdentifier::new_unwrap("2.16.840.1.101.3.4.2.3");

const RSA_ENCRYPTION: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.113549.1.1.1");
const RSA_PSS: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.113549.1.1.10");
const SHA1_WITH_RSA: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.113549.1.1.5");
const SHA256_WITH_RSA: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.113549.1.1.11");
const SHA384_WITH_RSA: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.113549.1.1.12");
const SHA512_WITH_RSA: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.113549.1.1.13");
const EC_PUBLIC_KEY: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.10045.2.1");
const ECDSA_WITH_SHA1: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.10045.4.1");
const ECDSA_WITH_SHA256: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.10045.4.3.2");
const ECDSA_WITH_SHA384: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.10045.4.3.3");
const ECDSA_WITH_SHA512: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.10045.4.3.4");
const P256: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.10045.3.1.7");
const P384: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.3.132.0.34");

/// The longest certificate chain followed to a trusted root.
const MAX_CHAIN: usize = 8;

/// The most a single stream of a checked PDF may decompress to: a small
/// file must not exhaust memory (a decompression bomb).
const MAX_STREAM_BYTES: usize = 64 * 1024 * 1024;

const TIME_STAMPING: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.3.6.1.5.5.7.3.8");
const OCSP_SIGNING: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.3.6.1.5.5.7.3.9");

/// Extended key usages that allow signing documents: as Acrobat accepts
/// them, a certificate restricted to other purposes (servers, code) does not
/// sign documents.
pub(crate) const DOCUMENT_SIGNING_PURPOSES: [ObjectIdentifier; 6] = [
    ObjectIdentifier::new_unwrap("2.5.29.37.0"), // any purpose
    ObjectIdentifier::new_unwrap("1.3.6.1.5.5.7.3.36"), // document signing
    ObjectIdentifier::new_unwrap("1.3.6.1.5.5.7.3.4"), // email protection
    ObjectIdentifier::new_unwrap("1.3.6.1.5.5.7.3.2"), // client authentication
    ObjectIdentifier::new_unwrap("1.2.840.113583.1.1.5"), // Adobe Authentic Documents
    ObjectIdentifier::new_unwrap("1.3.6.1.4.1.311.10.3.12"), // Microsoft document signing
];

/// `pdf` read with every stream's decompressed size bounded.
pub(crate) fn load_bounded(pdf: &[u8]) -> Option<Document> {
    let options = lopdf::LoadOptions {
        max_decompressed_size: Some(MAX_STREAM_BYTES),
        ..lopdf::LoadOptions::default()
    };
    Document::load_mem_with_options(pdf, options).ok()
}

/// One signature in a PDF, checked.
#[derive(Clone, Debug, PartialEq)]
pub struct SignatureReport {
    /// The signature field's full name.
    pub field: String,
    /// The signer: the certificate's common name, else the name the PDF
    /// gives.
    pub signer: String,
    /// The signer's organisation, from the certificate.
    pub organization: Option<String>,
    pub reason: Option<String>,
    pub location: Option<String>,
    /// When the signer's computer said it signed (seconds since 1970).
    pub claimed_time: Option<i64>,
    pub timestamp: Option<TimestampReport>,
    /// A document timestamp: a timestamp authority's seal, not a person's.
    pub document_timestamp: bool,
    pub integrity: Integrity,
    pub changes: Changes,
    pub certificate: CertificateReport,
    /// The page the signature's box is on, when it is visible.
    pub page: Option<u32>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TimestampReport {
    /// Seconds since 1970.
    pub time: i64,
    pub authority: String,
    /// The token's signature and its imprint of the signature both match.
    pub valid: bool,
    /// Its authority chains to a trusted root, may issue timestamps and was
    /// in date then: only then does its time count.
    pub trusted: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Integrity {
    /// The signed bytes and the signature both match.
    Intact,
    /// The signed bytes changed, or the signature does not match them.
    Altered,
    /// A signature this app cannot check (an old or unusual format).
    Unsupported(String),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Changes {
    /// The signature covers the whole file.
    None,
    /// Later revisions only added signatures, notes, markup or form entries.
    Permitted { signatures: bool, other: bool },
    /// A later revision changed the pages' content.
    Content,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CertificateReport {
    /// The chain from the signer up, by common name.
    pub chain: Vec<String>,
    /// Every certificate in the chain was within its dates when the
    /// trusted timestamp says it signed, or is now without one.
    pub in_date: bool,
    pub trust: Trust,
    pub revocation: Revocation,
}

/// Whether the chain's certificates were revoked, by OCSP or a revocation
/// list the PDF holds or their issuers publish.
#[derive(Clone, Debug, PartialEq)]
pub enum Revocation {
    /// Every certificate below the root was not revoked when it signed.
    Good,
    /// A certificate was revoked before the signature's proven time (now
    /// without a trusted timestamp), at this time (seconds since 1970).
    Revoked { at: i64 },
    /// No revocation data could be read or fetched.
    NotChecked,
}

/// Fetches revocation data: a URL and, for OCSP, the request to post.
pub type Fetch<'a> = &'a dyn Fn(&str, Option<&[u8]>) -> Option<Vec<u8>>;

/// Revocation data: DER CRLs and OCSP responses.
#[derive(Clone, Default)]
struct RevocationData {
    crls: Vec<Vec<u8>>,
    ocsps: Vec<Vec<u8>>,
}

impl RevocationData {
    fn extend(&mut self, other: &RevocationData) {
        self.crls.extend(other.crls.iter().cloned());
        self.ocsps.extend(other.ocsps.iter().cloned());
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Trust {
    /// The chain ends at one of the trusted roots.
    Trusted,
    /// The chain ends at a root that is not trusted (or a certificate whose
    /// issuer the PDF does not include).
    Untrusted,
    /// The chain is trusted, but the certificate is not for signing
    /// documents (a web server's, say).
    NotForSigning,
}

/// What a signature, or a document's signatures together, amount to.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum Verdict {
    /// Intact, unchanged but for permitted additions, from a trusted
    /// certificate that was in date.
    Valid,
    /// Intact, but the signer's identity cannot be confirmed (an untrusted
    /// or out-of-date certificate), or the signature cannot be checked.
    Unverified,
    /// Altered, or the pages changed after signing.
    Invalid,
}

impl SignatureReport {
    pub fn verdict(&self) -> Verdict {
        match (&self.integrity, &self.changes) {
            (Integrity::Altered, _) | (Integrity::Intact, Changes::Content) => Verdict::Invalid,
            (Integrity::Intact, _)
                if matches!(self.certificate.revocation, Revocation::Revoked { .. }) =>
            {
                Verdict::Invalid
            }
            (Integrity::Unsupported(_), _) => Verdict::Unverified,
            (Integrity::Intact, _)
                if self.certificate.trust != Trust::Trusted || !self.certificate.in_date =>
            {
                Verdict::Unverified
            }
            (Integrity::Intact, _) => Verdict::Valid,
        }
    }
}

/// The worst of the signatures' verdicts; `None` without signatures.
pub fn document_verdict(reports: &[SignatureReport]) -> Option<Verdict> {
    reports.iter().map(SignatureReport::verdict).max()
}

/// The certificates this computer trusts as roots (DER), read once: the
/// macOS Keychain's trust settings, the Windows root store, or the Linux
/// certificate bundle.
pub fn system_trusted_roots() -> &'static [Vec<u8>] {
    static ROOTS: std::sync::OnceLock<Vec<Vec<u8>>> = std::sync::OnceLock::new();
    ROOTS.get_or_init(|| {
        rustls_native_certs::load_native_certs()
            .certs
            .into_iter()
            .map(|certificate| certificate.to_vec())
            .collect()
    })
}

/// Every signature in `pdf`, checked against `trusted_roots` (DER
/// certificates) now. An unreadable PDF has none.
pub fn signatures(pdf: &[u8], trusted_roots: &[Vec<u8>]) -> Vec<SignatureReport> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_secs() as i64);
    signatures_at(pdf, trusted_roots, now)
}

/// [`signatures`] as checked at `now` (seconds since 1970): without a
/// trusted timestamp, a certificate must be in date now, since the time a
/// signer's computer claims proves nothing.
pub fn signatures_at(pdf: &[u8], trusted_roots: &[Vec<u8>], now: i64) -> Vec<SignatureReport> {
    signatures_checked(pdf, trusted_roots, now, None)
}

/// [`signatures_at`], also asking certificates' OCSP responders and
/// revocation lists through `fetch` when the PDF holds no answer.
pub fn signatures_checked(
    pdf: &[u8],
    trusted_roots: &[Vec<u8>],
    now: i64,
    fetch: Option<Fetch<'_>>,
) -> Vec<SignatureReport> {
    let Some(document) = load_bounded(pdf) else {
        return Vec::new();
    };
    let anchors: Vec<RawCertificate> = trusted_roots
        .iter()
        .filter_map(|der| RawCertificate::new(der.clone()))
        .collect();
    let stored = stored_certificates(&document);
    let stored_revocation = stored_revocation(&document);
    let fields = signature_fields(&document);
    let mut reports = Vec::new();
    for field in fields {
        let context = CheckContext {
            anchors: &anchors,
            stored: &stored,
            revocation: &stored_revocation,
            now,
            fetch,
        };
        if let Some(report) = check_field(&document, pdf, &field, &context) {
            reports.push(report);
        }
    }
    reports
}

struct SignatureField {
    name: String,
    /// The field dictionary (which may also be its widget).
    id: Option<ObjectId>,
    widget_ids: Vec<ObjectId>,
    value: Dictionary,
}

fn signature_fields(document: &Document) -> Vec<SignatureField> {
    let mut fields = Vec::new();
    let Ok(catalog) = document.catalog() else {
        return fields;
    };
    let Some(form) = catalog
        .get(b"AcroForm")
        .ok()
        .and_then(|form| dictionary(document, form))
    else {
        return fields;
    };
    let Some(Object::Array(roots)) = form.get(b"Fields").ok().map(|f| resolve(document, f)) else {
        return fields;
    };
    let mut seen = HashSet::new();
    for root in roots {
        walk_field(
            document,
            root,
            String::new(),
            None,
            &mut seen,
            &mut fields,
            0,
        );
    }
    fields
}

fn walk_field(
    document: &Document,
    object: &Object,
    parent_name: String,
    inherited_type: Option<Vec<u8>>,
    seen: &mut HashSet<ObjectId>,
    fields: &mut Vec<SignatureField>,
    depth: usize,
) {
    if depth > 32 {
        return;
    }
    let id = object.as_reference().ok();
    if let Some(id) = id
        && !seen.insert(id)
    {
        return;
    }
    let Some(field) = dictionary(document, object) else {
        return;
    };
    let partial = field
        .get(b"T")
        .ok()
        .and_then(|name| text(resolve(document, name)));
    let name = match (parent_name.is_empty(), partial) {
        (_, None) => parent_name.clone(),
        (true, Some(partial)) => partial,
        (false, Some(partial)) => format!("{parent_name}.{partial}"),
    };
    let kind = field
        .get(b"FT")
        .ok()
        .and_then(|kind| resolve(document, kind).as_name().ok())
        .map(<[u8]>::to_vec)
        .or(inherited_type);
    let kids: Vec<Object> = match field.get(b"Kids").ok().map(|kids| resolve(document, kids)) {
        Some(Object::Array(kids)) => kids.clone(),
        _ => Vec::new(),
    };
    // Kids with their own names are fields; the rest are its widgets.
    let (child_fields, widgets): (Vec<_>, Vec<_>) = kids
        .iter()
        .partition(|kid| dictionary(document, kid).is_some_and(|kid| kid.has(b"T")));
    if kind.as_deref() == Some(b"Sig")
        && let Some(value) = field
            .get(b"V")
            .ok()
            .and_then(|value| dictionary(document, value))
    {
        let mut widget_ids: Vec<ObjectId> = widgets
            .iter()
            .filter_map(|widget| widget.as_reference().ok())
            .collect();
        if let Some(id) = id {
            widget_ids.push(id);
        }
        fields.push(SignatureField {
            name: name.clone(),
            id,
            widget_ids,
            value: value.clone(),
        });
    }
    for child in child_fields {
        walk_field(
            document,
            child,
            name.clone(),
            kind.clone(),
            seen,
            fields,
            depth + 1,
        );
    }
}

struct CheckContext<'a> {
    anchors: &'a [RawCertificate],
    stored: &'a [RawCertificate],
    revocation: &'a RevocationData,
    now: i64,
    fetch: Option<Fetch<'a>>,
}

fn check_field(
    document: &Document,
    pdf: &[u8],
    field: &SignatureField,
    context: &CheckContext<'_>,
) -> Option<SignatureReport> {
    let (anchors, stored, now) = (context.anchors, context.stored, context.now);
    let value = &field.value;
    let name_entry = |key: &[u8]| {
        value
            .get(key)
            .ok()
            .and_then(|entry| text(resolve(document, entry)))
    };
    let sub_filter = value
        .get(b"SubFilter")
        .ok()
        .and_then(|filter| resolve(document, filter).as_name().ok())
        .map(|name| String::from_utf8_lossy(name).into_owned())
        .unwrap_or_default();
    let document_timestamp = value
        .get(b"Type")
        .ok()
        .and_then(|kind| resolve(document, kind).as_name().ok())
        == Some(b"DocTimeStamp".as_slice())
        || sub_filter == "ETSI.RFC3161";
    let claimed_time = name_entry(b"M").and_then(|time| pdf_date(&time));
    let mut report = SignatureReport {
        field: field.name.clone(),
        signer: name_entry(b"Name").unwrap_or_default(),
        organization: None,
        reason: name_entry(b"Reason"),
        location: name_entry(b"Location"),
        claimed_time,
        timestamp: None,
        document_timestamp,
        integrity: Integrity::Unsupported(String::new()),
        changes: Changes::None,
        certificate: CertificateReport {
            chain: Vec::new(),
            in_date: false,
            trust: Trust::Untrusted,
            revocation: Revocation::NotChecked,
        },
        page: signature_page(document, field),
    };

    let Some(ranges) = byte_range(document, value, pdf.len()) else {
        report.integrity = Integrity::Altered;
        return Some(report);
    };
    report.changes = changes_after(document, pdf, ranges.covered_end, field);
    let Some(contents) = signature_contents(pdf, &ranges) else {
        report.integrity = Integrity::Altered;
        return Some(report);
    };
    let signed: [&[u8]; 2] = [
        &pdf[ranges.first.0..ranges.first.1],
        &pdf[ranges.second.0..ranges.second.1],
    ];

    let checked = match sub_filter.as_str() {
        "adbe.pkcs7.detached" | "ETSI.CAdES.detached" => {
            check_signed_data(contents, Content::Detached(&signed))
        }
        "adbe.pkcs7.sha1" => check_signed_data(contents, Content::Sha1OfDocument(&signed)),
        "ETSI.RFC3161" => check_document_timestamp(contents, &signed),
        other => {
            report.integrity = Integrity::Unsupported(format!(
                "signatures of the {other} kind cannot be checked yet"
            ));
            return Some(report);
        }
    };
    let checked = match checked {
        Ok(checked) => checked,
        Err(Failure::Unsupported(reason)) => {
            report.integrity = Integrity::Unsupported(reason);
            return Some(report);
        }
        Err(Failure::Altered) => {
            report.integrity = Integrity::Altered;
            return Some(report);
        }
    };
    report.integrity = if checked.verified {
        Integrity::Intact
    } else {
        Integrity::Altered
    };
    if document_timestamp {
        report.timestamp = checked.own_time.map(|time| TimestampReport {
            time,
            authority: checked
                .signer
                .as_ref()
                .map(RawCertificate::display_name)
                .unwrap_or_default(),
            valid: checked.verified,
            trusted: false,
        });
    } else {
        report.timestamp = checked.timestamp.clone();
    }
    // A timestamp's time counts only from an authority that is trusted.
    let pool: Vec<&RawCertificate> = checked.certificates.iter().chain(stored).collect();
    if let Some(timestamp) = report.timestamp.as_mut()
        && timestamp.valid
    {
        let (authority, certificates) = if document_timestamp {
            (checked.signer.as_ref(), Vec::new())
        } else {
            (
                checked.timestamp_signer.as_ref(),
                checked.timestamp_certificates.iter().collect(),
            )
        };
        timestamp.trusted = authority.is_some_and(|authority| {
            let pool: Vec<&RawCertificate> = certificates
                .into_iter()
                .chain(pool.iter().copied())
                .collect();
            timestamp_authority_trusted(authority, &pool, anchors, timestamp.time)
        });
    }
    if report.claimed_time.is_none() {
        report.claimed_time = checked.signing_time;
    }
    if let Some(signer) = &checked.signer {
        let name = signer.display_name();
        if !name.is_empty() {
            report.signer = name;
        }
        report.organization = signer
            .certificate
            .tbs_certificate()
            .subject()
            .organization()
            .ok()
            .flatten()
            .map(|organization| organization.value().into_owned());
        let (chain, mut trust) = certificate_chain(signer, &pool, anchors);
        let purpose = if document_timestamp {
            signer.has_purpose(&[TIME_STAMPING]) == Some(true)
        } else {
            signer.may_sign_documents()
        };
        if trust == Trust::Trusted && !purpose {
            trust = Trust::NotForSigning;
        }
        let checked_at = report
            .timestamp
            .as_ref()
            .filter(|timestamp| timestamp.valid && timestamp.trusted)
            .map_or(now, |timestamp| timestamp.time);
        let mut revocation_data = context.revocation.clone();
        revocation_data.extend(&checked.revocation);
        let revocation =
            chain_revocation(&chain, &pool, &revocation_data, checked_at, context.fetch);
        report.certificate = CertificateReport {
            chain: chain.iter().map(RawCertificate::display_name).collect(),
            in_date: chain
                .iter()
                .all(|certificate| certificate.valid_at(checked_at)),
            trust,
            revocation,
        };
    }
    Some(report)
}

struct Ranges {
    first: (usize, usize),
    second: (usize, usize),
    covered_end: usize,
}

fn byte_range(document: &Document, value: &Dictionary, length: usize) -> Option<Ranges> {
    let Object::Array(range) = resolve(document, value.get(b"ByteRange").ok()?) else {
        return None;
    };
    let numbers: Vec<usize> = range
        .iter()
        .map(|number| {
            resolve(document, number)
                .as_i64()
                .ok()
                .and_then(|n| usize::try_from(n).ok())
        })
        .collect::<Option<_>>()?;
    let [start, first_length, second_start, second_length] = numbers[..] else {
        return None;
    };
    let first_end = start.checked_add(first_length)?;
    let second_end = second_start.checked_add(second_length)?;
    (start == 0 && first_end < second_start && second_end <= length).then_some(Ranges {
        first: (start, first_end),
        second: (second_start, second_end),
        covered_end: second_end,
    })
}

/// The signature itself: the hex string in the gap the byte range leaves,
/// read from the file (an encrypted PDF does not encrypt it).
fn signature_contents(pdf: &[u8], ranges: &Ranges) -> Option<Vec<u8>> {
    let gap = &pdf[ranges.first.1..ranges.second.0];
    let open = gap.iter().position(|byte| *byte == b'<')?;
    let close = gap.iter().rposition(|byte| *byte == b'>')?;
    if close <= open
        || gap[..open].iter().any(|byte| !byte.is_ascii_whitespace())
        || gap[close + 1..]
            .iter()
            .any(|byte| !byte.is_ascii_whitespace())
    {
        return None;
    }
    let digits: Vec<u8> = gap[open + 1..close]
        .iter()
        .copied()
        .filter(|byte| !byte.is_ascii_whitespace())
        .collect();
    let mut bytes = Vec::with_capacity(digits.len() / 2);
    for pair in digits.chunks(2) {
        let high = (pair[0] as char).to_digit(16)?;
        let low = pair
            .get(1)
            .map_or(Some(0), |low| (*low as char).to_digit(16))?;
        bytes.push((high * 16 + low) as u8);
    }
    // The signature is padded with zeros to fill its reserved space.
    let length = tlv(&bytes).map(|(tlv, _)| tlv.raw.len())?;
    bytes.truncate(length);
    Some(bytes)
}

enum Content<'a> {
    /// The PDF's signed byte ranges themselves.
    Detached(&'a [&'a [u8]]),
    /// Old Acrobat signatures: the SHA-1 of the byte ranges is the content.
    Sha1OfDocument(&'a [&'a [u8]]),
}

enum Failure {
    Unsupported(String),
    Altered,
}

#[derive(Default)]
struct Checked {
    verified: bool,
    signer: Option<RawCertificate>,
    certificates: Vec<RawCertificate>,
    signing_time: Option<i64>,
    timestamp: Option<TimestampReport>,
    /// The signature timestamp's authority and the certificates its token
    /// carries.
    timestamp_signer: Option<RawCertificate>,
    timestamp_certificates: Vec<RawCertificate>,
    /// A timestamp token's own time (for document timestamps).
    own_time: Option<i64>,
    revocation: RevocationData,
}

fn check_signed_data(cms: Vec<u8>, content: Content<'_>) -> Result<Checked, Failure> {
    let parsed = ParsedSignedData::parse(&cms)?;
    let digest_oid = parsed.digest_algorithm;
    let mut checked = Checked {
        certificates: parsed.certificates.clone(),
        ..Checked::default()
    };
    let content_digest = match content {
        Content::Detached(ranges) => digest(digest_oid, ranges).ok_or_else(unsupported_digest)?,
        Content::Sha1OfDocument(ranges) => {
            let document_sha1 = digest(SHA1, ranges).ok_or_else(unsupported_digest)?;
            if parsed.encapsulated.as_deref() != Some(document_sha1.as_slice()) {
                return Err(Failure::Altered);
            }
            digest(digest_oid, &[&document_sha1]).ok_or_else(unsupported_digest)?
        }
    };
    checked.signer = parsed.signer().cloned();
    checked.signing_time = parsed.signing_time;
    checked.revocation = parsed.revocation.clone();
    checked.verified = parsed.verify(&content_digest)?;
    if let Some(token) = &parsed.timestamp_token {
        let (report, signer, certificates) = check_timestamp_token(token, &parsed.signature);
        checked.timestamp = Some(report);
        checked.timestamp_signer = signer;
        checked.timestamp_certificates = certificates;
    }
    Ok(checked)
}

/// A document timestamp (ETSI.RFC3161): the token's imprint covers the byte
/// ranges.
fn check_document_timestamp(token: Vec<u8>, ranges: &[&[u8]]) -> Result<Checked, Failure> {
    let parsed = ParsedSignedData::parse(&token)?;
    let info = parsed.timestamp_info().ok_or(Failure::Altered)?;
    let imprint_matches =
        digest(info.imprint_algorithm, ranges).is_some_and(|imprint| imprint == info.imprint);
    let content_digest = digest(
        parsed.digest_algorithm,
        &[parsed.encapsulated.as_deref().unwrap_or_default()],
    )
    .ok_or_else(unsupported_digest)?;
    let verified = parsed.verify(&content_digest)? && imprint_matches;
    Ok(Checked {
        verified,
        signer: parsed.signer().cloned(),
        certificates: parsed.certificates.clone(),
        own_time: Some(info.time),
        ..Checked::default()
    })
}

/// A signature timestamp token checked against `signature`, with its
/// authority and the certificates it carries.
fn check_timestamp_token(
    token: &[u8],
    signature: &[u8],
) -> (TimestampReport, Option<RawCertificate>, Vec<RawCertificate>) {
    let mut report = TimestampReport {
        time: 0,
        authority: String::new(),
        valid: false,
        trusted: false,
    };
    let Ok(parsed) = ParsedSignedData::parse(token) else {
        return (report, None, Vec::new());
    };
    let Some(info) = parsed.timestamp_info() else {
        return (report, None, Vec::new());
    };
    report.time = info.time;
    report.authority = parsed
        .signer()
        .map(RawCertificate::display_name)
        .unwrap_or_default();
    let imprint_matches =
        digest(info.imprint_algorithm, &[signature]).is_some_and(|imprint| imprint == info.imprint);
    let content = parsed.encapsulated.as_deref().unwrap_or_default();
    report.valid = imprint_matches
        && digest(parsed.digest_algorithm, &[content])
            .is_some_and(|content_digest| parsed.verify(&content_digest).unwrap_or(false));
    let signer = parsed.signer().cloned();
    (report, signer, parsed.certificates.clone())
}

/// Whether a timestamp server's `token` answers a query for `signature`
/// with `nonce`: its imprint and nonce match and its own signature checks
/// out (whether its authority is trusted is the reader's to judge).
pub(crate) fn timestamp_answers(token: &[u8], signature: &[u8], nonce: &[u8]) -> bool {
    let (report, _, _) = check_timestamp_token(token, signature);
    let integer = |bytes: &[u8]| -> Vec<u8> {
        bytes
            .iter()
            .copied()
            .skip_while(|byte| *byte == 0)
            .collect()
    };
    report.valid
        && ParsedSignedData::parse(token)
            .ok()
            .and_then(|parsed| parsed.timestamp_info())
            .and_then(|info| info.nonce)
            .is_some_and(|answered| integer(&answered) == integer(nonce))
}

/// Whether a timestamp `authority` may vouch for `time`: its certificate is
/// for timestamping, chains to a trusted root, and the chain was in date.
fn timestamp_authority_trusted(
    authority: &RawCertificate,
    pool: &[&RawCertificate],
    anchors: &[RawCertificate],
    time: i64,
) -> bool {
    if authority.has_purpose(&[TIME_STAMPING]) != Some(true) {
        return false;
    }
    let (chain, trust) = certificate_chain(authority, pool, anchors);
    trust == Trust::Trusted && chain.iter().all(|certificate| certificate.valid_at(time))
}

fn unsupported_digest() -> Failure {
    Failure::Unsupported("the signature uses a hash this app does not support".into())
}

/// A CMS SignedData with one signer, read from its raw DER.
struct ParsedSignedData {
    digest_algorithm: ObjectIdentifier,
    signature_algorithm: der::asn1::Any,
    /// The signed attributes as signed: their DER with the SET tag.
    signed_attributes: Option<Vec<u8>>,
    message_digest: Option<Vec<u8>>,
    signing_time: Option<i64>,
    signature: Vec<u8>,
    certificates: Vec<RawCertificate>,
    signer_index: Option<usize>,
    encapsulated_type: Option<ObjectIdentifier>,
    encapsulated: Option<Vec<u8>>,
    timestamp_token: Option<Vec<u8>>,
    /// CRLs the SignedData carries and Adobe's archive in its signed
    /// attributes.
    revocation: RevocationData,
}

impl ParsedSignedData {
    fn parse(cms: &[u8]) -> Result<Self, Failure> {
        let malformed = || Failure::Altered;
        let (content_info, _) = tlv(cms).ok_or_else(malformed)?;
        let parts = children(content_info.content).ok_or_else(malformed)?;
        let [content_type, wrapped, ..] = parts.as_slice() else {
            return Err(malformed());
        };
        if ObjectIdentifier::from_der(content_type.raw).ok() != Some(SIGNED_DATA) {
            return Err(Failure::Unsupported(
                "the signature is not CMS signed data".into(),
            ));
        }
        let (signed_data, _) = tlv(wrapped.content).ok_or_else(malformed)?;
        let parts = children(signed_data.content).ok_or_else(malformed)?;
        let encapsulated_info = parts.get(2).ok_or_else(malformed)?;
        let encapsulated_type = children(encapsulated_info.content).and_then(|parts| {
            parts
                .first()
                .and_then(|oid| ObjectIdentifier::from_der(oid.raw).ok())
        });
        let encapsulated = children(encapsulated_info.content)
            .and_then(|parts| parts.get(1).map(|explicit| explicit.content.to_vec()))
            .and_then(|octets| tlv(&octets).map(|(octets, _)| octets.content.to_vec()));
        let certificates: Vec<RawCertificate> = parts
            .iter()
            .find(|part| part.tag == 0xA0)
            .and_then(|set| children(set.content))
            .unwrap_or_default()
            .into_iter()
            .filter(|certificate| certificate.tag == 0x30)
            .filter_map(|certificate| RawCertificate::new(certificate.raw.to_vec()))
            .collect();
        let mut revocation = RevocationData::default();
        if let Some(crls) = parts.iter().find(|part| part.tag == 0xA1) {
            revocation.crls.extend(
                children(crls.content)
                    .unwrap_or_default()
                    .into_iter()
                    .filter(|crl| crl.tag == 0x30)
                    .map(|crl| crl.raw.to_vec()),
            );
        }
        let signer_infos = parts
            .last()
            .filter(|part| part.tag == 0x31)
            .ok_or_else(malformed)?;
        let signer_info = children(signer_infos.content)
            .and_then(|infos| infos.into_iter().next())
            .ok_or_else(malformed)?;
        let fields = children(signer_info.content).ok_or_else(malformed)?;
        let mut fields = fields.into_iter();
        let _version = fields.next().ok_or_else(malformed)?;
        let sid = fields.next().ok_or_else(malformed)?;
        let digest_algorithm = fields
            .next()
            .and_then(|algorithm| algorithm_oid(algorithm.raw))
            .ok_or_else(malformed)?;
        let mut next = fields.next().ok_or_else(malformed)?;
        let mut signed_attributes = None;
        let mut message_digest = None;
        let mut signing_time = None;
        if next.tag == 0xA0 {
            let mut set = next.raw.to_vec();
            set[0] = 0x31;
            for attribute in children(next.content).ok_or_else(malformed)? {
                let parts = children(attribute.content).ok_or_else(malformed)?;
                let (Some(oid), Some(values)) = (parts.first(), parts.get(1)) else {
                    continue;
                };
                let oid = ObjectIdentifier::from_der(oid.raw).ok();
                let first = children(values.content).and_then(|values| values.into_iter().next());
                match (oid, first) {
                    (Some(MESSAGE_DIGEST), Some(value)) if value.tag == 0x04 => {
                        message_digest = Some(value.content.to_vec());
                    }
                    (Some(SIGNING_TIME), Some(value)) => {
                        signing_time = asn1_time(value.tag, value.content);
                    }
                    (Some(ADOBE_REVOCATION_ARCHIVE), Some(value)) => {
                        // RevocationInfoArchival: [0] CRLs, [1] OCSP responses.
                        for part in children(value.content).unwrap_or_default() {
                            let items = children(part.content)
                                .and_then(|wrapped| wrapped.into_iter().next())
                                .and_then(|list| children(list.content))
                                .unwrap_or_default();
                            let raw = items.into_iter().map(|item| item.raw.to_vec());
                            match part.tag {
                                0xA0 => revocation.crls.extend(raw),
                                0xA1 => revocation.ocsps.extend(raw),
                                _ => {}
                            }
                        }
                    }
                    _ => {}
                }
            }
            signed_attributes = Some(set);
            next = fields.next().ok_or_else(malformed)?;
        }
        let signature_algorithm = der::asn1::Any::from_der(next.raw).map_err(|_| malformed())?;
        let signature = fields
            .next()
            .filter(|signature| signature.tag == 0x04)
            .ok_or_else(malformed)?
            .content
            .to_vec();
        let mut timestamp_token = None;
        if let Some(unsigned) = fields.next().filter(|unsigned| unsigned.tag == 0xA1) {
            for attribute in children(unsigned.content).unwrap_or_default() {
                let parts = children(attribute.content).unwrap_or_default();
                if parts
                    .first()
                    .and_then(|oid| ObjectIdentifier::from_der(oid.raw).ok())
                    == Some(TIMESTAMP_TOKEN)
                {
                    timestamp_token = parts
                        .get(1)
                        .and_then(|values| children(values.content))
                        .and_then(|values| values.into_iter().next())
                        .map(|token| token.raw.to_vec());
                }
            }
        }
        let signer_index = certificates
            .iter()
            .position(|certificate| certificate.identified_by(sid.tag, sid.raw, sid.content));
        Ok(Self {
            digest_algorithm,
            signature_algorithm,
            signed_attributes,
            message_digest,
            signing_time,
            signature,
            certificates,
            signer_index,
            encapsulated_type,
            encapsulated,
            timestamp_token,
            revocation,
        })
    }

    /// The timestamp information a timestamp token seals.
    fn timestamp_info(&self) -> Option<TstInfo> {
        if self.encapsulated_type != Some(TST_INFO) {
            return None;
        }
        TstInfo::parse(self.encapsulated.as_deref()?)
    }

    fn signer(&self) -> Option<&RawCertificate> {
        self.signer_index.map(|index| &self.certificates[index])
    }

    /// Whether the signature matches content whose digest is
    /// `content_digest`.
    fn verify(&self, content_digest: &[u8]) -> Result<bool, Failure> {
        let Some(signer) = self.signer() else {
            return Err(Failure::Unsupported(
                "the signature does not include its signer's certificate".into(),
            ));
        };
        let signed_digest = match &self.signed_attributes {
            Some(attributes) => {
                if self.message_digest.as_deref() != Some(content_digest) {
                    return Ok(false);
                }
                digest(self.digest_algorithm, &[attributes]).ok_or_else(unsupported_digest)?
            }
            None => content_digest.to_vec(),
        };
        verify_signature(
            signer.public_key_der(),
            &self.signature_algorithm,
            self.digest_algorithm,
            &signed_digest,
            &self.signature,
        )
    }
}

/// The parts of a timestamp's TSTInfo this needs.
struct TstInfo {
    imprint_algorithm: ObjectIdentifier,
    imprint: Vec<u8>,
    time: i64,
    /// The request's nonce, as the token repeats it.
    nonce: Option<Vec<u8>>,
}

impl TstInfo {
    fn parse(der: &[u8]) -> Option<Self> {
        let (info, _) = tlv(der)?;
        let parts = children(info.content)?;
        // version, policy, messageImprint, serialNumber, genTime, ...
        let imprint = children(parts.get(2)?.content)?;
        let imprint_algorithm = algorithm_oid(imprint.first()?.raw)?;
        let imprint_value = imprint.get(1).filter(|value| value.tag == 0x04)?;
        let time = parts.get(4)?;
        // After genTime: accuracy, ordering, then the nonce (an INTEGER).
        let nonce = parts
            .iter()
            .skip(5)
            .find(|part| part.tag == 0x02)
            .map(|nonce| nonce.content.to_vec());
        Some(Self {
            imprint_algorithm,
            imprint: imprint_value.content.to_vec(),
            time: asn1_time(time.tag, time.content)?,
            nonce,
        })
    }
}

#[derive(Clone)]
struct RawCertificate {
    der: Vec<u8>,
    certificate: Certificate,
}

impl RawCertificate {
    fn new(der: Vec<u8>) -> Option<Self> {
        let certificate = Certificate::from_der(&der).ok()?;
        Some(Self { der, certificate })
    }

    fn display_name(&self) -> String {
        let subject = self.certificate.tbs_certificate().subject();
        subject
            .common_name()
            .ok()
            .flatten()
            .map(|name| name.value().into_owned())
            .or_else(|| {
                subject
                    .organization()
                    .ok()
                    .flatten()
                    .map(|name| name.value().into_owned())
            })
            .unwrap_or_else(|| subject.to_string())
    }

    fn public_key_der(&self) -> Vec<u8> {
        self.certificate
            .tbs_certificate()
            .subject_public_key_info()
            .to_der()
            .unwrap_or_default()
    }

    /// The certificate's to-be-signed part as it was signed.
    fn tbs_der(&self) -> Option<&[u8]> {
        let (certificate, _) = tlv(&self.der)?;
        let (tbs, _) = tlv(certificate.content)?;
        Some(tbs.raw)
    }

    fn identified_by(&self, tag: u8, raw: &[u8], content: &[u8]) -> bool {
        let tbs = self.certificate.tbs_certificate();
        match tag {
            // IssuerAndSerialNumber
            0x30 => {
                let Some(parts) = children(content) else {
                    return false;
                };
                let (Some(issuer), Some(serial)) = (parts.first(), parts.get(1)) else {
                    return false;
                };
                tbs.issuer().to_der().ok().as_deref() == Some(issuer.raw)
                    && tbs.serial_number().to_der().ok().as_deref() == Some(serial.raw)
            }
            // [0] SubjectKeyIdentifier
            0x80 => self.subject_key_identifier().as_deref() == Some(content),
            _ => {
                let _ = raw;
                false
            }
        }
    }

    fn subject_key_identifier(&self) -> Option<Vec<u8>> {
        let extensions = self.certificate.tbs_certificate().extensions()?;
        let extension = extensions
            .iter()
            .find(|extension| extension.extn_id == SUBJECT_KEY_IDENTIFIER)?;
        let (octets, _) = tlv(extension.extn_value.as_bytes())?;
        Some(octets.content.to_vec())
    }

    fn valid_at(&self, time: i64) -> bool {
        let validity = self.certificate.tbs_certificate().validity();
        let start = validity.not_before.to_unix_duration().as_secs() as i64;
        let end = validity.not_after.to_unix_duration().as_secs() as i64;
        (start..=end).contains(&time)
    }

    fn issued(&self, child: &RawCertificate) -> bool {
        let child_tbs = child.certificate.tbs_certificate();
        if child_tbs.issuer() != self.certificate.tbs_certificate().subject() {
            return false;
        }
        let Some(tbs) = child.tbs_der() else {
            return false;
        };
        let Ok(algorithm) = der::asn1::Any::from_der(
            &child
                .certificate
                .signature_algorithm()
                .to_der()
                .unwrap_or_default(),
        ) else {
            return false;
        };
        let Some(digest_oid) = signature_digest(&algorithm) else {
            return false;
        };
        let Some(tbs_digest) = digest(digest_oid, &[tbs]) else {
            return false;
        };
        verify_signature(
            self.public_key_der(),
            &algorithm,
            digest_oid,
            &tbs_digest,
            child.certificate.signature().raw_bytes(),
        )
        .unwrap_or(false)
    }

    fn self_signed(&self) -> bool {
        self.issued(self)
    }

    fn extension<T: const_oid::AssociatedOid + for<'a> der::Decode<'a>>(&self) -> Option<T> {
        self.certificate
            .tbs_certificate()
            .get_extension::<T>()
            .ok()
            .flatten()
            .map(|(_, value)| value)
    }

    /// Whether it may issue a certificate with `below` CA certificates
    /// between it and the end of the chain: a CA whose key may sign
    /// certificates, within its path length. A trust anchor without basic
    /// constraints (an X.509 v1 root) is trusted as a CA.
    fn may_issue(&self, below: usize, anchor: bool) -> bool {
        let constraints = self.extension::<x509_cert::ext::pkix::BasicConstraints>();
        let ca = match &constraints {
            Some(constraints) => {
                constraints.ca
                    && constraints
                        .path_len_constraint
                        .is_none_or(|limit| below <= usize::from(limit))
            }
            None => anchor,
        };
        ca && self
            .extension::<x509_cert::ext::pkix::KeyUsage>()
            .is_none_or(|usage| usage.key_cert_sign())
    }

    fn has_purpose(&self, purposes: &[ObjectIdentifier]) -> Option<bool> {
        self.extension::<x509_cert::ext::pkix::ExtendedKeyUsage>()
            .map(|usage| usage.0.iter().any(|oid| purposes.contains(oid)))
    }

    /// Whether its key may sign documents: digital signature or
    /// non-repudiation, and a document purpose when it names purposes.
    fn may_sign_documents(&self) -> bool {
        self.extension::<x509_cert::ext::pkix::KeyUsage>()
            .is_none_or(|usage| usage.digital_signature() || usage.non_repudiation())
            && self.has_purpose(&DOCUMENT_SIGNING_PURPOSES) != Some(false)
    }
}

/// The chain from `signer` towards a root, and whether a trusted root ends
/// it.
fn certificate_chain(
    signer: &RawCertificate,
    pool: &[&RawCertificate],
    anchors: &[RawCertificate],
) -> (Vec<RawCertificate>, Trust) {
    let mut chain = vec![signer.clone()];
    for _ in 0..MAX_CHAIN {
        let current = chain.last().expect("the chain starts with the signer");
        if anchors.iter().any(|anchor| anchor.der == current.der) {
            return (chain, Trust::Trusted);
        }
        // Only a CA may issue: the certificates between an issuer and the
        // signer count against its path length.
        let below = chain.len() - 1;
        if let Some(anchor) = anchors
            .iter()
            .find(|anchor| anchor.may_issue(below, true) && anchor.issued(current))
        {
            chain.push(anchor.clone());
            return (chain, Trust::Trusted);
        }
        if current.self_signed() {
            break;
        }
        let Some(issuer) = pool.iter().find(|candidate| {
            candidate.der != current.der
                && candidate.may_issue(below, false)
                && candidate.issued(current)
        }) else {
            break;
        };
        let issuer = (*issuer).clone();
        if chain
            .iter()
            .any(|certificate| certificate.der == issuer.der)
        {
            break;
        }
        chain.push(issuer);
    }
    (chain, Trust::Untrusted)
}

/// The streams of a document security store's (`/DSS`) array `key`.
fn stored_streams(document: &Document, key: &[u8]) -> Vec<Vec<u8>> {
    let Some(store) = document
        .catalog()
        .ok()
        .and_then(|catalog| catalog.get(b"DSS").ok())
        .and_then(|store| dictionary(document, store))
    else {
        return Vec::new();
    };
    let Some(Object::Array(items)) = store.get(key).ok().map(|items| resolve(document, items))
    else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|item| match resolve(document, item) {
            Object::Stream(stream) => stream_bytes(stream),
            _ => None,
        })
        .collect()
}

/// A stream's decoded bytes, bounded; its raw bytes when it has no filter.
fn stream_bytes(stream: &lopdf::Stream) -> Option<Vec<u8>> {
    if stream.dict.has(b"Filter") {
        stream
            .decompressed_content_with_limit(MAX_STREAM_BYTES)
            .ok()
    } else {
        Some(stream.content.clone())
    }
}

/// Revocation data a document security store keeps.
fn stored_revocation(document: &Document) -> RevocationData {
    RevocationData {
        crls: stored_streams(document, b"CRLs"),
        ocsps: stored_streams(document, b"OCSPs"),
    }
}

/// Whether any certificate below the root was revoked before `at`.
fn chain_revocation(
    chain: &[RawCertificate],
    pool: &[&RawCertificate],
    data: &RevocationData,
    at: i64,
    fetch: Option<Fetch<'_>>,
) -> Revocation {
    let mut all_good = true;
    // A chain of only a self-signed certificate has nothing to ask, so its
    // status stays unknown rather than "not revoked".
    let mut asked = false;
    for (index, certificate) in chain.iter().enumerate() {
        if certificate.self_signed() {
            continue;
        }
        asked = true;
        let issuer = chain.get(index + 1).or_else(|| {
            pool.iter()
                .copied()
                .find(|candidate| candidate.issued(certificate))
        });
        let Some(issuer) = issuer else {
            all_good = false;
            continue;
        };
        match certificate_status(certificate, issuer, data, at, fetch) {
            Some(Some(revoked)) if revoked <= at => return Revocation::Revoked { at: revoked },
            Some(_) => {}
            None => all_good = false,
        }
    }
    if all_good && asked {
        Revocation::Good
    } else {
        Revocation::NotChecked
    }
}

/// `Some(None)` when `certificate` was not revoked at `at`, `Some(Some(time))`
/// when it was revoked at `time`, `None` when nothing current says.
fn certificate_status(
    certificate: &RawCertificate,
    issuer: &RawCertificate,
    data: &RevocationData,
    at: i64,
    fetch: Option<Fetch<'_>>,
) -> Option<Option<i64>> {
    // Every stored answer counts: one saying it was revoked by `at` wins
    // over others saying it was not.
    let stored: Vec<Option<i64>> = data
        .ocsps
        .iter()
        .filter_map(|response| ocsp_status(response, certificate, issuer, at))
        .chain(
            data.crls
                .iter()
                .filter_map(|crl| crl_status(crl, certificate, issuer, at)),
        )
        .collect();
    if let Some(revoked) = stored
        .iter()
        .flatten()
        .filter(|revoked| **revoked <= at)
        .min()
    {
        return Some(Some(*revoked));
    }
    if let Some(status) = stored.into_iter().next() {
        return Some(status);
    }
    let fetch = fetch?;
    let request = ocsp_request(certificate, issuer);
    for url in certificate.extension_urls(AUTHORITY_INFORMATION_ACCESS, Some(OCSP_ACCESS)) {
        if let Some(response) = request
            .as_deref()
            .and_then(|request| fetch(&url, Some(request)))
            && let Some(status) = ocsp_status(&response, certificate, issuer, at)
        {
            return Some(status);
        }
    }
    for url in certificate.extension_urls(CRL_DISTRIBUTION_POINTS, None) {
        if let Some(crl) = fetch(&url, None)
            && let Some(status) = crl_status(&crl, certificate, issuer, at)
        {
            return Some(status);
        }
    }
    None
}

/// Whether `certificate`'s (DER) key is RSA (else ECDSA).
pub(crate) fn certificate_is_rsa(certificate: &[u8]) -> bool {
    RawCertificate::new(certificate.to_vec()).is_some_and(|certificate| {
        let spki = certificate.public_key_der();
        tlv(&spki)
            .and_then(|(spki, _)| children(spki.content))
            .and_then(|parts| algorithm_oid(parts.first()?.raw))
            .is_some_and(|oid| oid == RSA_ENCRYPTION || oid == RSA_PSS)
    })
}

/// An ECDSA signature as DER, from the fixed-width `r || s` some systems
/// (Windows CNG) return.
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) fn ecdsa_der_from_raw(raw: &[u8]) -> Vec<u8> {
    let half = raw.len() / 2;
    let integer = |bytes: &[u8]| {
        let trimmed: Vec<u8> = bytes
            .iter()
            .copied()
            .skip_while(|byte| *byte == 0)
            .collect();
        let mut content = if trimmed.first().is_some_and(|byte| byte & 0x80 != 0) {
            vec![0]
        } else {
            Vec::new()
        };
        content.extend(if trimmed.is_empty() { vec![0] } else { trimmed });
        let mut out = vec![0x02, content.len() as u8];
        out.extend(content);
        out
    };
    let body = [integer(&raw[..half]), integer(&raw[half..])].concat();
    let mut out = vec![0x30];
    if body.len() < 0x80 {
        out.push(body.len() as u8);
    } else {
        out.extend([0x81, body.len() as u8]);
    }
    out.extend(body);
    out
}

/// Where `certificate` (DER) says its revocation status is: OCSP
/// responders, then revocation lists.
pub(crate) fn revocation_urls(certificate: &[u8]) -> (Vec<String>, Vec<String>) {
    let Some(certificate) = RawCertificate::new(certificate.to_vec()) else {
        return (Vec::new(), Vec::new());
    };
    (
        certificate.extension_urls(AUTHORITY_INFORMATION_ACCESS, Some(OCSP_ACCESS)),
        certificate.extension_urls(CRL_DISTRIBUTION_POINTS, None),
    )
}

/// Whether `issuer` (DER) issued `certificate` (DER), its signature checked.
pub(crate) fn issued_by(certificate: &[u8], issuer: &[u8]) -> bool {
    match (
        RawCertificate::new(certificate.to_vec()),
        RawCertificate::new(issuer.to_vec()),
    ) {
        (Some(certificate), Some(issuer)) => issuer.issued(&certificate),
        _ => false,
    }
}

/// Whether `certificate` (DER) is a self-signed root.
pub(crate) fn self_signed(certificate: &[u8]) -> bool {
    RawCertificate::new(certificate.to_vec()).is_some_and(|certificate| certificate.self_signed())
}

/// The certificates a CMS SignedData (such as a timestamp token) carries.
pub(crate) fn signed_data_certificates(cms: &[u8]) -> Vec<Vec<u8>> {
    ParsedSignedData::parse(cms)
        .map(|parsed| {
            parsed
                .certificates
                .into_iter()
                .map(|certificate| certificate.der)
                .collect()
        })
        .unwrap_or_default()
}

/// The certificates of the timestamp token in a signature's CMS, for tests.
#[cfg(test)]
pub(crate) fn timestamp_token_certificates(cms: &[u8]) -> Vec<Vec<u8>> {
    let cms = match tlv(cms) {
        Some((sequence, _)) => sequence.raw.to_vec(),
        None => return Vec::new(),
    };
    ParsedSignedData::parse(&cms)
        .ok()
        .and_then(|parsed| parsed.timestamp_token)
        .map(|token| signed_data_certificates(&token))
        .unwrap_or_default()
}

/// An OCSP request for `certificate` (RFC 6960), its CertID by SHA-1 as
/// responders universally accept.
pub(crate) fn ocsp_request_for(certificate: &[u8], issuer: &[u8]) -> Option<Vec<u8>> {
    ocsp_request(
        &RawCertificate::new(certificate.to_vec())?,
        &RawCertificate::new(issuer.to_vec())?,
    )
}

fn ocsp_request(certificate: &RawCertificate, issuer: &RawCertificate) -> Option<Vec<u8>> {
    let encode = |tag: u8, content: &[u8]| -> Vec<u8> {
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
    };
    let sha1_algorithm = encode(0x30, &[SHA1.to_der().ok()?, vec![0x05, 0x00]].concat());
    let cert_id = encode(
        0x30,
        &[
            sha1_algorithm,
            encode(0x04, &digest(SHA1, &[issuer.subject_der()?])?),
            encode(0x04, &digest(SHA1, &[issuer.public_key_bits()?])?),
            certificate.serial_der()?.to_vec(),
        ]
        .concat(),
    );
    let request = encode(0x30, &cert_id);
    let request_list = encode(0x30, &request);
    let tbs_request = encode(0x30, &request_list);
    Some(encode(0x30, &tbs_request))
}

/// What an OCSP response says of `certificate`, when it answers for it and
/// its signature checks out.
/// Whether a "not revoked" answer covers `at`: given at or after it (a
/// revocation is permanent), or still current then.
fn current_at(this_update: i64, next_update: Option<i64>, at: i64) -> bool {
    this_update >= at || next_update.is_some_and(|next| next >= at)
}

/// What an OCSP response says of `certificate` at `at`, when `issuer`
/// signed it or certified a responder for it.
fn ocsp_status(
    response: &[u8],
    certificate: &RawCertificate,
    issuer: &RawCertificate,
    at: i64,
) -> Option<Option<i64>> {
    // An OCSPResponse wraps a BasicOCSPResponse; the DSS may hold either.
    let basic = {
        let (outer, _) = tlv(response)?;
        let parts = children(outer.content)?;
        if parts.first()?.tag == 0x0A {
            // OCSPResponse: status 0 (successful), [0] responseBytes.
            if parts.first()?.content != [0] {
                return None;
            }
            let bytes = children(parts.get(1)?.content)?.into_iter().next()?;
            let bytes = children(bytes.content)?;
            if ObjectIdentifier::from_der(bytes.first()?.raw).ok()? != OCSP_BASIC {
                return None;
            }
            bytes.get(1)?.content.to_vec()
        } else {
            response.to_vec()
        }
    };
    let (basic_tlv, _) = tlv(&basic)?;
    let parts = children(basic_tlv.content)?;
    let tbs = parts.first()?;
    let algorithm = der::asn1::Any::from_der(parts.get(1)?.raw).ok()?;
    let signature = parts.get(2)?.content.get(1..)?;
    let embedded: Vec<RawCertificate> = parts
        .get(3)
        .and_then(|explicit| children(explicit.content))
        .and_then(|wrapped| wrapped.into_iter().next())
        .and_then(|list| children(list.content))
        .unwrap_or_default()
        .into_iter()
        .filter_map(|certificate| RawCertificate::new(certificate.raw.to_vec()))
        .collect();
    let digest_oid = signature_digest(&algorithm)?;
    let tbs_digest = digest(digest_oid, &[tbs.raw])?;
    // Signed by the issuer, or by a responder the issuer certified for
    // answering (RFC 6960 4.2.2.2).
    let signed_by = |signer: &RawCertificate| {
        verify_signature(
            signer.public_key_der(),
            &algorithm,
            digest_oid,
            &tbs_digest,
            signature,
        )
        .unwrap_or(false)
    };
    let trusted = signed_by(issuer)
        || embedded.iter().any(|responder| {
            responder.has_purpose(&[OCSP_SIGNING]) == Some(true)
                && issuer.issued(responder)
                && signed_by(responder)
        });
    if !trusted {
        return None;
    }
    let data = children(tbs.content)?;
    let responses = data.iter().find(|part| part.tag == 0x30)?;
    for single in children(responses.content)? {
        let fields = children(single.content)?;
        let cert_id = children(fields.first()?.content)?;
        let hash_oid = algorithm_oid(cert_id.first()?.raw)?;
        let matches = cert_id.get(3)?.raw == certificate.serial_der()?
            && cert_id.get(1)?.content == digest(hash_oid, &[issuer.subject_der()?])?.as_slice()
            && cert_id.get(2)?.content
                == digest(hash_oid, &[issuer.public_key_bits()?])?.as_slice();
        if !matches {
            continue;
        }
        let status = fields.get(1)?;
        let this_update = fields
            .get(2)
            .filter(|time| time.tag == 0x18)
            .and_then(|time| asn1_time(time.tag, time.content))?;
        let next_update = fields
            .iter()
            .skip(3)
            .find(|field| field.tag == 0xA0)
            .and_then(|field| children(field.content)?.into_iter().next())
            .and_then(|time| asn1_time(time.tag, time.content));
        return match status.tag {
            0x80 if current_at(this_update, next_update, at) => Some(None),
            0x80 => None,
            0xA1 => {
                let time = children(status.content)?.into_iter().next()?;
                Some(Some(asn1_time(time.tag, time.content)?))
            }
            _ => None,
        };
    }
    None
}

/// What a CRL says of `certificate`, when `issuer` issued and signed it.
fn crl_status(
    crl: &[u8],
    certificate: &RawCertificate,
    issuer: &RawCertificate,
    at: i64,
) -> Option<Option<i64>> {
    let (list, _) = tlv(crl)?;
    let parts = children(list.content)?;
    let tbs = parts.first()?;
    let algorithm = der::asn1::Any::from_der(parts.get(1)?.raw).ok()?;
    let signature = parts.get(2)?.content.get(1..)?;
    let digest_oid = signature_digest(&algorithm)?;
    let verified = verify_signature(
        issuer.public_key_der(),
        &algorithm,
        digest_oid,
        &digest(digest_oid, &[tbs.raw])?,
        signature,
    )
    .unwrap_or(false);
    if !verified {
        return None;
    }
    let mut fields = children(tbs.content)?.into_iter().peekable();
    if fields.peek()?.tag == 0x02 {
        fields.next(); // version
    }
    fields.next(); // signature algorithm
    let crl_issuer = fields.next()?;
    if Some(crl_issuer.raw) != issuer.subject_der() {
        return None;
    }
    let this_update = fields
        .next()
        .and_then(|time| asn1_time(time.tag, time.content))?;
    let next_update = fields
        .peek()
        .filter(|time| matches!(time.tag, 0x17 | 0x18))
        .and_then(|time| asn1_time(time.tag, time.content));
    let serial = certificate.serial_der()?;
    for field in fields {
        if field.tag != 0x30 {
            continue;
        }
        // nextUpdate is a time; revokedCertificates is a SEQUENCE OF.
        for entry in children(field.content).unwrap_or_default() {
            let Some(parts) = children(entry.content) else {
                continue;
            };
            if parts.first().map(|serial| serial.raw) == Some(serial) {
                let time = parts.get(1)?;
                return Some(Some(asn1_time(time.tag, time.content)?));
            }
        }
    }
    current_at(this_update, next_update, at).then_some(None)
}

impl RawCertificate {
    fn tbs_parts(&self) -> Option<(Vec<Tlv<'_>>, usize)> {
        let (certificate, _) = tlv(&self.der)?;
        let (tbs, _) = tlv(certificate.content)?;
        let parts = children(tbs.content)?;
        let offset = usize::from(parts.first()?.tag == 0xA0);
        Some((parts, offset))
    }

    fn serial_der(&self) -> Option<&[u8]> {
        let (parts, offset) = self.tbs_parts()?;
        parts.get(offset).map(|serial| serial.raw)
    }

    fn subject_der(&self) -> Option<&[u8]> {
        let (parts, offset) = self.tbs_parts()?;
        parts.get(offset + 4).map(|subject| subject.raw)
    }

    /// The subject public key's bits, as OCSP hashes them.
    fn public_key_bits(&self) -> Option<&[u8]> {
        let (parts, offset) = self.tbs_parts()?;
        let spki = children(parts.get(offset + 5)?.content)?;
        spki.get(1)?.content.get(1..)
    }

    /// The http URLs in extension `extension`, under access method `method`
    /// for authority information access.
    fn extension_urls(
        &self,
        extension: ObjectIdentifier,
        method: Option<ObjectIdentifier>,
    ) -> Vec<String> {
        let Some(extensions) = self.certificate.tbs_certificate().extensions() else {
            return Vec::new();
        };
        let Some(found) = extensions.iter().find(|found| found.extn_id == extension) else {
            return Vec::new();
        };
        let mut urls = Vec::new();
        let mut pending = vec![found.extn_value.as_bytes().to_vec()];
        while let Some(bytes) = pending.pop() {
            for item in children(&bytes).unwrap_or_default() {
                // An AccessDescription (method OID, location) for another
                // access method.
                if let Some(method) = method
                    && item.tag == 0x30
                    && let Some(parts) = children(item.content)
                    && let Some(oid) = parts.first().filter(|oid| oid.tag == 0x06)
                    && ObjectIdentifier::from_der(oid.raw).ok() != Some(method)
                {
                    continue;
                }
                if item.tag == 0x86 {
                    if let Ok(url) = std::str::from_utf8(item.content)
                        && url.starts_with("http")
                    {
                        urls.push(url.to_owned());
                    }
                } else if item.tag & 0x20 != 0 {
                    pending.push(item.content.to_vec());
                }
            }
        }
        urls
    }
}

/// Certificates a document security store (`/DSS`) keeps for validation.
fn stored_certificates(document: &Document) -> Vec<RawCertificate> {
    let Some(store) = document
        .catalog()
        .ok()
        .and_then(|catalog| catalog.get(b"DSS").ok())
        .and_then(|store| dictionary(document, store))
    else {
        return Vec::new();
    };
    let Some(Object::Array(certificates)) = store.get(b"Certs").ok().map(|c| resolve(document, c))
    else {
        return Vec::new();
    };
    certificates
        .iter()
        .filter_map(|certificate| match resolve(document, certificate) {
            Object::Stream(stream) => stream_bytes(stream),
            _ => None,
        })
        .filter_map(RawCertificate::new)
        .collect()
}

fn verify_signature(
    public_key: Vec<u8>,
    algorithm: &der::asn1::Any,
    digest_oid: ObjectIdentifier,
    hashed: &[u8],
    signature: &[u8],
) -> Result<bool, Failure> {
    use rsa::pkcs8::DecodePublicKey as _;
    let algorithm_der = algorithm.to_der().unwrap_or_default();
    let algorithm_oid = algorithm_oid(&algorithm_der).ok_or(Failure::Altered)?;
    let key_algorithm = tlv(&public_key)
        .and_then(|(spki, _)| children(spki.content))
        .and_then(|parts| {
            parts
                .first()
                .and_then(|algorithm| children(algorithm.content))
        })
        .ok_or(Failure::Altered)?;
    let key_oid = key_algorithm
        .first()
        .and_then(|oid| ObjectIdentifier::from_der(oid.raw).ok())
        .ok_or(Failure::Altered)?;
    let unsupported =
        || Failure::Unsupported("the signature uses a key this app does not support".into());
    if key_oid == RSA_ENCRYPTION || key_oid == RSA_PSS {
        let key = rsa::RsaPublicKey::from_public_key_der(&public_key).map_err(|_| unsupported())?;
        let verified = if algorithm_oid == RSA_PSS {
            let salt = pss_salt_length(&algorithm_der);
            match digest_oid {
                SHA1 => key.verify(
                    rsa::Pss::<sha1::Sha1>::new_with_salt(salt),
                    hashed,
                    signature,
                ),
                SHA256 => key.verify(
                    rsa::Pss::<signature_sha2::Sha256>::new_with_salt(salt),
                    hashed,
                    signature,
                ),
                SHA384 => key.verify(
                    rsa::Pss::<signature_sha2::Sha384>::new_with_salt(salt),
                    hashed,
                    signature,
                ),
                SHA512 => key.verify(
                    rsa::Pss::<signature_sha2::Sha512>::new_with_salt(salt),
                    hashed,
                    signature,
                ),
                _ => return Err(unsupported_digest()),
            }
        } else {
            let scheme = match digest_oid {
                SHA1 => rsa::Pkcs1v15Sign::new::<sha1::Sha1>(),
                SHA256 => rsa::Pkcs1v15Sign::new::<signature_sha2::Sha256>(),
                SHA384 => rsa::Pkcs1v15Sign::new::<signature_sha2::Sha384>(),
                SHA512 => rsa::Pkcs1v15Sign::new::<signature_sha2::Sha512>(),
                _ => return Err(unsupported_digest()),
            };
            key.verify(scheme, hashed, signature)
        };
        return Ok(verified.is_ok());
    }
    if key_oid == EC_PUBLIC_KEY {
        use p256::ecdsa::signature::hazmat::PrehashVerifier as _;
        let curve = key_algorithm
            .get(1)
            .and_then(|curve| ObjectIdentifier::from_der(curve.raw).ok());
        let point = tlv(&public_key)
            .and_then(|(spki, _)| children(spki.content))
            .and_then(|parts| {
                parts
                    .get(1)
                    .map(|bits| bits.content.get(1..).unwrap_or_default().to_vec())
            })
            .ok_or(Failure::Altered)?;
        return Ok(match curve {
            Some(P256) => {
                let key = p256::ecdsa::VerifyingKey::from_sec1_bytes(&point)
                    .map_err(|_| unsupported())?;
                p256::ecdsa::Signature::from_der(signature)
                    .is_ok_and(|signature| key.verify_prehash(hashed, &signature).is_ok())
            }
            Some(P384) => {
                let key = p384::ecdsa::VerifyingKey::from_sec1_bytes(&point)
                    .map_err(|_| unsupported())?;
                p384::ecdsa::Signature::from_der(signature)
                    .is_ok_and(|signature| key.verify_prehash(hashed, &signature).is_ok())
            }
            _ => return Err(unsupported()),
        });
    }
    Err(unsupported())
}

/// The digest a certificate's signature algorithm names.
fn signature_digest(algorithm: &der::asn1::Any) -> Option<ObjectIdentifier> {
    let der = algorithm.to_der().ok()?;
    let oid = algorithm_oid(&der)?;
    Some(match oid {
        SHA1_WITH_RSA | ECDSA_WITH_SHA1 => SHA1,
        SHA256_WITH_RSA | ECDSA_WITH_SHA256 => SHA256,
        SHA384_WITH_RSA | ECDSA_WITH_SHA384 => SHA384,
        SHA512_WITH_RSA | ECDSA_WITH_SHA512 => SHA512,
        RSA_PSS => pss_digest(&der).unwrap_or(SHA1),
        _ => return None,
    })
}

/// RSASSA-PSS parameters: [0] hash algorithm (SHA-1 by default) and
/// [2] salt length (20 by default).
fn pss_parameters(algorithm: &[u8]) -> Vec<Tlv<'_>> {
    tlv(algorithm)
        .and_then(|(algorithm, _)| children(algorithm.content))
        .and_then(|parts| {
            parts
                .get(1)
                .and_then(|parameters| children(parameters.content))
        })
        .unwrap_or_default()
}

fn pss_digest(algorithm: &[u8]) -> Option<ObjectIdentifier> {
    let parameters = pss_parameters(algorithm);
    let hash = parameters.iter().find(|parameter| parameter.tag == 0xA0)?;
    algorithm_oid(tlv(hash.content)?.0.raw)
}

fn pss_salt_length(algorithm: &[u8]) -> usize {
    pss_parameters(algorithm)
        .iter()
        .find(|parameter| parameter.tag == 0xA2)
        .and_then(|salt| tlv(salt.content))
        .filter(|(integer, _)| integer.tag == 0x02 && integer.content.len() <= 4)
        .map_or(20, |(integer, _)| {
            integer
                .content
                .iter()
                .fold(0_usize, |value, byte| value << 8 | usize::from(*byte))
        })
}

fn digest(oid: ObjectIdentifier, parts: &[&[u8]]) -> Option<Vec<u8>> {
    use signature_sha2::Digest as _;
    fn run<D: signature_sha2::Digest>(parts: &[&[u8]]) -> Vec<u8> {
        let mut hasher = D::new();
        for part in parts {
            hasher.update(part);
        }
        hasher.finalize().to_vec()
    }
    let _ = signature_sha2::Sha256::new;
    Some(match oid {
        SHA1 => run::<sha1::Sha1>(parts),
        SHA256 => run::<signature_sha2::Sha256>(parts),
        SHA384 => run::<signature_sha2::Sha384>(parts),
        SHA512 => run::<signature_sha2::Sha512>(parts),
        _ => return None,
    })
}

fn algorithm_oid(algorithm: &[u8]) -> Option<ObjectIdentifier> {
    let (algorithm, _) = tlv(algorithm)?;
    let (oid, _) = tlv(algorithm.content)?;
    ObjectIdentifier::from_der(oid.raw).ok()
}

/// What later revisions changed, from the objects they replaced.
/// What changed after the signed revision (the bytes up to `covered_end`):
/// every object of the final document is compared with the same object in
/// that revision, so an update cannot hide by pointing objects back at
/// older bytes. Pages and the page tree are recognised by structure, not by
/// what an object says it is.
fn changes_after(
    document: &Document,
    pdf: &[u8],
    covered_end: usize,
    signed: &SignatureField,
) -> Changes {
    if pdf[covered_end..]
        .iter()
        .all(|byte| byte.is_ascii_whitespace() || *byte == 0)
    {
        return Changes::None;
    }
    let Some(earlier) = load_bounded(&pdf[..covered_end]) else {
        return Changes::Content;
    };
    let tree_now = PageTree::of(document);
    let tree_then = PageTree::of(&earlier);
    // What the pages draw, then or now: content, resources (inherited
    // too) and the page tree itself.
    let mut drawn = page_content_objects(document);
    drawn.extend(page_content_objects(&earlier));
    drawn.extend(tree_now.nodes.iter().chain(&tree_then.nodes));
    // Earlier signatures: their fields, widgets, values and appearances.
    let signatures_then = signed_signature_objects(&earlier);
    let catalog_ids: BTreeSet<ObjectId> = [document, &earlier]
        .iter()
        .filter_map(|document| document.trailer.get(b"Root").ok()?.as_reference().ok())
        .collect();
    let info_ids: BTreeSet<ObjectId> = [document, &earlier]
        .iter()
        .filter_map(|document| document.trailer.get(b"Info").ok()?.as_reference().ok())
        .collect();
    let form_ids: BTreeSet<ObjectId> = [document, &earlier]
        .iter()
        .filter_map(|document| {
            document
                .catalog()
                .ok()?
                .get(b"AcroForm")
                .ok()?
                .as_reference()
                .ok()
        })
        .collect();
    let field_list_ids: BTreeSet<ObjectId> = [document, &earlier]
        .iter()
        .filter_map(|document| {
            let form = dictionary(document, document.catalog().ok()?.get(b"AcroForm").ok()?)?;
            form.get(b"Fields").ok()?.as_reference().ok()
        })
        .collect();
    let annotation_lists = tree_now.annotation_lists(document);
    let annotation_lists_then = tree_then.annotation_lists(&earlier);

    let mut signatures = false;
    let mut other = false;
    for (id, object) in &document.objects {
        let previous = earlier.objects.get(id);
        if previous == Some(object) {
            continue;
        }
        if drawn.contains(id) || signatures_then.contains(id) {
            return Changes::Content;
        }
        if tree_now.leaves.contains(id) || tree_then.leaves.contains(id) {
            // A page may gain annotations; nothing else of it may change.
            let (Some(Object::Dictionary(before)), Object::Dictionary(after)) = (previous, object)
            else {
                return Changes::Content;
            };
            if differs_beyond(before, after, &[b"Annots"]) {
                return Changes::Content;
            }
            if annotations_removed(
                before.get(b"Annots").ok(),
                &earlier,
                after.get(b"Annots").ok(),
                document,
            ) {
                other = true;
            }
            continue;
        }
        if annotation_lists.contains(id) || annotation_lists_then.contains(id) {
            if annotations_removed(previous, &earlier, Some(object), document) {
                other = true;
            }
            continue;
        }
        if catalog_ids.contains(id) {
            let (Some(Object::Dictionary(before)), Object::Dictionary(after)) = (previous, object)
            else {
                return Changes::Content;
            };
            // Validation data, signature permissions and the form are about
            // signatures and fields, and the version and its extensions
            // are bookkeeping; the rest (open actions, layers, names) can
            // change what is shown.
            if differs_beyond(
                before,
                after,
                &[
                    b"DSS",
                    b"Perms",
                    b"AcroForm",
                    b"Metadata",
                    b"Version",
                    b"Extensions",
                ],
            ) {
                return Changes::Content;
            }
            if before.get(b"DSS").ok() != after.get(b"DSS").ok()
                || before.get(b"Perms").ok() != after.get(b"Perms").ok()
            {
                signatures = true;
            }
            if let (Ok(Object::Dictionary(form_before)), Ok(Object::Dictionary(form_after))) =
                (before.get(b"AcroForm"), after.get(b"AcroForm"))
                && differs_beyond(
                    form_before,
                    form_after,
                    &[b"Fields", b"SigFlags", b"DR", b"DA"],
                )
            {
                other = true;
            }
            continue;
        }
        if info_ids.contains(id) {
            continue;
        }
        if form_ids.contains(id) {
            let (Some(Object::Dictionary(before)), Object::Dictionary(after)) = (previous, object)
            else {
                other = true;
                continue;
            };
            if differs_beyond(before, after, &[b"Fields", b"SigFlags", b"DR", b"DA"]) {
                other = true;
            }
            continue;
        }
        if field_list_ids.contains(id) {
            if annotations_removed(previous, &earlier, Some(object), document) {
                other = true;
            }
            continue;
        }
        let dictionary = object_dictionary(object);
        let kind = dictionary
            .and_then(|dictionary| dictionary.get(b"Type").ok())
            .and_then(|kind| kind.as_name().ok());
        if matches!(kind, Some(b"XRef" | b"ObjStm" | b"Metadata")) {
            continue;
        }
        let Some(dictionary) = dictionary else {
            // An existing array, string or stream nothing above explains.
            if previous.is_some() {
                other = true;
            }
            continue;
        };
        // A new field named like the signed one could stand in for it (a
        // known attack on signed PDFs).
        if dictionary.has(b"T")
            && Some(*id) != signed.id
            && field_name(document, dictionary) == signed.name
        {
            return Changes::Content;
        }
        if matches!(kind, Some(b"Sig" | b"DocTimeStamp")) {
            if previous.is_some() {
                return Changes::Content;
            }
            signatures = true;
        } else if matches!(kind, Some(b"DSS" | b"VRI")) || signature_field(document, dictionary) {
            signatures = true;
        } else if dictionary.has(b"FT")
            || (dictionary.has(b"Subtype") && dictionary.has(b"Rect"))
            || previous.is_some()
        {
            // A form value, an annotation, or something else that existed.
            other = true;
        }
        // Otherwise a new object (an appearance, a font): it counts as
        // whatever refers to it.
    }
    // Objects the signed revision had that are gone or unreadable now.
    for id in earlier.objects.keys() {
        if document.objects.contains_key(id) {
            continue;
        }
        if drawn.contains(id) || signatures_then.contains(id) || catalog_ids.contains(id) {
            return Changes::Content;
        }
        other = true;
    }
    Changes::Permitted { signatures, other }
}

/// Whether two dictionaries differ in any key but `allowed`.
fn differs_beyond(before: &Dictionary, after: &Dictionary, allowed: &[&[u8]]) -> bool {
    before.iter().chain(after.iter()).any(|(key, _)| {
        !allowed.contains(&key.as_slice()) && before.get(key).ok() != after.get(key).ok()
    })
}

/// Whether a list (of annotations or fields) lost an entry it had.
fn annotations_removed(
    before: Option<&Object>,
    earlier: &Document,
    after: Option<&Object>,
    document: &Document,
) -> bool {
    let items = |object: Option<&Object>, document: &Document| -> Vec<Object> {
        match object.map(|object| resolve(document, object)) {
            Some(Object::Array(items)) => items.clone(),
            _ => Vec::new(),
        }
    };
    let now = items(after, document);
    items(before, earlier)
        .iter()
        .any(|item| !now.contains(item))
}

/// A document's page tree: its pages, and the nodes above them.
struct PageTree {
    leaves: BTreeSet<ObjectId>,
    nodes: BTreeSet<ObjectId>,
}

impl PageTree {
    fn of(document: &Document) -> Self {
        let mut tree = PageTree {
            leaves: BTreeSet::new(),
            nodes: BTreeSet::new(),
        };
        let Some(root) = document
            .catalog()
            .ok()
            .and_then(|catalog| catalog.get(b"Pages").ok())
            .and_then(|pages| pages.as_reference().ok())
        else {
            return tree;
        };
        let mut pending = vec![(root, 0usize)];
        while let Some((id, depth)) = pending.pop() {
            if depth > 64 || tree.nodes.contains(&id) || tree.leaves.contains(&id) {
                continue;
            }
            let Ok(node) = document.get_dictionary(id) else {
                continue;
            };
            match node.get(b"Kids").ok().map(|kids| resolve(document, kids)) {
                Some(Object::Array(kids)) => {
                    tree.nodes.insert(id);
                    if let Ok(kids_id) = node.get(b"Kids").and_then(Object::as_reference) {
                        tree.nodes.insert(kids_id);
                    }
                    pending.extend(
                        kids.iter()
                            .filter_map(|kid| kid.as_reference().ok())
                            .map(|kid| (kid, depth + 1)),
                    );
                }
                _ => {
                    tree.leaves.insert(id);
                }
            }
        }
        tree
    }

    /// The pages' annotation lists that are objects of their own.
    fn annotation_lists(&self, document: &Document) -> BTreeSet<ObjectId> {
        self.leaves
            .iter()
            .filter_map(|page| {
                document
                    .get_dictionary(*page)
                    .ok()?
                    .get(b"Annots")
                    .ok()?
                    .as_reference()
                    .ok()
            })
            .collect()
    }
}

/// The objects of the signatures a document already has: their fields,
/// widgets, signature values and appearances.
fn signed_signature_objects(document: &Document) -> BTreeSet<ObjectId> {
    let mut roots: Vec<Object> = Vec::new();
    for field in signature_fields(document) {
        for id in field.id.iter().chain(&field.widget_ids) {
            roots.push(Object::Reference(*id));
        }
    }
    let mut found = reachable(document, roots.iter().collect(), &[b"Parent", b"P"]);
    for root in &roots {
        if let Ok(id) = root.as_reference() {
            found.insert(id);
        }
    }
    found
}

/// A field's full name, from its own and its parents' partial names.
fn field_name(document: &Document, field: &Dictionary) -> String {
    let mut parts = Vec::new();
    let mut current = Some(field);
    for _ in 0..32 {
        let Some(field) = current else {
            break;
        };
        if let Some(part) = field
            .get(b"T")
            .ok()
            .and_then(|name| text(resolve(document, name)))
        {
            parts.push(part);
        }
        current = field
            .get(b"Parent")
            .ok()
            .and_then(|parent| dictionary(document, parent));
    }
    parts.reverse();
    parts.join(".")
}

/// A signature field, or a widget of one.
fn signature_field(document: &Document, dictionary: &Dictionary) -> bool {
    let mut current = Some(dictionary);
    for _ in 0..32 {
        let Some(field) = current else {
            return false;
        };
        if let Ok(kind) = field.get(b"FT") {
            return resolve(document, kind).as_name().ok() == Some(b"Sig".as_slice());
        }
        current = field
            .get(b"Parent")
            .ok()
            .and_then(|parent| self::dictionary(document, parent));
    }
    false
}

/// Every object the pages draw from: their content streams and resources
/// (their own or inherited), but not their annotations.
fn page_content_objects(document: &Document) -> BTreeSet<ObjectId> {
    let tree = PageTree::of(document);
    let mut roots: Vec<&Object> = Vec::new();
    for id in tree.leaves.iter().chain(&tree.nodes) {
        let Ok(page) = document.get_dictionary(*id) else {
            continue;
        };
        for key in [b"Contents".as_slice(), b"Resources"] {
            if let Ok(object) = page.get(key) {
                roots.push(object);
            }
        }
    }
    reachable(document, roots, &[])
}

/// The objects reachable from `roots`, not following keys in `skip`.
fn reachable(document: &Document, roots: Vec<&Object>, skip: &[&[u8]]) -> BTreeSet<ObjectId> {
    let mut found = BTreeSet::new();
    let mut pending = roots;
    while let Some(object) = pending.pop() {
        match object {
            Object::Reference(id) => {
                if found.insert(*id)
                    && let Ok(object) = document.get_object(*id)
                {
                    pending.push(object);
                }
            }
            Object::Array(items) => pending.extend(items.iter()),
            Object::Dictionary(dictionary) => pending.extend(
                dictionary
                    .iter()
                    .filter(|(key, _)| !skip.contains(&key.as_slice()))
                    .map(|(_, value)| value),
            ),
            Object::Stream(stream) => pending.extend(
                stream
                    .dict
                    .iter()
                    .filter(|(key, _)| !skip.contains(&key.as_slice()))
                    .map(|(_, value)| value),
            ),
            _ => {}
        }
    }
    found
}

fn signature_page(document: &Document, field: &SignatureField) -> Option<u32> {
    let visible = field.widget_ids.iter().any(|id| {
        document
            .get_dictionary(*id)
            .ok()
            .and_then(|widget| widget.get(b"Rect").ok())
            .and_then(|rect| match resolve(document, rect) {
                Object::Array(rect) => {
                    let numbers: Vec<f32> = rect
                        .iter()
                        .filter_map(|number| resolve(document, number).as_float().ok())
                        .collect();
                    (numbers.len() == 4).then(|| {
                        (numbers[2] - numbers[0]).abs() > 0.5
                            && (numbers[3] - numbers[1]).abs() > 0.5
                    })
                }
                _ => None,
            })
            .unwrap_or(false)
    });
    if !visible {
        return None;
    }
    let _ = field.id;
    for (number, page) in document.get_pages() {
        let Ok(page) = document.get_dictionary(page) else {
            continue;
        };
        let Some(Object::Array(annotations)) =
            page.get(b"Annots").ok().map(|a| resolve(document, a))
        else {
            continue;
        };
        if annotations.iter().any(|annotation| {
            annotation
                .as_reference()
                .is_ok_and(|id| field.widget_ids.contains(&id))
        }) {
            return Some(number - 1);
        }
    }
    None
}

fn resolve<'a>(document: &'a Document, object: &'a Object) -> &'a Object {
    document
        .dereference(object)
        .map(|(_, object)| object)
        .unwrap_or(object)
}

fn dictionary<'a>(document: &'a Document, object: &'a Object) -> Option<&'a Dictionary> {
    object_dictionary(resolve(document, object))
}

fn object_dictionary(object: &Object) -> Option<&Dictionary> {
    match object {
        Object::Dictionary(dictionary) => Some(dictionary),
        Object::Stream(stream) => Some(&stream.dict),
        _ => None,
    }
}

pub(crate) fn text(object: &Object) -> Option<String> {
    let Object::String(bytes, _) = object else {
        return None;
    };
    let text = if let Some(utf16) = bytes.strip_prefix(&[0xFE, 0xFF]) {
        let units: Vec<u16> = utf16
            .chunks_exact(2)
            .map(|pair| u16::from_be_bytes([pair[0], pair[1]]))
            .collect();
        String::from_utf16_lossy(&units)
    } else if let Some(utf8) = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        String::from_utf8_lossy(utf8).into_owned()
    } else {
        bytes.iter().map(|byte| char::from(*byte)).collect()
    };
    let text = text.trim().to_owned();
    (!text.is_empty()).then_some(text)
}

/// A PDF date (`D:YYYYMMDDHHmmSSOHH'mm'`) as seconds since 1970.
pub(crate) fn pdf_date(date: &str) -> Option<i64> {
    let date = date.strip_prefix("D:").unwrap_or(date);
    let digits = |range: std::ops::Range<usize>, default: i64| -> Option<i64> {
        match date.get(range) {
            Some(text) if text.bytes().all(|byte| byte.is_ascii_digit()) && !text.is_empty() => {
                text.parse().ok()
            }
            Some(_) => None,
            None => Some(default),
        }
    };
    let year = digits(0..4, 0)?;
    if year == 0 {
        return None;
    }
    let local = civil_seconds(
        year,
        digits(4..6, 1)?,
        digits(6..8, 1)?,
        digits(8..10, 0)?,
        digits(10..12, 0)?,
        digits(12..14, 0)?,
    );
    let zone = date.get(14..).unwrap_or("");
    let offset = match zone.as_bytes().first() {
        Some(sign @ (b'+' | b'-')) => {
            let numbers: Vec<i64> = zone[1..]
                .split(|c: char| !c.is_ascii_digit())
                .filter(|part| !part.is_empty())
                .filter_map(|part| part.parse().ok())
                .collect();
            let minutes =
                numbers.first().copied().unwrap_or(0) * 60 + numbers.get(1).copied().unwrap_or(0);
            if *sign == b'+' {
                minutes * 60
            } else {
                -minutes * 60
            }
        }
        _ => 0,
    };
    Some(local - offset)
}

/// UTCTime (0x17) or GeneralizedTime (0x18), with optional fractions.
fn asn1_time(tag: u8, content: &[u8]) -> Option<i64> {
    let text = std::str::from_utf8(content).ok()?;
    let text = text.strip_suffix('Z')?;
    let (whole, _fraction) = text.split_once('.').unwrap_or((text, ""));
    let (year, rest) = match tag {
        0x17 => {
            let year: i64 = whole.get(0..2)?.parse().ok()?;
            (
                if year < 50 { 2000 + year } else { 1900 + year },
                whole.get(2..)?,
            )
        }
        0x18 => (whole.get(0..4)?.parse().ok()?, whole.get(4..)?),
        _ => return None,
    };
    let number = |range: std::ops::Range<usize>| -> Option<i64> { rest.get(range)?.parse().ok() };
    Some(civil_seconds(
        year,
        number(0..2)?,
        number(2..4)?,
        number(4..6)?,
        number(6..8)?,
        number(8..10).unwrap_or(0),
    ))
}

fn civil_seconds(year: i64, month: i64, day: i64, hour: i64, minute: i64, second: i64) -> i64 {
    // Days from 1970-01-01 (Howard Hinnant's days_from_civil).
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let year_of_era = year - era * 400;
    let month_index = (month + 9) % 12;
    let day_of_year = (153 * month_index + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    let days = era * 146_097 + day_of_era - 719_468;
    days * 86_400 + hour * 3_600 + minute * 60 + second
}

/// One DER element (or BER, as Acrobat-style signers write their outer
/// layers): its tag, all its bytes and its content.
pub(crate) struct Tlv<'a> {
    pub(crate) tag: u8,
    pub(crate) raw: &'a [u8],
    pub(crate) content: &'a [u8],
}

/// How deep indefinite-length elements may nest.
const MAX_NESTING: usize = 64;

pub(crate) fn tlv(data: &[u8]) -> Option<(Tlv<'_>, &[u8])> {
    tlv_nested(data, 0)
}

fn tlv_nested(data: &[u8], depth: usize) -> Option<(Tlv<'_>, &[u8])> {
    let tag = *data.first()?;
    if tag & 0x1F == 0x1F || depth > MAX_NESTING {
        return None;
    }
    let first = *data.get(1)?;
    if first == 0x80 {
        // Indefinite length (BER): a constructed element's children run to
        // an end-of-contents marker.
        if tag & 0x20 == 0 {
            return None;
        }
        let mut rest = &data[2..];
        loop {
            if rest.starts_with(&[0, 0]) {
                let content_end = data.len() - rest.len();
                let end = content_end + 2;
                return Some((
                    Tlv {
                        tag,
                        raw: &data[..end],
                        content: &data[2..content_end],
                    },
                    &data[end..],
                ));
            }
            let (_, after) = tlv_nested(rest, depth + 1)?;
            rest = after;
        }
    }
    let (length, header) = if first < 0x80 {
        (usize::from(first), 2)
    } else {
        let count = usize::from(first & 0x7F);
        if count > 4 {
            return None;
        }
        let bytes = data.get(2..2 + count)?;
        (
            bytes
                .iter()
                .fold(0_usize, |length, byte| length << 8 | usize::from(*byte)),
            2 + count,
        )
    };
    let end = header.checked_add(length)?;
    let raw = data.get(..end)?;
    Some((
        Tlv {
            tag,
            raw,
            content: &raw[header..],
        },
        &data[end..],
    ))
}

pub(crate) fn children(mut content: &[u8]) -> Option<Vec<Tlv<'_>>> {
    let mut found = Vec::new();
    while !content.is_empty() {
        let (child, rest) = tlv(content)?;
        found.push(child);
        content = rest;
    }
    Some(found)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_timestamp_answers_only_its_own_request() {
        let pdf = std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/signatures/signed-timestamped.pdf"),
        )
        .unwrap();
        let document = load_bounded(&pdf).unwrap();
        let field = signature_fields(&document).remove(0);
        let ranges = byte_range(&document, &field.value, pdf.len()).unwrap();
        let Ok(parsed) = ParsedSignedData::parse(&signature_contents(&pdf, &ranges).unwrap())
        else {
            panic!("the signature parses");
        };
        let token = parsed.timestamp_token.clone().unwrap();
        let nonce = ParsedSignedData::parse(&token)
            .ok()
            .unwrap()
            .timestamp_info()
            .unwrap()
            .nonce
            .expect("pyHanko asks with a nonce");
        assert!(timestamp_answers(&token, &parsed.signature, &nonce));
        assert!(!timestamp_answers(&token, b"another signature", &nonce));
        assert!(!timestamp_answers(&token, &parsed.signature, &[0x42; 8]));
    }

    #[test]
    fn pdf_dates_and_asn1_times_read_as_utc_seconds() {
        assert_eq!(pdf_date("D:19700101000000Z"), Some(0));
        assert_eq!(
            pdf_date("D:20261031093000+10'00'"),
            pdf_date("D:20261030233000Z")
        );
        assert_eq!(pdf_date("D:2026"), Some(civil_seconds(2026, 1, 1, 0, 0, 0)));
        assert_eq!(
            asn1_time(0x18, b"20261031093000.123Z"),
            pdf_date("D:20261031093000Z")
        );
        assert_eq!(
            asn1_time(0x17, b"261031093000Z"),
            pdf_date("D:20261031093000Z")
        );
        assert_eq!(asn1_time(0x17, b"261031093000"), None);
    }

    #[test]
    fn raw_ecdsa_signatures_become_der() {
        // r has its top bit set (needs a zero byte); s has leading zeros.
        let mut raw = vec![0x80; 32];
        raw.extend(std::iter::repeat_n(0, 30));
        raw.extend([0x01, 0x02]);
        let der = ecdsa_der_from_raw(&raw);
        let (sequence, _) = tlv(&der).unwrap();
        let parts = children(sequence.content).unwrap();
        assert_eq!(parts[0].content.len(), 33);
        assert_eq!(parts[0].content[0], 0);
        assert_eq!(parts[1].content, &[0x01, 0x02]);
    }

    #[test]
    fn this_computer_trusts_some_roots() {
        assert!(!system_trusted_roots().is_empty());
    }

    #[test]
    fn der_and_indefinite_ber_elements_split() {
        let (sequence, rest) = tlv(&[0x30, 0x03, 0x02, 0x01, 0x05, 0xFF]).unwrap();
        assert_eq!(sequence.tag, 0x30);
        assert_eq!(rest, &[0xFF]);
        let parts = children(sequence.content).unwrap();
        assert_eq!(parts[0].content, &[0x05]);
        // SEQUENCE (indefinite) { INTEGER 5, [0] (indefinite) { NULL } }
        let ber = [
            0x30, 0x80, 0x02, 0x01, 0x05, 0xA0, 0x80, 0x05, 0x00, 0, 0, 0, 0, 0xFF,
        ];
        let (sequence, rest) = tlv(&ber).unwrap();
        assert_eq!(rest, &[0xFF]);
        let parts = children(sequence.content).unwrap();
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[1].content, &[0x05, 0x00]);
        // Unterminated, or primitive with an indefinite length.
        assert!(tlv(&[0x30, 0x80, 0x02, 0x01, 0x05]).is_none());
        assert!(tlv(&[0x04, 0x80, 0x00, 0x00]).is_none());
        // Nesting past the limit.
        let deep: Vec<u8> = std::iter::repeat_n([0x30, 0x80], 200).flatten().collect();
        assert!(tlv(&deep).is_none());
    }
}
