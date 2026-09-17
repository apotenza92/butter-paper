//! Cancellable native camera boundary; captured images never touch disk.
use crate::image_asset_decode::{SanitizedSignatureFile, sanitize_signature_bytes};
use std::{
    io::Read,
    path::PathBuf,
    process::{Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
use zeroize::Zeroizing;

pub fn helper_path() -> Option<PathBuf> {
    if !cfg!(target_os = "macos") {
        return None;
    }
    let path = std::env::current_exe()
        .ok()?
        .parent()?
        .join("butter-paper-signature-camera");
    path.is_file().then_some(path)
}

pub fn capture(cancelled: Arc<AtomicBool>) -> Result<Option<SanitizedSignatureFile>, String> {
    let helper = helper_path().ok_or("Camera capture is unavailable in this build.")?;
    capture_from(helper, cancelled)
}
fn capture_from(
    helper: PathBuf,
    cancelled: Arc<AtomicBool>,
) -> Result<Option<SanitizedSignatureFile>, String> {
    if cancelled.load(Ordering::Acquire) {
        return Ok(None);
    }
    let mut child = Command::new(helper)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| "The camera could not be started.")?;
    let stdout = child
        .stdout
        .take()
        .ok_or("The camera output is unavailable.")?;
    // Read concurrently to avoid a full pipe blocking process exit; cap allocation.
    let reader = std::thread::spawn(move || {
        let mut bytes = Zeroizing::new(Vec::new());
        stdout
            .take(16 * 1024 * 1024 + 1)
            .read_to_end(&mut bytes)
            .map(|_| bytes)
    });
    let start = Instant::now();
    let status = loop {
        if cancelled.load(Ordering::Acquire) || start.elapsed() > Duration::from_secs(125) {
            let _ = child.kill();
            let _ = child.wait();
            let _ = reader.join();
            return Ok(None);
        }
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => std::thread::sleep(Duration::from_millis(50)),
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = reader.join();
                return Err("The camera stopped unexpectedly.".into());
            }
        }
    };
    let bytes = reader
        .join()
        .map_err(|_| "The camera image could not be read.")?
        .map_err(|_| "The camera image could not be read.")?;
    if cancelled.load(Ordering::Acquire) || status.code() == Some(2) {
        return Ok(None);
    }
    if !status.success() {
        return Err("Camera access was denied or the camera is unavailable.".into());
    }
    sanitize_signature_bytes(&bytes)
        .map(Some)
        .map_err(|_| "The captured signature could not be processed.".into())
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use image::ImageEncoder;
    use std::os::unix::fs::PermissionsExt;
    #[test]
    fn camera_pipe_sanitizes_image_and_handles_failure_and_cancel() {
        let root = std::env::temp_dir().join(format!("bp-camera-pipe-test-{}", std::process::id()));
        std::fs::create_dir(&root).unwrap();
        let helper = root.join("capture");
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
        let octal = png
            .iter()
            .map(|b| format!("\\{:03o}", b))
            .collect::<String>();
        std::fs::write(&helper, format!("#!/bin/sh\nprintf '{octal}'\n")).unwrap();
        std::fs::set_permissions(&helper, std::fs::Permissions::from_mode(0o700)).unwrap();
        let result = capture_from(helper.clone(), Arc::new(AtomicBool::new(false))).unwrap();
        assert!(result.is_some());
        std::fs::write(&helper, "#!/bin/sh\nexit 1\n").unwrap();
        assert!(capture_from(helper.clone(), Arc::new(AtomicBool::new(false))).is_err());
        std::fs::write(&helper, "#!/bin/sh\nexit 2\n").unwrap();
        assert!(
            capture_from(helper.clone(), Arc::new(AtomicBool::new(false)))
                .unwrap()
                .is_none()
        );
        std::fs::write(&helper, "#!/bin/sh\nexec /bin/sleep 30\n").unwrap();
        let cancel = Arc::new(AtomicBool::new(false));
        let signal = cancel.clone();
        let task = std::thread::spawn(move || capture_from(helper, cancel));
        std::thread::sleep(Duration::from_millis(100));
        signal.store(true, Ordering::Release);
        assert!(task.join().unwrap().unwrap().is_none());
        std::fs::remove_dir_all(root).unwrap();
    }
}
