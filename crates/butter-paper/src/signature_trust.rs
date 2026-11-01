//! The certificates a signature's chain may end at: this computer's trusted
//! roots, and Adobe's two published trust lists (the Adobe Approved Trust
//! List and the EU Trusted List), which hold the document-signing roots that
//! system stores leave out.
//!
//! Adobe publishes each list as a PDF signed through Adobe Root CA G2, with
//! the certificates in an embedded `SecuritySettings.xml`. A list is used
//! only when that signature is intact, covers the whole file and chains to
//! the pinned root; a verified copy is kept and refreshed weekly.

use std::{
    io::Read as _,
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};

use base64::Engine as _;
use lopdf::Object;

use crate::pdf_signatures::{Changes, Integrity, Trust, signatures, system_trusted_roots};

/// Adobe Root CA G2 (SHA-256 458E0E21…772D1B22), as served with the lists
/// from `trustlist.adobe.com`.
const ADOBE_ROOT_CA_G2: &[u8] = include_bytes!("../assets/certificates/adobe-root-ca-g2.cer");

/// The lists: a file name for the kept copy, and where Adobe publishes it.
pub const TRUST_LISTS: [(&str, &str); 2] = [
    (
        "adobe-approved-trust-list.pdf",
        "https://trustlist.adobe.com/tl12.acrobatsecuritysettings",
    ),
    (
        "eu-trusted-list.pdf",
        "https://trustlist.adobe.com/eutl12.acrobatsecuritysettings",
    ),
];

const REFRESH_AFTER: Duration = Duration::from_secs(7 * 24 * 60 * 60);
const MAX_LIST_BYTES: u64 = 32 * 1024 * 1024;

/// The roots one of Adobe's trust list PDFs holds, once its signature is
/// checked.
pub fn trust_list_roots(pdf: &[u8]) -> Result<Vec<Vec<u8>>, String> {
    let found = signatures(pdf, &[ADOBE_ROOT_CA_G2.to_vec()]);
    let [signature] = found.as_slice() else {
        return Err("the trust list is not signed once".into());
    };
    if signature.integrity != Integrity::Intact
        || signature.changes != Changes::None
        || signature.certificate.trust != Trust::Trusted
        || !signature.certificate.in_date
    {
        return Err("the trust list's signature does not check out".into());
    }
    let settings = embedded_settings(pdf).ok_or("the trust list has no certificates")?;
    let roots = settings_roots(&settings)?;
    if roots.is_empty() {
        return Err("the trust list has no trusted roots".into());
    }
    Ok(roots)
}

/// The embedded `SecuritySettings.xml`.
fn embedded_settings(pdf: &[u8]) -> Option<String> {
    let document = crate::pdf_signatures::load_bounded(pdf)?;
    let names = document
        .catalog()
        .ok()?
        .get(b"Names")
        .ok()
        .and_then(|names| document.dereference(names).ok())?
        .1
        .as_dict()
        .ok()?;
    let files = document
        .dereference(names.get(b"EmbeddedFiles").ok()?)
        .ok()?
        .1
        .as_dict()
        .ok()?;
    let mut pending = vec![files.clone()];
    while let Some(node) = pending.pop() {
        if let Ok(Object::Array(kids)) = node.get(b"Kids") {
            for kid in kids {
                if let Ok((_, Object::Dictionary(kid))) = document.dereference(kid) {
                    pending.push(kid.clone());
                }
            }
        }
        let Ok(Object::Array(entries)) = node.get(b"Names") else {
            continue;
        };
        for pair in entries.chunks(2) {
            let [name, spec] = pair else {
                continue;
            };
            if crate::pdf_signatures::text(name).as_deref() != Some("SecuritySettings.xml") {
                continue;
            }
            let spec = document.dereference(spec).ok()?.1.as_dict().ok()?;
            let embedded = document
                .dereference(spec.get(b"EF").ok()?)
                .ok()?
                .1
                .as_dict()
                .ok()?;
            let Object::Stream(stream) = document.dereference(embedded.get(b"F").ok()?).ok()?.1
            else {
                return None;
            };
            let bytes = if stream.dict.has(b"Filter") {
                stream
                    .decompressed_content_with_limit(MAX_LIST_BYTES as usize)
                    .ok()?
            } else {
                stream.content.clone()
            };
            return String::from_utf8(bytes).ok();
        }
    }
    None
}

/// Each identity Adobe marks as a trusted root.
fn settings_roots(xml: &str) -> Result<Vec<Vec<u8>>, String> {
    let document = roxmltree::Document::parse(xml).map_err(|error| error.to_string())?;
    let mut roots = Vec::new();
    for identity in document
        .descendants()
        .filter(|node| node.has_tag_name("Identity"))
    {
        let root = identity
            .children()
            .find(|node| node.has_tag_name("Trust"))
            .and_then(|trust| child_text(trust, "Root"))
            == Some("1");
        if !root {
            continue;
        }
        let Some(certificate) = child_text(identity, "Certificate") else {
            continue;
        };
        let body: String = certificate.split_whitespace().collect();
        if let Ok(der) = base64::engine::general_purpose::STANDARD.decode(body) {
            roots.push(der);
        }
    }
    Ok(roots)
}

fn child_text<'a>(parent: roxmltree::Node<'a, '_>, name: &str) -> Option<&'a str> {
    parent
        .children()
        .find(|node| node.has_tag_name(name))
        .and_then(|node| node.text())
        .map(str::trim)
}

/// Roots to check signatures against, with the kept trust lists in
/// `directory`.
pub struct TrustStore {
    directory: PathBuf,
}

impl TrustStore {
    pub fn new(directory: impl Into<PathBuf>) -> Self {
        Self {
            directory: directory.into(),
        }
    }

    /// This computer's roots and those of every kept list that checks out.
    /// Reads files: call it off the UI thread.
    pub fn roots(&self) -> Vec<Vec<u8>> {
        let mut roots = system_trusted_roots().to_vec();
        for (file, _) in TRUST_LISTS {
            if let Ok(pdf) = std::fs::read(self.directory.join(file))
                && let Ok(list) = trust_list_roots(&pdf)
            {
                roots.extend(list);
            }
        }
        roots.sort();
        roots.dedup();
        roots
    }

    /// Downloads each list whose kept copy is missing or a week old, keeping
    /// it only when it checks out; whether any list was replaced. Blocking:
    /// call it off the UI thread.
    pub fn refresh(&self) -> Result<bool, String> {
        let mut replaced = false;
        let mut failures = Vec::new();
        for (file, url) in TRUST_LISTS {
            let path = self.directory.join(file);
            if !stale(&path) {
                continue;
            }
            match self.download(&path, url) {
                Ok(()) => replaced = true,
                Err(error) => failures.push(format!("{file}: {error}")),
            }
        }
        if failures.is_empty() {
            Ok(replaced)
        } else {
            Err(failures.join("; "))
        }
    }

    fn download(&self, path: &Path, url: &str) -> Result<(), String> {
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(60))
            .build()
            .map_err(|error| error.to_string())?;
        let response = client
            .get(url)
            .send()
            .and_then(reqwest::blocking::Response::error_for_status)
            .map_err(|error| error.to_string())?;
        let mut pdf = Vec::new();
        response
            .take(MAX_LIST_BYTES + 1)
            .read_to_end(&mut pdf)
            .map_err(|error| error.to_string())?;
        if pdf.len() as u64 > MAX_LIST_BYTES {
            return Err("the list is too large".into());
        }
        trust_list_roots(&pdf)?;
        std::fs::create_dir_all(&self.directory).map_err(|error| error.to_string())?;
        let partial = path.with_extension("part");
        std::fs::write(&partial, &pdf).map_err(|error| error.to_string())?;
        std::fs::rename(&partial, path).map_err(|error| error.to_string())
    }
}

/// Asks a certificate's OCSP responder (posting `request`) or downloads its
/// revocation list, for [`crate::pdf_signatures::signatures_checked`].
/// Blocking: call it off the UI thread.
pub fn fetch_revocation(url: &str, request: Option<&[u8]>) -> Option<Vec<u8>> {
    const MAX_REVOCATION_BYTES: u64 = 8 * 1024 * 1024;
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return None;
    }
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .ok()?;
    let response = match request {
        Some(request) => client
            .post(url)
            .header("Content-Type", "application/ocsp-request")
            .body(request.to_vec())
            .send(),
        None => client.get(url).send(),
    }
    .and_then(reqwest::blocking::Response::error_for_status)
    .ok()?;
    let mut bytes = Vec::new();
    response
        .take(MAX_REVOCATION_BYTES + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    (bytes.len() as u64 <= MAX_REVOCATION_BYTES).then_some(bytes)
}

fn stale(path: &Path) -> bool {
    std::fs::metadata(path)
        .and_then(|metadata| metadata.modified())
        .ok()
        .and_then(|modified| SystemTime::now().duration_since(modified).ok())
        .is_none_or(|age| age > REFRESH_AFTER)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identities_marked_as_roots_are_the_roots() {
        let der = |byte: u8| base64::engine::general_purpose::STANDARD.encode([byte; 4]);
        let xml = format!(
            "<?xml version=\"1.0\"?><SecuritySettings><TrustedIdentities>\
             <Identity><Certificate>{}</Certificate><Trust><Root>1</Root></Trust></Identity>\
             <Identity><Certificate>{}</Certificate><Trust><Root>0</Root></Trust></Identity>\
             <Identity><Certificate>\n{}\n</Certificate><Trust><Root> 1 </Root></Trust></Identity>\
             </TrustedIdentities></SecuritySettings>",
            der(1),
            der(2),
            der(3)
        );
        assert_eq!(settings_roots(&xml).unwrap(), vec![vec![1; 4], vec![3; 4]]);
    }

    #[test]
    fn a_list_not_signed_by_adobe_is_refused() {
        let unsigned = std::fs::read(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/text-edit/word-table.pdf"),
        )
        .unwrap();
        assert!(trust_list_roots(&unsigned).is_err());
        let signed_by_others = std::fs::read(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/signatures/signed-rsa.pdf"),
        )
        .unwrap();
        assert!(trust_list_roots(&signed_by_others).is_err());
    }

    #[test]
    fn the_pinned_adobe_root_is_adobe_root_ca_g2() {
        use signature_sha2::{Digest as _, Sha256};
        assert_eq!(
            Sha256::digest(ADOBE_ROOT_CA_G2).to_vec(),
            [
                0x45, 0x8E, 0x0E, 0x21, 0x96, 0x98, 0xB1, 0x8D, 0x2D, 0x40, 0x93, 0xD6, 0x33, 0x6A,
                0x12, 0x54, 0x79, 0x53, 0xA5, 0x4C, 0x05, 0xDD, 0x96, 0x7C, 0x3A, 0x5A, 0x26, 0x8B,
                0x77, 0x2D, 0x1B, 0x22
            ]
        );
    }

    /// Downloads both lists from Adobe and checks them.
    #[test]
    #[ignore = "downloads Adobe's trust lists"]
    fn adobe_trust_lists_download_and_check_out() {
        let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!(
            "target/test-scratch/trust-lists-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&directory);
        let store = TrustStore::new(&directory);
        assert!(store.refresh().unwrap(), "both lists were downloaded");
        assert!(!store.refresh().unwrap(), "fresh lists are kept");
        for (file, _) in TRUST_LISTS {
            let roots = trust_list_roots(&std::fs::read(directory.join(file)).unwrap()).unwrap();
            assert!(roots.len() > 20, "{file}: {}", roots.len());
        }
        assert!(store.roots().len() > system_trusted_roots().len());
        let _ = std::fs::remove_dir_all(directory);
    }
}
