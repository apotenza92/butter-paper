//! The signing settings Settings ▸ Signatures keeps: the timestamp server
//! digital signatures ask for a trusted time. None is set at first, as in
//! Acrobat and Revu; then signatures carry no timestamp.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

pub const SIGNATURE_SETTINGS_FILE_NAME: &str = "signature-settings.json";

/// The application-wide signing settings.
#[derive(Clone, Debug, Default)]
pub struct SignatureSettings {
    timestamp_server: Option<String>,
    /// Where they are kept; `None` keeps them for this run only.
    path: Option<PathBuf>,
}

impl gpui::Global for SignatureSettings {}

#[derive(Default, Deserialize, Serialize)]
struct Stored {
    #[serde(default)]
    timestamp_server: Option<String>,
}

impl SignatureSettings {
    /// The settings kept at `path`, or none set.
    pub fn load(path: impl Into<PathBuf>) -> Self {
        let path = path.into();
        let stored: Stored = std::fs::read(&path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        Self {
            timestamp_server: stored
                .timestamp_server
                .and_then(|server| timestamp_server_from_text(&server).ok().flatten()),
            path: Some(path),
        }
    }

    /// The timestamp server set in Settings, if any.
    pub fn timestamp_server(cx: &gpui::App) -> Option<String> {
        cx.try_global::<Self>()
            .and_then(|settings| settings.timestamp_server.clone())
    }

    /// Sets the timestamp server (`None` for none) and keeps it.
    pub fn set_timestamp_server(server: Option<String>, cx: &mut gpui::App) {
        let mut settings = cx.try_global::<Self>().cloned().unwrap_or_default();
        if settings.timestamp_server == server {
            return;
        }
        settings.timestamp_server = server;
        if let Some(path) = &settings.path
            && let Err(error) = save(path, &settings)
        {
            eprintln!("unable to save Butter Paper signature settings: {error}");
        }
        cx.set_global(settings);
    }
}

fn save(path: &Path, settings: &SignatureSettings) -> std::io::Result<()> {
    let stored = Stored {
        timestamp_server: settings.timestamp_server.clone(),
    };
    let bytes = serde_json::to_vec_pretty(&stored).map_err(std::io::Error::other)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let partial = path.with_extension("json.part");
    std::fs::write(&partial, bytes)?;
    std::fs::rename(partial, path)
}

/// A timestamp server typed into Settings: empty for none, else an
/// `http` or `https` address.
pub fn timestamp_server_from_text(text: &str) -> Result<Option<String>, &'static str> {
    let text = text.trim();
    if text.is_empty() {
        return Ok(None);
    }
    match reqwest::Url::parse(text) {
        Ok(url) if matches!(url.scheme(), "http" | "https") && url.host().is_some() => {
            Ok(Some(text.to_owned()))
        }
        _ => Err("Enter the server's full address, such as http://timestamp.digicert.com."),
    }
}

/// The server's host, to name it in the signing dialog.
pub fn timestamp_server_host(server: &str) -> String {
    reqwest::Url::parse(server)
        .ok()
        .and_then(|url| url.host_str().map(str::to_owned))
        .unwrap_or_else(|| server.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timestamp_server_addresses_are_checked() {
        assert_eq!(timestamp_server_from_text("  "), Ok(None));
        assert_eq!(
            timestamp_server_from_text(" http://timestamp.digicert.com "),
            Ok(Some("http://timestamp.digicert.com".to_owned()))
        );
        assert!(timestamp_server_from_text("timestamp.digicert.com").is_err());
        assert!(timestamp_server_from_text("ftp://example.com").is_err());
        assert_eq!(
            timestamp_server_host("http://timestamp.digicert.com/tsa"),
            "timestamp.digicert.com"
        );
    }

    #[test]
    fn settings_are_kept_between_runs() {
        let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!(
            "target/test-scratch/signature-settings-{}",
            std::process::id()
        ));
        let path = directory.join(SIGNATURE_SETTINGS_FILE_NAME);
        let _ = std::fs::remove_dir_all(&directory);
        assert_eq!(
            SignatureSettings::load(&path).timestamp_server,
            None,
            "none at first"
        );
        let mut settings = SignatureSettings::load(&path);
        settings.timestamp_server = Some("https://tsa.example.com".into());
        save(&path, &settings).unwrap();
        assert_eq!(
            SignatureSettings::load(&path).timestamp_server.as_deref(),
            Some("https://tsa.example.com")
        );
        let _ = std::fs::remove_dir_all(directory);
    }
}
