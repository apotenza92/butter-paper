//! Native client for the reviewed BPS1 phone-to-desktop relay contract.
//! Only token hashes leave the desktop during creation. The AES key and phone
//! capability occur in the QR URL fragment, never the HTTP request or logs.
use crate::{
    annotation_model::DecodedRgbaAsset,
    image_asset_decode::{SanitizedSignatureFile, sanitize_signature_bytes},
};
use aes_gcm::{
    Aes256Gcm, Nonce,
    aead::{Aead, KeyInit, Payload},
};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD as B64};
use reqwest::{
    Url,
    blocking::{Client, Response},
    redirect::Policy,
};
use sha2::{Digest, Sha256};
use std::{
    io::Read,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use zeroize::{Zeroize, Zeroizing};

const MAX_IMAGE: usize = 1024 * 1024;
const MAX_ENVELOPE: usize = MAX_IMAGE + 53;
const TTL: u64 = 300_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PhoneMode {
    Draw,
    Image,
}
impl PhoneMode {
    fn name(self) -> &'static str {
        match self {
            Self::Draw => "draw",
            Self::Image => "image",
        }
    }
    fn byte(self) -> u8 {
        match self {
            Self::Draw => 1,
            Self::Image => 2,
        }
    }
}

/// Dropping the workspace-owned guard cancels pending capture/transfer work.
pub struct SignatureOperation(pub Arc<AtomicBool>);
impl Default for SignatureOperation {
    fn default() -> Self {
        Self(Arc::new(AtomicBool::new(false)))
    }
}
impl Drop for SignatureOperation {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Release);
    }
}

pub fn configured_origin() -> Result<String, String> {
    // Build-owned endpoint, like Electron. No runtime feed override in a release.
    validate_origin(
        option_env!("BP_SIGNATURE_RELAY_PRODUCTION_ORIGIN")
            .unwrap_or("https://butter-paper-signature-relay.apotenza92.workers.dev"),
    )
}
fn validate_origin(value: &str) -> Result<String, String> {
    let url = Url::parse(value).map_err(|_| "Phone transfer is not configured in this build.")?;
    if url.scheme() != "https"
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || url.path() != "/"
    {
        return Err("The phone signature relay must be an HTTPS origin.".into());
    }
    Ok(url.origin().ascii_serialization())
}
fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}
fn random<const N: usize>() -> Result<Zeroizing<[u8; N]>, String> {
    let mut value = Zeroizing::new([0; N]);
    getrandom::fill(value.as_mut()).map_err(|_| "Secure randomness is unavailable.")?;
    Ok(value)
}
fn bounded(response: Response, limit: usize) -> Result<Zeroizing<Vec<u8>>, String> {
    if response.content_length().is_some_and(|n| n > limit as u64) {
        return Err("The phone relay response is too large.".into());
    }
    let mut bytes = Zeroizing::new(Vec::new());
    response
        .take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "The phone relay response could not be read.")?;
    if bytes.len() > limit {
        return Err("The phone relay response is too large.".into());
    }
    Ok(bytes)
}
struct Session {
    client: Client,
    origin: String,
    id: String,
    token: Zeroizing<String>,
    key: Zeroizing<[u8; 32]>,
    mode: PhoneMode,
    expires_at: u64,
}
impl Session {
    fn endpoint(&self, suffix: &str) -> String {
        format!("{}/api/sessions/{}{}", self.origin, self.id, suffix)
    }
    fn delete(&self) {
        let _ = self
            .client
            .delete(self.endpoint(""))
            .bearer_auth(self.token.as_str())
            .send();
    }
}

/// Runs on the background executor. `show_qr` publishes only raster pixels to UI.
/// After delivery, decryption/authentication and sanitisation precede acknowledgement.
pub fn receive(
    origin: String,
    mode: PhoneMode,
    cancelled: Arc<AtomicBool>,
    show_qr: impl FnOnce(DecodedRgbaAsset) -> bool,
) -> Result<Option<SanitizedSignatureFile>, String> {
    receive_with_code(origin, mode, cancelled, |url| {
        qr_asset(url).is_ok_and(show_qr)
    })
}
fn receive_with_code(
    origin: String,
    mode: PhoneMode,
    cancelled: Arc<AtomicBool>,
    show_code: impl FnOnce(&str) -> bool,
) -> Result<Option<SanitizedSignatureFile>, String> {
    let origin = validate_origin(&origin)?;
    let client = Client::builder()
        .https_only(true)
        .redirect(Policy::none())
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(10))
        .build()
        .map_err(|_| "The secure phone client could not be started.")?;
    if cancelled.load(Ordering::Acquire) {
        return Ok(None);
    }
    let id = B64.encode(random::<16>()?.as_ref());
    let desktop = random::<32>()?;
    let phone = random::<32>()?;
    let key = random::<32>()?;
    let mut session = Session {
        client,
        origin,
        id,
        token: Zeroizing::new(B64.encode(desktop.as_ref())),
        key,
        mode,
        expires_at: 0,
    };
    let response = session.client.post(session.endpoint("")).json(&serde_json::json!({"version":1,"desktopTokenHash":B64.encode(Sha256::digest(desktop.as_ref())),"phoneTokenHash":B64.encode(Sha256::digest(phone.as_ref()))})).send().map_err(|_| "Unable to connect to the phone signature relay.")?;
    if response.status().as_u16() != 201 {
        return Err("Unable to create a private phone signature session.".into());
    }
    let result = (|| {
        let body = bounded(response, 4096)?;
        let json: serde_json::Value = serde_json::from_slice(&body)
            .map_err(|_| "The phone relay returned an invalid response.")?;
        session.expires_at = json
            .get("expiresAt")
            .and_then(|v| v.as_u64())
            .ok_or("The phone relay returned an invalid expiry.")?;
        let now = now_ms();
        if session.expires_at <= now || session.expires_at > now.saturating_add(TTL + 30_000) {
            return Err("The phone relay returned an invalid expiry.".into());
        }
        let phone_fragment = Zeroizing::new(B64.encode(phone.as_ref()));
        let key_fragment = Zeroizing::new(B64.encode(session.key.as_ref()));
        let url = Zeroizing::new(format!(
            "{}/s/{}#v=1&m={}&e={}&t={}&k={}",
            session.origin,
            session.id,
            mode.name(),
            session.expires_at,
            phone_fragment.as_str(),
            key_fragment.as_str()
        ));
        if cancelled.load(Ordering::Acquire) || !show_code(&url) {
            return Ok(None);
        }
        drop(url);
        drop(phone_fragment);
        drop(key_fragment);
        drop(phone);
        drop(desktop);
        let deadline = Instant::now() + Duration::from_millis(session.expires_at - now);
        for _ in 0..180 {
            if cancelled.load(Ordering::Acquire) {
                return Ok(None);
            }
            if Instant::now() >= deadline || now_ms() >= session.expires_at {
                return Err("The phone session expired. Start a new transfer.".into());
            }
            let response = session
                .client
                .get(session.endpoint("/payload"))
                .bearer_auth(session.token.as_str())
                .send();
            match response {
                Ok(response) if response.status().as_u16() == 200 => {
                    let bytes = bounded(response, MAX_ENVELOPE)?;
                    if cancelled.load(Ordering::Acquire) {
                        return Ok(None);
                    }
                    let (image, message_id) = decrypt(
                        &bytes,
                        &session.id,
                        session.mode,
                        session.expires_at,
                        &session.key,
                    )?;
                    session.key.zeroize();
                    for _ in 0..3 {
                        if cancelled.load(Ordering::Acquire) {
                            return Ok(None);
                        }
                        let ack = session.client.post(session.endpoint("/ack")).bearer_auth(session.token.as_str()).json(&serde_json::json!({"version":1,"messageId":B64.encode(message_id)})).send();
                        if ack.is_ok_and(|r| matches!(r.status().as_u16(), 204 | 410)) {
                            break;
                        }
                    }
                    return Ok(Some(image));
                }
                Ok(response) if matches!(response.status().as_u16(), 404 | 410) => {
                    return Err("The phone session expired. Start a new transfer.".into());
                }
                Ok(response) if response.status().as_u16() != 204 => {
                    return Err("Unable to receive the phone signature.".into());
                }
                _ => {}
            }
            for _ in 0..20 {
                if cancelled.load(Ordering::Acquire) {
                    return Ok(None);
                }
                std::thread::sleep(Duration::from_millis(100));
            }
        }
        Err("The phone session expired. Start a new transfer.".into())
    })();
    session.key.zeroize();
    session.delete();
    result
}

fn aad(id: &str, mode: PhoneMode, expires: u64, message: &[u8], len: usize) -> Vec<u8> {
    let mut value = b"ButterPaper.PhoneSignature.phone-to-desktop\0".to_vec();
    value.push(1);
    value.extend_from_slice(&Sha256::digest(id.as_bytes()));
    value.extend_from_slice(&expires.to_be_bytes());
    value.push(mode.byte());
    value.extend_from_slice(message);
    value.extend_from_slice(&(len as u32).to_be_bytes());
    value
}
fn decrypt(
    bytes: &[u8],
    id: &str,
    mode: PhoneMode,
    expires: u64,
    key: &[u8; 32],
) -> Result<(SanitizedSignatureFile, [u8; 16]), String> {
    if bytes.len() < 53 || bytes.len() > MAX_ENVELOPE || &bytes[..4] != b"BPS1" {
        return Err("The encrypted signature envelope is invalid.".into());
    }
    let message: [u8; 16] = bytes[4..20].try_into().unwrap();
    let cipher =
        Aes256Gcm::new_from_slice(key).map_err(|_| "The phone signature key is invalid.")?;
    let plaintext = Zeroizing::new(
        cipher
            .decrypt(
                &Nonce::try_from(&bytes[20..32]).map_err(|_| "The phone nonce is invalid.")?,
                Payload {
                    msg: &bytes[32..],
                    aad: &aad(id, mode, expires, &message, bytes.len() - 48),
                },
            )
            .map_err(|_| "The phone signature could not be authenticated.")?,
    );
    let len = u32::from_be_bytes(plaintext[1..5].try_into().unwrap()) as usize;
    if !matches!(plaintext[0], 1 | 2) || len == 0 || len > MAX_IMAGE || plaintext.len() != len + 5 {
        return Err("The phone signature payload is invalid.".into());
    }
    let image = &plaintext[5..];
    if (plaintext[0] == 1 && !image.starts_with(b"\x89PNG\r\n\x1a\n"))
        || (plaintext[0] == 2 && !image.starts_with(b"\xff\xd8\xff"))
    {
        return Err("The phone signature image type is invalid.".into());
    }
    Ok((
        sanitize_signature_bytes(image)
            .map_err(|_| "The phone signature image could not be processed.")?,
        message,
    ))
}
fn qr_asset(url: &str) -> Result<DecodedRgbaAsset, String> {
    let qr = qrcode::QrCode::with_error_correction_level(url.as_bytes(), qrcode::EcLevel::M)
        .map_err(|_| "Unable to create the phone QR code.")?;
    let modules = qr.width();
    let scale = 4;
    let size = (modules + 8) * scale;
    let mut rgba = vec![255; size * size * 4];
    for y in 0..modules {
        for x in 0..modules {
            if qr[(x, y)] == qrcode::Color::Dark {
                for dy in 0..scale {
                    for dx in 0..scale {
                        let i = (((y + 4) * scale + dy) * size + (x + 4) * scale + dx) * 4;
                        rgba[i..i + 3].fill(0);
                    }
                }
            }
        }
    }
    DecodedRgbaAsset::new(size as u32, size as u32, rgba)
        .map_err(|_| "Unable to render the phone QR code.".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (Vec<u8>, String, u64) {
        let v: serde_json::Value = serde_json::from_str(include_str!(
            "../tests/fixtures/phone-signature-envelope.json"
        ))
        .unwrap();
        (
            base64::engine::general_purpose::STANDARD
                .decode(v["envelope"].as_str().unwrap())
                .unwrap(),
            v["sessionId"].as_str().unwrap().into(),
            v["expiresAt"].as_u64().unwrap(),
        )
    }
    #[test]
    fn phone_protocol_accepts_existing_webcrypto_envelope() {
        let (bytes, id, expiry) = fixture();
        let (image, message) = decrypt(&bytes, &id, PhoneMode::Draw, expiry, &[3; 32]).unwrap();
        assert_eq!(message, [6; 16]);
        assert!(image.asset().width_px() > 0);
    }
    #[test]
    fn phone_protocol_rejects_tamper_key_and_cross_session_replay() {
        let (bytes, id, expiry) = fixture();
        for i in [0, 4, 20, 32, bytes.len() - 1] {
            let mut modified = bytes.clone();
            modified[i] ^= 1;
            assert!(decrypt(&modified, &id, PhoneMode::Draw, expiry, &[3; 32]).is_err());
        }
        assert!(decrypt(&bytes, "another-session", PhoneMode::Draw, expiry, &[3; 32]).is_err());
        assert!(decrypt(&bytes, &id, PhoneMode::Image, expiry, &[3; 32]).is_err());
        assert!(decrypt(&bytes, &id, PhoneMode::Draw, expiry + 1, &[3; 32]).is_err());
        assert!(decrypt(&bytes, &id, PhoneMode::Draw, expiry, &[4; 32]).is_err());
    }
    #[test]
    fn phone_protocol_bounds_and_origin_fail_closed() {
        for n in [0, 4, 32, 52, MAX_ENVELOPE + 1] {
            assert!(decrypt(&vec![0; n], "test", PhoneMode::Draw, 1, &[3; 32]).is_err());
        }
        for origin in [
            "",
            "http://localhost",
            "https://example.com/path",
            "https://name:password@example.com",
            "https://example.com?x=1",
            "https://example.com/#secret",
        ] {
            assert!(validate_origin(origin).is_err());
        }
        assert_eq!(
            validate_origin("https://example.com/").unwrap(),
            "https://example.com"
        );
    }
    #[test]
    fn phone_operation_drop_cancels_and_qr_has_quiet_border() {
        let operation = SignatureOperation::default();
        let flag = operation.0.clone();
        drop(operation);
        assert!(flag.load(Ordering::Acquire));
        let image = qr_asset("https://example.com/s/test#k=test-only").unwrap();
        assert_eq!(image.width_px(), image.height_px());
        assert!(
            image.rgba()[..image.width_px() as usize * 4 * 16]
                .iter()
                .all(|v| *v == 255)
        );
    }
}

#[cfg(test)]
mod live_tests {
    use super::*;
    use image::ImageEncoder;
    #[test]
    #[ignore = "creates and removes disposable encrypted synthetic sessions on the configured HTTPS relay"]
    fn phone_live_https_draw_image_and_cancellation() {
        for mode in [PhoneMode::Draw, PhoneMode::Image] {
            let flag = Arc::new(AtomicBool::new(false));
            let result = receive_with_code(configured_origin().unwrap(), mode, flag, |code| {
                let url = Url::parse(code).unwrap();
                let fields: std::collections::HashMap<_, _> = url
                    .fragment()
                    .unwrap()
                    .split('&')
                    .map(|v| v.split_once('=').unwrap())
                    .collect();
                let key = Zeroizing::new(B64.decode(fields["k"]).unwrap());
                let id = url.path().strip_prefix("/s/").unwrap();
                let expiry = fields["e"].parse().unwrap();
                let mut pixels = vec![255u8; 32 * 16 * 4];
                for y in 7..10 {
                    for x in 5..28 {
                        let i = (y * 32 + x) * 4;
                        pixels[i..i + 3].fill(0);
                    }
                }
                let mut png = Vec::new();
                image::codecs::png::PngEncoder::new(&mut png)
                    .write_image(&pixels, 32, 16, image::ExtendedColorType::Rgba8)
                    .unwrap();
                let mut plain = Zeroizing::new(vec![1]);
                plain.extend_from_slice(&(png.len() as u32).to_be_bytes());
                plain.extend_from_slice(&png);
                let message = [6u8; 16];
                let nonce = [7u8; 12];
                let cipher = Aes256Gcm::new_from_slice(&key).unwrap();
                let encrypted = cipher
                    .encrypt(
                        &Nonce::from(nonce),
                        Payload {
                            msg: &plain,
                            aad: &aad(id, mode, expiry, &message, plain.len()),
                        },
                    )
                    .unwrap();
                let mut envelope = b"BPS1".to_vec();
                envelope.extend_from_slice(&message);
                envelope.extend_from_slice(&nonce);
                envelope.extend_from_slice(&encrypted);
                let response = Client::builder()
                    .redirect(Policy::none())
                    .timeout(Duration::from_secs(10))
                    .build()
                    .unwrap()
                    .put(format!(
                        "{}/api/sessions/{id}/payload",
                        url.origin().ascii_serialization()
                    ))
                    .bearer_auth(fields["t"])
                    .header("Content-Type", "application/octet-stream")
                    .body(envelope)
                    .send()
                    .unwrap();
                assert_eq!(response.status().as_u16(), 201);
                true
            })
            .unwrap();
            assert!(result.is_some());
        }
        let flag = Arc::new(AtomicBool::new(false));
        let cancel = flag.clone();
        let result = receive_with_code(configured_origin().unwrap(), PhoneMode::Draw, flag, |_| {
            cancel.store(true, Ordering::Release);
            true
        })
        .unwrap();
        assert!(result.is_none());
    }
}
