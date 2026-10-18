//! `cargo xtask store-submit`: replaces the Microsoft Store packages with the
//! given MSIX files and submits the update for certification, through the
//! Microsoft Store submission API.
//!
//! It authenticates as the Microsoft Entra application that Partner Center
//! associates with the developer account (User management > Microsoft Entra
//! applications, Manager role). Its credentials come only from the
//! environment and reach curl through standard input, never its arguments:
//! STORE_TENANT_ID, STORE_CLIENT_ID and STORE_CLIENT_SECRET.

use std::{
    io::Write as _,
    path::{Path, PathBuf},
    process::Stdio,
    time::{Duration, Instant},
};

use serde_json::{Value, json};

use crate::{Args, Result, command, create_dir, fresh_dir, output, target_dir};

const API: &str = "https://manage.devcenter.microsoft.com/v1.0/my/applications";
/// Pre-processing usually finishes in minutes; give up well before the
/// workflow's own timeout.
const COMMIT_LIMIT: Duration = Duration::from_secs(45 * 60);

pub fn run(args: &mut Args) -> Result {
    let app = args.required("--app")?;
    let mut packages = Vec::new();
    while let Some(path) = args.value("--msix")? {
        packages.push(PathBuf::from(path));
    }
    args.finish()?;
    if packages.is_empty() {
        return Err("store-submit needs at least one --msix".into());
    }
    for package in &packages {
        if !package.is_file() {
            return Err(format!("{} is not a file", package.display()));
        }
    }
    let token = access_token()?;
    let base = format!("{API}/{app}");

    // An earlier draft would block a new submission; one already in
    // certification is left alone and reported.
    let application = api(&token, "GET", &base, None)?;
    if let Some(pending) = application["pendingApplicationSubmission"]["id"].as_str() {
        let status = api(
            &token,
            "GET",
            &format!("{base}/submissions/{pending}/status"),
            None,
        )?;
        if status["status"] != "PendingCommit" {
            return Err(format!(
                "submission {pending} is already {}; wait for it or cancel it in Partner Center",
                status["status"]
            ));
        }
        eprintln!("Deleting the unsubmitted draft {pending}");
        api(
            &token,
            "DELETE",
            &format!("{base}/submissions/{pending}"),
            None,
        )?;
    }

    let created = api(
        &token,
        "POST",
        &format!("{base}/submissions"),
        Some(&json!({})),
    )?;
    let id = created["id"]
        .as_str()
        .ok_or("the new submission has no id")?
        .to_owned();
    let upload_url = created["fileUploadUrl"]
        .as_str()
        .ok_or("the new submission has no upload URL")?
        .to_owned();
    eprintln!("Created submission {id}");

    let names = packages
        .iter()
        .map(|package| file_name(package))
        .collect::<Result<Vec<_>>>()?;
    let updated = replace_packages(created, &names)?;
    upload(&upload_url, &packages)?;
    api(
        &token,
        "PUT",
        &format!("{base}/submissions/{id}"),
        Some(&updated),
    )?;
    api(
        &token,
        "POST",
        &format!("{base}/submissions/{id}/commit"),
        None,
    )?;
    eprintln!("Committed submission {id}; waiting for pre-processing");

    let started = Instant::now();
    loop {
        std::thread::sleep(Duration::from_secs(30));
        let status = api(
            &token,
            "GET",
            &format!("{base}/submissions/{id}/status"),
            None,
        )?;
        match status["status"].as_str().unwrap_or_default() {
            "CommitStarted" if started.elapsed() < COMMIT_LIMIT => continue,
            "CommitStarted" => {
                return Err(format!(
                    "submission {id} was still committing after {COMMIT_LIMIT:?}; check Partner Center"
                ));
            }
            "CommitFailed" => {
                return Err(format!(
                    "submission {id} failed to commit: {}",
                    status["statusDetails"]
                ));
            }
            other => {
                eprintln!("Submission {id} is {other}: Microsoft certifies and then publishes it");
                return Ok(());
            }
        }
    }
}

/// The new submission (a copy of the live one) with its packages replaced:
/// every existing package is deleted and each new file is uploaded.
fn replace_packages(mut submission: Value, files: &[String]) -> Result<Value> {
    let packages = submission["applicationPackages"]
        .as_array_mut()
        .ok_or("the submission has no applicationPackages")?;
    for package in packages.iter_mut() {
        package["fileStatus"] = json!("PendingDelete");
    }
    for file in files {
        packages.push(json!({
            "fileName": file,
            "fileStatus": "PendingUpload",
            "minimumDirectXVersion": "None",
            "minimumSystemRam": "None",
        }));
    }
    Ok(submission)
}

fn file_name(path: &Path) -> Result<String> {
    path.file_name()
        .and_then(|name| name.to_str())
        .map(str::to_owned)
        .ok_or_else(|| format!("{} has no file name", path.display()))
}

fn access_token() -> Result<String> {
    let secret = |name: &str| {
        std::env::var(name)
            .ok()
            .filter(|value| !value.is_empty())
            .ok_or_else(|| format!("{name} is not set"))
    };
    let tenant = secret("STORE_TENANT_ID")?;
    let form = format!(
        "grant_type=client_credentials&resource=https%3A%2F%2Fmanage.devcenter.microsoft.com&client_id={}&client_secret={}",
        encode(&secret("STORE_CLIENT_ID")?),
        encode(&secret("STORE_CLIENT_SECRET")?),
    );
    let config = format!(
        "url = \"https://login.microsoftonline.com/{tenant}/oauth2/token\"\nrequest = \"POST\"\ndata = \"{form}\"\n"
    );
    let response = curl(&config)?;
    let value: Value =
        serde_json::from_str(&response).map_err(|error| format!("token response: {error}"))?;
    value["access_token"]
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| format!("no access token: {}", value["error_description"]))
}

/// One Store API call. The token and body go through curl's configuration
/// on standard input, so neither appears in a process listing.
fn api(token: &str, method: &str, url: &str, body: Option<&Value>) -> Result<Value> {
    let work = target_dir().join("store-submit");
    create_dir(&work)?;
    let mut config = format!(
        "url = \"{url}\"\nrequest = \"{method}\"\nheader = \"Authorization: Bearer {token}\"\nheader = \"Content-Type: application/json\"\n"
    );
    if let Some(body) = body {
        let path = work.join("body.json");
        crate::write(
            &path,
            serde_json::to_vec(body).map_err(|error| error.to_string())?,
        )?;
        config.push_str(&format!("data-binary = \"@{}\"\n", path.display()));
    } else if method == "POST" {
        config.push_str("data = \"\"\n");
    }
    let response = curl(&config).map_err(|error| format!("{method} {url}: {error}"))?;
    if response.trim().is_empty() {
        return Ok(Value::Null);
    }
    serde_json::from_str(&response).map_err(|error| format!("{method} {url}: {error}"))
}

fn curl(config: &str) -> Result<String> {
    let mut child = command("curl")
        .args([
            "--silent",
            "--show-error",
            "--fail-with-body",
            "--retry",
            "3",
            "--config",
            "-",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("curl: {error}"))?;
    child
        .stdin
        .take()
        .ok_or("curl has no stdin")?
        .write_all(config.as_bytes())
        .map_err(|error| format!("curl: {error}"))?;
    let result = child
        .wait_with_output()
        .map_err(|error| format!("curl: {error}"))?;
    let body = String::from_utf8_lossy(&result.stdout).into_owned();
    if result.status.success() {
        Ok(body)
    } else {
        Err(format!(
            "{} {}",
            String::from_utf8_lossy(&result.stderr).trim(),
            body.trim()
        ))
    }
}

/// Uploads the packages as the one zip the submission's upload URL expects.
fn upload(url: &str, packages: &[PathBuf]) -> Result {
    let work = target_dir().join("store-submit");
    fresh_dir(&work)?;
    let zip = work.join("packages.zip");
    let mut zip_command = command("zip");
    zip_command.arg("-j").arg(&zip).args(packages);
    output(&mut zip_command)?;
    let config = format!(
        "url = \"{url}\"\nrequest = \"PUT\"\nheader = \"x-ms-blob-type: BlockBlob\"\nupload-file = \"{}\"\n",
        zip.display()
    );
    curl(&config).map(|_| ())
}

/// Percent-encodes a form value.
fn encode(value: &str) -> String {
    value
        .bytes()
        .map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                (byte as char).to_string()
            }
            _ => format!("%{byte:02X}"),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replacing_packages_deletes_the_live_ones_and_uploads_the_new_files() {
        let live = json!({
            "id": "1",
            "applicationPackages": [
                {"fileName": "Butter-Paper-Windows-x64.msix", "fileStatus": "Uploaded", "id": "a"},
                {"fileName": "Butter-Paper-Windows-arm64.msix", "fileStatus": "Uploaded", "id": "b"},
            ],
            "listings": {"en-us": {}},
        });
        let files = [
            "Butter-Paper-Windows-x64.msix".to_owned(),
            "Butter-Paper-Windows-arm64.msix".to_owned(),
        ];
        let updated = replace_packages(live, &files).unwrap();
        let packages = updated["applicationPackages"].as_array().unwrap();
        assert_eq!(packages.len(), 4);
        assert!(
            packages[..2]
                .iter()
                .all(|package| package["fileStatus"] == "PendingDelete")
        );
        assert_eq!(packages[2]["fileName"], "Butter-Paper-Windows-x64.msix");
        assert!(
            packages[2..]
                .iter()
                .all(|package| package["fileStatus"] == "PendingUpload")
        );
        // Everything else is the live submission's.
        assert_eq!(updated["listings"], json!({"en-us": {}}));
    }

    #[test]
    fn form_values_are_percent_encoded() {
        assert_eq!(encode("a+b/c=d~e"), "a%2Bb%2Fc%3Dd~e");
    }
}
