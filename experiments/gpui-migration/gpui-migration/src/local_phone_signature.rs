//! Application-owned lifetime for the bundled qrcp signature helper.
use crate::{
    annotation_model::DecodedRgbaAsset,
    image_asset_decode::{SanitizedSignatureFile, sanitize_signature_bytes},
    phone_signature::PhoneMode,
};
use base64::{Engine as _, engine::general_purpose::STANDARD as B64};
use std::{
    io::{BufRead, BufReader, Read},
    path::PathBuf,
    process::{Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};
use zeroize::Zeroizing;

const MAX_RECORD: u64 = 2 * 1024 * 1024;

pub fn helper_path() -> Option<PathBuf> {
    let name = if cfg!(windows) {
        "butter-paper-signature-phone.exe"
    } else {
        "butter-paper-signature-phone"
    };
    let path = std::env::current_exe().ok()?.parent()?.join(name);
    path.is_file().then_some(path)
}

pub fn receive(
    mode: PhoneMode,
    cancelled: Arc<AtomicBool>,
    show_qr: impl FnOnce(DecodedRgbaAsset) -> bool,
) -> Result<Option<SanitizedSignatureFile>, String> {
    let helper = helper_path().ok_or("Phone signing is unavailable in this build.")?;
    receive_from(helper, "auto", mode, cancelled, show_qr)
}

fn receive_from(
    helper: PathBuf,
    bind: &str,
    mode: PhoneMode,
    cancelled: Arc<AtomicBool>,
    show_qr: impl FnOnce(DecodedRgbaAsset) -> bool,
) -> Result<Option<SanitizedSignatureFile>, String> {
    if cancelled.load(Ordering::Acquire) {
        return Ok(None);
    }
    let mut child = Command::new(helper)
        .args([
            "--bind",
            bind,
            "--mode",
            if mode == PhoneMode::Draw {
                "draw"
            } else {
                "image"
            },
            "--watch-parent",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| "Cannot start phone signing.")?;
    // Closing this pipe also ends the helper if the application crashes.
    let parent_pipe = child.stdin.take();
    let stdout = child.stdout.take().expect("piped helper stdout");
    let (tx, rx) = mpsc::channel();
    let reader = std::thread::spawn(move || {
        let mut reader = BufReader::new(stdout);
        // Exactly one ready record and one result; never accumulate an unbounded stream.
        for _ in 0..2 {
            let mut record = Zeroizing::new(Vec::new());
            let read = reader
                .by_ref()
                .take(MAX_RECORD + 1)
                .read_until(b'\n', &mut record);
            if !matches!(read, Ok(1..)) || record.len() > MAX_RECORD as usize {
                break;
            }
            if tx.send(record).is_err() {
                break;
            }
        }
    });
    let start = Instant::now();
    let mut show_qr = Some(show_qr);
    let result = loop {
        if cancelled.load(Ordering::Acquire) {
            break Ok(None);
        }
        if start.elapsed() > Duration::from_secs(310) {
            break Err("Session expired. Try again.".into());
        }
        match rx.recv_timeout(Duration::from_millis(50)) {
            Ok(record) => {
                let event: serde_json::Value = match serde_json::from_slice(&record) {
                    Ok(value) => value,
                    Err(_) => break Err("Cannot read the phone signature.".into()),
                };
                match event["event"].as_str() {
                    Some("ready") if show_qr.is_some() => {
                        let qr = event["qrPng"]
                            .as_str()
                            .and_then(|s| B64.decode(s).ok())
                            .and_then(|bytes| {
                                image::load_from_memory_with_format(&bytes, image::ImageFormat::Png)
                                    .ok()
                            })
                            .and_then(|image| {
                                let rgba = image.into_rgba8();
                                DecodedRgbaAsset::new(rgba.width(), rgba.height(), rgba.into_raw())
                                    .ok()
                            });
                        match qr {
                            Some(asset) => {
                                if !show_qr.take().unwrap()(asset) {
                                    break Ok(None);
                                }
                            }
                            _ => break Ok(None),
                        }
                    }
                    Some("received") if show_qr.is_none() => {
                        let bytes = event["png"]
                            .as_str()
                            .and_then(|s| B64.decode(s).ok())
                            .map(Zeroizing::new);
                        break match bytes {
                            Some(bytes) if bytes.len() <= 1024 * 1024 => {
                                sanitize_signature_bytes(&bytes).map(Some).map_err(|_| {
                                    "Cannot use this signature. Try another image.".into()
                                })
                            }
                            _ => Err("Cannot read the phone signature.".into()),
                        };
                    }
                    Some("cancelled") => break Ok(None),
                    Some("expired") => break Err("Session expired. Try again.".into()),
                    _ => break Err("Cannot read the phone signature.".into()),
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
            Err(_) => break Err("Cannot connect. Check your Wi-Fi and try again.".into()),
        }
    };
    drop(parent_pipe);
    // A bounded grace lets qrcp flush the browser response on success.
    let end = Instant::now();
    while matches!(child.try_wait(), Ok(None)) && end.elapsed() < Duration::from_secs(2) {
        std::thread::sleep(Duration::from_millis(20));
    }
    let _ = child.kill();
    let _ = child.wait();
    let _ = reader.join();
    if cancelled.load(Ordering::Acquire) {
        Ok(None)
    } else {
        result
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use image::ImageEncoder;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn helper_delivery_is_sanitized_and_dismissal_cancels() {
        let root = std::env::temp_dir().join(format!("bp-local-phone-test-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let helper = root.join("helper");
        let mut rgba = vec![255u8; 32 * 16 * 4];
        for y in 7..10 {
            for x in 4..28 {
                rgba[(y * 32 + x) * 4..(y * 32 + x) * 4 + 3].fill(0);
            }
        }
        let mut png = Vec::new();
        image::codecs::png::PngEncoder::new(&mut png)
            .write_image(&rgba, 32, 16, image::ExtendedColorType::Rgba8)
            .unwrap();
        let ready = serde_json::json!({"event":"ready","qrPng":B64.encode(&png)});
        let received = serde_json::json!({"event":"received","png":B64.encode(&png)});
        std::fs::write(
            &helper,
            format!("#!/bin/sh\nprintf '%s\\n' '{ready}' '{received}'\n"),
        )
        .unwrap();
        std::fs::set_permissions(&helper, std::fs::Permissions::from_mode(0o700)).unwrap();
        let cancelled = Arc::new(AtomicBool::new(false));
        let result = receive_from(
            helper.clone(),
            "127.0.0.1",
            PhoneMode::Draw,
            cancelled.clone(),
            |_| true,
        )
        .unwrap();
        assert!(result.is_some());
        assert!(
            receive_from(
                helper.clone(),
                "127.0.0.1",
                PhoneMode::Draw,
                cancelled.clone(),
                |_| false
            )
            .unwrap()
            .is_none()
        );
        cancelled.store(true, Ordering::Release);
        assert!(
            receive_from(
                helper.clone(),
                "127.0.0.1",
                PhoneMode::Draw,
                cancelled,
                |_| panic!("cancelled operation must not show QR")
            )
            .unwrap()
            .is_none()
        );
        std::fs::write(&helper, "#!/bin/sh\nprintf '%s\\n' broken\n").unwrap();
        assert!(
            receive_from(
                helper,
                "127.0.0.1",
                PhoneMode::Draw,
                Arc::new(AtomicBool::new(false)),
                |_| true
            )
            .is_err()
        );
        std::fs::remove_dir_all(root).unwrap();
    }
}
