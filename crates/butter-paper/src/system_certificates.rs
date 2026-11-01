//! Signing identities the operating system holds: the macOS Keychain
//! (including smart cards it exposes) and the Windows personal certificate
//! store (including smart cards through their drivers). The private key
//! never leaves the system: signing asks it to sign a digest, and it may
//! ask the user for a PIN or permission.

use std::fmt;

/// Signs a SHA-256 digest with a key the system holds.
pub trait SystemKey: Send + Sync {
    /// The signature over `digest` (a SHA-256 hash): PKCS#1 v1.5 for RSA,
    /// DER for ECDSA.
    fn sign_sha256(&self, digest: &[u8]) -> Result<Vec<u8>, String>;
}

/// A certificate with a private key the system can sign with.
pub struct SystemIdentity {
    /// The signer's name.
    pub name: String,
    /// Who issued the certificate.
    pub issuer: String,
    /// When the certificate expires (seconds since 1970).
    pub expires: i64,
    /// The certificate chain, signer first, as DER.
    pub chain: Vec<Vec<u8>>,
    pub key: Box<dyn SystemKey>,
}

impl fmt::Debug for SystemIdentity {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SystemIdentity")
            .field("name", &self.name)
            .field("issuer", &self.issuer)
            .field("expires", &self.expires)
            .finish_non_exhaustive()
    }
}

/// The name, issuer and expiry of a DER certificate, and whether it may
/// sign documents: its key usage (if any) allows signatures, and its
/// extended key usage (if any) includes a purpose documents fit.
#[cfg_attr(not(any(target_os = "macos", windows)), allow(dead_code))]
fn describe(certificate: &[u8]) -> Option<(String, String, i64, bool)> {
    use der::Decode as _;
    let certificate = x509_cert::Certificate::from_der(certificate).ok()?;
    let tbs = certificate.tbs_certificate();
    let name_of = |name: &x509_cert::name::Name| {
        name.common_name()
            .ok()
            .flatten()
            .map(|common| common.value().into_owned())
            .or_else(|| {
                name.organization()
                    .ok()
                    .flatten()
                    .map(|organization| organization.value().into_owned())
            })
            .unwrap_or_else(|| name.to_string())
    };
    let expires = tbs.validity().not_after.to_unix_duration().as_secs() as i64;
    let key_usage = tbs
        .get_extension::<x509_cert::ext::pkix::KeyUsage>()
        .ok()
        .flatten()
        .is_none_or(|(_, usage)| usage.digital_signature() || usage.non_repudiation());
    // Restricted to other purposes (code signing, servers), such as a
    // developer's certificates, it is not for documents.
    let document_purposes = [
        "2.5.29.37.0",             // any purpose
        "1.3.6.1.5.5.7.3.36",      // document signing
        "1.3.6.1.5.5.7.3.4",       // email protection
        "1.3.6.1.5.5.7.3.2",       // client authentication
        "1.2.840.113583.1.1.5",    // Adobe Authentic Documents Trust
        "1.3.6.1.4.1.311.10.3.12", // Microsoft document signing
    ];
    let purpose = tbs
        .get_extension::<x509_cert::ext::pkix::ExtendedKeyUsage>()
        .ok()
        .flatten()
        .is_none_or(|(_, usage)| {
            usage
                .0
                .iter()
                .any(|oid| document_purposes.contains(&oid.to_string().as_str()))
        });
    Some((
        name_of(tbs.subject()),
        name_of(tbs.issuer()),
        expires,
        key_usage && purpose,
    ))
}

/// The identities that can sign documents, newest expiry first.
pub fn system_identities() -> Vec<SystemIdentity> {
    let mut found = platform::identities(None);
    found.sort_by(|a, b| b.expires.cmp(&a.expires).then(a.name.cmp(&b.name)));
    found
}

/// The identities in the keychain at `path`, unlocked with `password`, for
/// tests that must not touch the user's keychains.
#[cfg(target_os = "macos")]
pub fn keychain_identities(path: &std::path::Path, password: &str) -> Vec<SystemIdentity> {
    use security_framework::os::macos::keychain::SecKeychain;
    let Ok(mut keychain) = SecKeychain::open(path) else {
        return Vec::new();
    };
    if keychain.unlock(Some(password)).is_err() {
        return Vec::new();
    }
    platform::identities(Some(&[keychain]))
}

#[cfg(target_os = "macos")]
mod platform {
    use security_framework::{
        certificate::SecCertificate,
        item::{ItemClass, ItemSearchOptions, Limit, Reference, SearchResult},
        key::{Algorithm, SecKey},
        os::macos::keychain::SecKeychain,
        policy::SecPolicy,
        trust::SecTrust,
    };

    use super::{SystemIdentity, SystemKey, describe};

    struct KeychainKey {
        key: SecKey,
        rsa: bool,
    }

    impl SystemKey for KeychainKey {
        fn sign_sha256(&self, digest: &[u8]) -> Result<Vec<u8>, String> {
            let algorithm = if self.rsa {
                Algorithm::RSASignatureDigestPKCS1v15SHA256
            } else {
                Algorithm::ECDSASignatureDigestX962SHA256
            };
            self.key
                .create_signature(algorithm, digest)
                .map_err(|error| match error.code() {
                    -128 => "signing was cancelled".to_owned(),
                    -25293 => "the keychain's password was wrong".to_owned(),
                    -25308 => "its keychain is locked, and macOS could not ask for its password"
                        .to_owned(),
                    // Not allowed by the key's access control.
                    -60008 | -25244 => "macOS did not let Butter Paper use the certificate's \
                                        key; allow Butter Paper in the key's Access Control in \
                                        Keychain Access"
                        .to_owned(),
                    _ => error.description().to_string(),
                })
        }
    }

    pub(super) fn identities(keychains: Option<&[SecKeychain]>) -> Vec<SystemIdentity> {
        let mut search = ItemSearchOptions::new();
        search
            .class(ItemClass::identity())
            .load_refs(true)
            .limit(Limit::All);
        if let Some(keychains) = keychains {
            search.keychains(keychains);
        }
        let Ok(results) = search.search() else {
            return Vec::new();
        };
        let mut found = Vec::new();
        for result in results {
            let SearchResult::Ref(Reference::Identity(identity)) = result else {
                continue;
            };
            let (Ok(certificate), Ok(key)) = (identity.certificate(), identity.private_key())
            else {
                continue;
            };
            let der = certificate.to_der();
            let Some((name, issuer, expires, signs)) = describe(&der) else {
                continue;
            };
            if !signs {
                continue;
            }
            let rsa = crate::pdf_signatures::certificate_is_rsa(&der);
            found.push(SystemIdentity {
                name,
                issuer,
                expires,
                chain: chain(&certificate),
                key: Box::new(KeychainKey { key, rsa }),
            });
        }
        found
    }

    /// The certificate and the issuers the system knows, up to its root.
    fn chain(certificate: &SecCertificate) -> Vec<Vec<u8>> {
        let mut chain = vec![certificate.to_der()];
        if let Ok(trust) = SecTrust::create_with_certificates(
            std::slice::from_ref(certificate),
            &[SecPolicy::create_x509()],
        ) {
            let _ = trust.evaluate_with_error();
            for issuer in trust.chain().into_iter().skip(1) {
                chain.push(issuer.to_der());
            }
        }
        chain
    }
}

#[cfg(windows)]
mod platform {
    use windows_sys::Win32::Security::Cryptography::{
        BCRYPT_PAD_PKCS1, BCRYPT_PKCS1_PADDING_INFO, BCRYPT_SHA256_ALGORITHM, CERT_CHAIN_CONTEXT,
        CERT_CHAIN_PARA, CERT_CONTEXT, CERT_KEY_PROV_INFO_PROP_ID,
        CRYPT_ACQUIRE_ONLY_NCRYPT_KEY_FLAG, CertCloseStore, CertDuplicateCertificateContext,
        CertEnumCertificatesInStore, CertFreeCertificateChain, CertFreeCertificateContext,
        CertGetCertificateChain, CertGetCertificateContextProperty, CertOpenSystemStoreW,
        CryptAcquireCertificatePrivateKey, HCRYPTPROV_OR_NCRYPT_KEY_HANDLE, NCryptFreeObject,
        NCryptSignHash,
    };

    use super::{SystemIdentity, SystemKey, describe};

    /// A personal-store certificate whose CNG key signs.
    struct StoreKey {
        certificate: *const CERT_CONTEXT,
        rsa: bool,
    }

    // SAFETY: a certificate context is reference counted and may be used
    // from any thread; the key is opened for each signature.
    unsafe impl Send for StoreKey {}
    unsafe impl Sync for StoreKey {}

    impl Drop for StoreKey {
        fn drop(&mut self) {
            // SAFETY: the context was duplicated for this key.
            unsafe { CertFreeCertificateContext(self.certificate) };
        }
    }

    impl SystemKey for StoreKey {
        fn sign_sha256(&self, digest: &[u8]) -> Result<Vec<u8>, String> {
            let mut key: HCRYPTPROV_OR_NCRYPT_KEY_HANDLE = 0;
            let mut spec = 0_u32;
            let mut free = 0_i32;
            // SAFETY: the certificate context is valid; the out-parameters
            // are written by the call.
            let acquired = unsafe {
                CryptAcquireCertificatePrivateKey(
                    self.certificate,
                    CRYPT_ACQUIRE_ONLY_NCRYPT_KEY_FLAG,
                    std::ptr::null(),
                    &mut key,
                    &mut spec,
                    &mut free,
                )
            };
            if acquired == 0 {
                return Err("the certificate's private key could not be opened".into());
            }
            let result = sign(key, digest, self.rsa);
            if free != 0 {
                // SAFETY: the call above handed the key to us to free.
                unsafe { NCryptFreeObject(key) };
            }
            result
        }
    }

    fn sign(key: usize, digest: &[u8], rsa: bool) -> Result<Vec<u8>, String> {
        let padding = BCRYPT_PKCS1_PADDING_INFO {
            pszAlgId: BCRYPT_SHA256_ALGORITHM,
        };
        let (padding_info, flags) = if rsa {
            (
                &padding as *const _ as *const core::ffi::c_void,
                BCRYPT_PAD_PKCS1,
            )
        } else {
            (std::ptr::null(), 0)
        };
        let mut length = 0_u32;
        // SAFETY: a size query, then a signature into a buffer that size.
        let status = unsafe {
            NCryptSignHash(
                key,
                padding_info,
                digest.as_ptr(),
                digest.len() as u32,
                std::ptr::null_mut(),
                0,
                &mut length,
                flags,
            )
        };
        if status != 0 {
            return Err(refusal(status));
        }
        let mut signature = vec![0_u8; length as usize];
        let status = unsafe {
            NCryptSignHash(
                key,
                padding_info,
                digest.as_ptr(),
                digest.len() as u32,
                signature.as_mut_ptr(),
                length,
                &mut length,
                flags,
            )
        };
        if status != 0 {
            return Err(refusal(status));
        }
        signature.truncate(length as usize);
        if rsa {
            Ok(signature)
        } else {
            Ok(crate::pdf_signatures::ecdsa_der_from_raw(&signature))
        }
    }

    /// Why the store or a smart card would not sign, in words.
    fn refusal(status: i32) -> String {
        match status as u32 {
            // NTE_USER_CANCELLED, or ERROR_CANCELLED as an HRESULT
            0x8009_0036 | 0x8007_04C7 => "signing was cancelled".to_owned(),
            0x8010_006B => "the smart card's PIN was wrong".to_owned(),
            0x8010_006C => "the smart card is blocked after too many wrong PINs".to_owned(),
            0x8010_000C | 0x8010_0069 => "the smart card isn't inserted".to_owned(),
            0x8009_0016 => "the certificate's private key could not be found".to_owned(),
            _ => format!("signing failed ({status:#x})"),
        }
    }

    fn der(certificate: *const CERT_CONTEXT) -> Vec<u8> {
        // SAFETY: the context is valid and points at its encoded bytes.
        unsafe {
            let context = &*certificate;
            std::slice::from_raw_parts(context.pbCertEncoded, context.cbCertEncoded as usize)
                .to_vec()
        }
    }

    pub(super) fn identities(_: Option<&[()]>) -> Vec<SystemIdentity> {
        let name: Vec<u16> = "MY\0".encode_utf16().collect();
        // SAFETY: opens the current user's personal store by name.
        let store = unsafe { CertOpenSystemStoreW(0, name.as_ptr()) };
        if store.is_null() {
            return Vec::new();
        }
        let mut found = Vec::new();
        let mut previous: *const CERT_CONTEXT = std::ptr::null();
        loop {
            // SAFETY: enumeration frees `previous` and returns the next.
            let certificate = unsafe { CertEnumCertificatesInStore(store, previous) };
            if certificate.is_null() {
                break;
            }
            previous = certificate;
            let mut size = 0_u32;
            // SAFETY: a size query for the key provider property.
            let has_key = unsafe {
                CertGetCertificateContextProperty(
                    certificate,
                    CERT_KEY_PROV_INFO_PROP_ID,
                    std::ptr::null_mut(),
                    &mut size,
                )
            } != 0;
            if !has_key {
                continue;
            }
            let encoded = der(certificate);
            let Some((name, issuer, expires, signs)) = describe(&encoded) else {
                continue;
            };
            if !signs {
                continue;
            }
            let rsa = crate::pdf_signatures::certificate_is_rsa(&encoded);
            found.push(SystemIdentity {
                name,
                issuer,
                expires,
                chain: chain(certificate),
                key: Box::new(StoreKey {
                    // SAFETY: kept past the enumeration, freed on drop.
                    certificate: unsafe { CertDuplicateCertificateContext(certificate) },
                    rsa,
                }),
            });
        }
        // SAFETY: the store was opened above.
        unsafe { CertCloseStore(store, 0) };
        found
    }

    /// The certificate and the issuers Windows finds for it.
    fn chain(certificate: *const CERT_CONTEXT) -> Vec<Vec<u8>> {
        let mut chain = vec![der(certificate)];
        let mut parameters: CERT_CHAIN_PARA = unsafe { std::mem::zeroed() };
        parameters.cbSize = std::mem::size_of::<CERT_CHAIN_PARA>() as u32;
        let mut context: *mut CERT_CHAIN_CONTEXT = std::ptr::null_mut();
        // SAFETY: builds the chain from the default engine and stores.
        let built = unsafe {
            CertGetCertificateChain(
                std::ptr::null_mut(),
                certificate,
                std::ptr::null(),
                std::ptr::null_mut(),
                &parameters,
                0,
                std::ptr::null(),
                &mut context,
            )
        };
        if built == 0 || context.is_null() {
            return chain;
        }
        // SAFETY: a built chain context holds its simple chains and elements.
        unsafe {
            let context_ref = &*context;
            if context_ref.cChain > 0 {
                let simple = &**context_ref.rgpChain;
                for index in 1..simple.cElement as usize {
                    let element = &**simple.rgpElement.add(index);
                    chain.push(der(element.pCertContext));
                }
            }
            CertFreeCertificateChain(context);
        }
        chain
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> Vec<u8> {
        std::fs::read(
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/signatures")
                .join(name),
        )
        .unwrap()
    }

    #[test]
    fn signing_certificates_are_offered_and_timestamp_ones_are_not() {
        let (name, issuer, _, signs) = describe(&fixture("test-root.cer")).unwrap();
        assert_eq!(
            (name.as_str(), issuer.as_str()),
            ("Butter Paper Test Root", "Butter Paper Test Root")
        );
        // A CA certificate only signs certificates.
        assert!(!signs);
        // The timestamp authority's certificate is for timestamps only.
        let token = crate::pdf_signatures::timestamp_token_certificates(&timestamp_token());
        let tsa = token.iter().find_map(|certificate| {
            describe(certificate).filter(|(name, ..)| name == "Test Timestamp Authority")
        });
        assert!(tsa.is_some_and(|(_, _, _, signs)| !signs));
    }

    /// The test timestamp authority's token, from the timestamped fixture:
    /// the signature is its longest hex string, and the token is inside.
    fn timestamp_token() -> Vec<u8> {
        let pdf = fixture("signed-timestamped.pdf");
        let mut longest: &[u8] = &[];
        let mut index = 0;
        while let Some(offset) = pdf[index..]
            .windows(5)
            .position(|window| window == b"<3082")
        {
            let open = index + offset;
            let close = open + pdf[open..].iter().position(|byte| *byte == b'>').unwrap();
            if close - open > longest.len() {
                longest = &pdf[open + 1..close];
            }
            index = close;
        }
        let cms: Vec<u8> = longest
            .chunks(2)
            .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
            .collect();
        cms
    }
}

#[cfg(not(any(target_os = "macos", windows)))]
mod platform {
    use super::SystemIdentity;

    /// Linux has no system store of signing identities; smart cards there
    /// need PKCS#11, which is not supported yet.
    pub(super) fn identities(_: Option<&[()]>) -> Vec<SystemIdentity> {
        Vec::new()
    }
}
