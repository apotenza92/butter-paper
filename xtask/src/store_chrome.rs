//! `cargo xtask store-chrome`: submits the Microsoft Store update through
//! Partner Center in the signed-in Google Chrome, for as long as the account
//! has no Store API credentials (see `store-submit`). macOS only.
//!
//! It opens its own Chrome window and drives it with AppleScript, which needs
//! Chrome's View > Developer > Allow JavaScript from Apple Events. The MSIX
//! files reach the page from a one-shot local server: Partner Center's upload
//! takes them from its file input, so nothing touches the user's files.

use std::{
    io::{BufRead as _, BufReader, Write as _},
    net::TcpListener,
    path::{Path, PathBuf},
    thread,
    time::{Duration, Instant},
};

use crate::{Args, Result, command, fresh_dir, output, target_dir, version};

/// Butter Paper's product in Partner Center.
const PRODUCT_ID: &str = "9N4KN0XNRNHC";
const PARTNER_CENTER: &str = "https://partner.microsoft.com/en-us/dashboard/products";

pub fn run(args: &mut Args) -> Result {
    let app = args
        .value("--app")?
        .unwrap_or_else(|| PRODUCT_ID.to_owned());
    let run_id = args.value("--run")?;
    let mut packages = Vec::new();
    while let Some(path) = args.value("--msix")? {
        packages.push(PathBuf::from(path));
    }
    args.finish()?;
    if !cfg!(target_os = "macos") {
        return Err("store-chrome drives Google Chrome on macOS".into());
    }
    if packages.is_empty() {
        packages = download_packages(run_id.as_deref())?;
    }
    for package in &packages {
        if !package.is_file() {
            return Err(format!("{} is not a file", package.display()));
        }
    }
    let version = format!("v{}.0", version::current()?.core());

    let chrome = Chrome::open(&format!("{PARTNER_CENTER}/{app}/overview"))?;
    // A session in another account lands on Partner Center's home page
    // instead of the product.
    chrome.wait_for(
        "document.body && (document.body.innerText.includes('Application overview') || location.pathname.endsWith('/dashboard/home') || location.host.startsWith('login.'))",
        // Partner Center can take minutes to render in a fresh window.
        Duration::from_secs(240),
    )?;
    if chrome.eval("document.body.innerText.includes('Application overview')")? != "true" {
        chrome.close();
        return Err("Chrome is not signed in to Partner Center as the developer account that owns the app: open Partner Center there, use the account menu's \"Sign in with another account\" to choose it, then run this again".into());
    }
    let overview = chrome.wait_for_text(&["Product release"], Duration::from_secs(180))?;
    if overview.contains("In certification") {
        return Err(
            "a submission is already in certification; wait for it or cancel it in Partner Center"
                .into(),
        );
    }
    let started = !overview.contains("In draft");
    if started {
        eprintln!("Starting an update");
        chrome.click_text("Start update")?;
        chrome.wait_for_text(&["In draft"], Duration::from_secs(90))?;
    }
    let overview_url = chrome.eval("location.href")?;
    let packages_url = chrome.eval(
        "[...document.querySelectorAll('a')].map(a => a.href).find(h => /\\/submissions\\/\\d+\\/packages$/.test(h)) || ''",
    )?;
    if packages_url.is_empty() {
        return Err("the draft submission has no Packages link".into());
    }
    chrome.navigate(&packages_url)?;
    chrome.wait_for(
        "!!document.querySelector('input[type=file]') && document.body.innerText.includes('Device family availability')",
        Duration::from_secs(180),
    )?;
    // The page settles (and may reload) shortly after it first renders.
    thread::sleep(Duration::from_secs(5));
    let listed = chrome.eval("document.body.innerText")?;
    if listed.contains(&format!("Version\t{version}")) || listed.contains(&format!("{version}, ")) {
        if started {
            chrome.navigate(&overview_url)?;
            chrome.wait_for_text(&["Delete submission"], Duration::from_secs(180))?;
            chrome.click_text("Delete submission")?;
            thread::sleep(Duration::from_secs(2));
            chrome.click_text("Yes")?;
            chrome.wait_for_text(&["Start update"], Duration::from_secs(120))?;
        }
        chrome.close();
        return Err(format!(
            "the Store already has {version}; nothing to submit"
        ));
    }

    eprintln!("Uploading {} packages", packages.len());
    let names = packages
        .iter()
        .map(|package| file_name(package))
        .collect::<Result<Vec<_>>>()?;
    let port = serve(packages.clone())?;
    let mut upload = String::new();
    for _ in 0..3 {
        chrome.eval(&upload_script(port, &names))?;
        let deadline = Instant::now() + Duration::from_secs(300);
        upload = loop {
            let state = chrome.eval("String(window.__bpUpload)")?;
            if state != "pending" || Instant::now() > deadline {
                break state;
            }
            thread::sleep(Duration::from_secs(2));
        };
        // "undefined": the page reloaded under the script; run it again.
        if upload != "undefined" {
            break;
        }
        thread::sleep(Duration::from_secs(5));
    }
    if upload != "set" {
        return Err(format!("the packages did not reach the page: {upload}"));
    }

    // Partner Center analyses each package, then marks the older ones for
    // removal.
    let deadline = Instant::now() + Duration::from_secs(20 * 60);
    loop {
        let text = chrome.eval("document.body.innerText")?;
        if packages_ready(&text, &version, names.len()) {
            break;
        }
        if text.contains("didn't finish processing") {
            return Err(
                "Partner Center could not process a package; remove it in the draft and rerun"
                    .into(),
            );
        }
        if Instant::now() > deadline {
            return Err("the packages were still processing after 20 minutes".into());
        }
        thread::sleep(Duration::from_secs(5));
    }
    eprintln!("Packages validated; saving");
    chrome.eval(
        "[...document.querySelectorAll('input[type=submit]')].find(e => e.value.trim() === 'Save').click(), 'ok'",
    )?;
    chrome.wait_for(
        "!location.pathname.endsWith('/packages')",
        Duration::from_secs(120),
    )?;
    chrome.wait_for_text(&["Submit for certification"], Duration::from_secs(60))?;
    chrome.click_text("Submit for certification")?;
    chrome.wait_for_text(&["In certification"], Duration::from_secs(180))?;
    eprintln!("Submitted {version} for certification; Microsoft publishes it once it passes");
    chrome.close();
    Ok(())
}

/// Downloads the Store packages from a release run (default: the latest).
fn download_packages(run_id: Option<&str>) -> Result<Vec<PathBuf>> {
    let run_id = match run_id {
        Some(id) => id.to_owned(),
        None => output(command("gh").args([
            "run",
            "list",
            "--workflow",
            "release.yml",
            "--status",
            "success",
            "--limit",
            "1",
            "--json",
            "databaseId",
            "--jq",
            ".[0].databaseId",
        ]))?,
    };
    let directory = target_dir().join("store-chrome");
    fresh_dir(&directory)?;
    eprintln!("Downloading the Store packages from release run {run_id}");
    output(
        command("gh")
            .args(["run", "download", &run_id, "-p", "microsoft-store-*", "-D"])
            .arg(&directory),
    )?;
    let packages = ["windows-x64", "windows-arm64"]
        .map(|target| {
            directory
                .join(format!("microsoft-store-{target}"))
                .join(format!(
                    "Butter-Paper-{}.msix",
                    target.replace("windows", "Windows")
                ))
        })
        .to_vec();
    Ok(packages)
}

/// Whether every new package is listed at `version`, nothing is still
/// uploading or being analysed, and each older package is marked for removal.
fn packages_ready(text: &str, version: &str, new_packages: usize) -> bool {
    if ["Uploading", "Analyzing package", "Validating"]
        .iter()
        .any(|busy| text.contains(busy))
    {
        return false;
    }
    let Some(listed) = text.split("Windows 10 packages").nth(1) else {
        return false;
    };
    let versions = listed
        .lines()
        .filter_map(|line| line.trim().strip_prefix("Version"))
        .map(str::trim)
        .collect::<Vec<_>>();
    let new = versions.iter().filter(|listed| **listed == version).count();
    let old = versions.len() - new;
    new == new_packages && listed.matches("will be removed").count() == old
}

/// Fetches the packages from the local server and gives them to the page's
/// file input; the outcome lands in `window.__bpUpload`.
fn upload_script(port: u16, names: &[String]) -> String {
    format!(
        "window.__bpUpload = 'pending'; (async () => {{ try {{ const transfer = new DataTransfer();
          for (const name of {names}) {{ const response = await fetch('http://127.0.0.1:{port}/' + name);
            transfer.items.add(new File([await response.blob()], name, {{type: 'application/octet-stream'}})); }}
          const input = document.querySelector('input[type=file]'); input.files = transfer.files;
          input.dispatchEvent(new Event('change', {{bubbles: true}})); window.__bpUpload = 'set';
        }} catch (error) {{ window.__bpUpload = 'error ' + error; }} }})(); 'started'",
        names = serde_json::to_string(names).expect("names serialise"),
    )
}

/// Serves the files by name to Partner Center's page (CORS and the
/// private-network preflight) for as long as the command runs.
fn serve(files: Vec<PathBuf>) -> Result<u16> {
    let listener =
        TcpListener::bind("127.0.0.1:0").map_err(|error| format!("local server: {error}"))?;
    let port = listener
        .local_addr()
        .map_err(|error| format!("local server: {error}"))?
        .port();
    thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let mut request = String::new();
            let mut reader = BufReader::new(&stream);
            if reader.read_line(&mut request).is_err() {
                continue;
            }
            let mut header = String::new();
            while reader.read_line(&mut header).is_ok_and(|read| read > 2) {
                header.clear();
            }
            let cors = "Access-Control-Allow-Origin: https://partner.microsoft.com\r\nAccess-Control-Allow-Private-Network: true\r\nAccess-Control-Allow-Headers: *\r\nConnection: close\r\n";
            let mut parts = request.split_whitespace();
            let (method, path) = (parts.next().unwrap_or(""), parts.next().unwrap_or(""));
            if method == "OPTIONS" {
                let _ = write!(
                    stream,
                    "HTTP/1.1 204 No Content\r\n{cors}Content-Length: 0\r\n\r\n"
                );
                continue;
            }
            let name = path.trim_start_matches('/');
            let file = files
                .iter()
                .find(|file| file.file_name().is_some_and(|file| file == name));
            match file.and_then(|file| std::fs::read(file).ok()) {
                Some(bytes) => {
                    let _ = write!(
                        stream,
                        "HTTP/1.1 200 OK\r\n{cors}Content-Type: application/octet-stream\r\nContent-Length: {}\r\n\r\n",
                        bytes.len()
                    );
                    let _ = stream.write_all(&bytes);
                }
                None => {
                    let _ = write!(
                        stream,
                        "HTTP/1.1 404 Not Found\r\n{cors}Content-Length: 0\r\n\r\n"
                    );
                }
            }
        }
    });
    Ok(port)
}

fn file_name(path: &Path) -> Result<String> {
    path.file_name()
        .and_then(|name| name.to_str())
        .map(str::to_owned)
        .ok_or_else(|| format!("{} has no file name", path.display()))
}

/// A Chrome window of its own, driven through AppleScript.
struct Chrome {
    window: String,
}

impl Chrome {
    fn open(url: &str) -> Result<Self> {
        let window = osascript(&format!(
            "tell application \"Google Chrome\"\nset w to make new window\nset URL of active tab of w to {}\nreturn id of w\nend tell",
            applescript_string(url)
        ))?;
        let chrome = Self { window };
        // Fails early with Chrome's own message when Apple Events JavaScript is off.
        thread::sleep(Duration::from_secs(2));
        chrome.eval("'ok'")?;
        Ok(chrome)
    }

    fn eval(&self, javascript: &str) -> Result<String> {
        osascript(&format!(
            "tell application \"Google Chrome\" to execute active tab of window id {} javascript {}",
            self.window,
            applescript_string(javascript)
        ))
        .map_err(|error| {
            if error.contains("Apple Events") || error.contains("JavaScript") {
                format!("{error}\nTurn on Chrome's View > Developer > Allow JavaScript from Apple Events")
            } else {
                error
            }
        })
    }

    fn navigate(&self, url: &str) -> Result {
        osascript(&format!(
            "tell application \"Google Chrome\" to set URL of active tab of window id {} to {}",
            self.window,
            applescript_string(url)
        ))
        .map(|_| ())
    }

    /// Waits until `condition` (JavaScript) is true.
    fn wait_for(&self, condition: &str, limit: Duration) -> Result {
        let deadline = Instant::now() + limit;
        loop {
            if self
                .eval(&format!("!!({condition})"))
                .is_ok_and(|value| value == "true")
            {
                return Ok(());
            }
            if Instant::now() > deadline {
                return Err(format!(
                    "Partner Center did not reach `{condition}` in time"
                ));
            }
            thread::sleep(Duration::from_secs(2));
        }
    }

    /// Waits until the page shows any of `texts`, and returns its text.
    fn wait_for_text(&self, texts: &[&str], limit: Duration) -> Result<String> {
        let deadline = Instant::now() + limit;
        loop {
            let text = self
                .eval("document.body ? document.body.innerText : ''")
                .unwrap_or_default();
            if texts.iter().any(|wanted| text.contains(wanted)) {
                return Ok(text);
            }
            if Instant::now() > deadline {
                return Err(format!("Partner Center did not show {texts:?} in time"));
            }
            thread::sleep(Duration::from_secs(2));
        }
    }

    /// Clicks the control labelled exactly `label`, including Partner
    /// Center's `he-button` elements.
    fn click_text(&self, label: &str) -> Result {
        let clicked = self.eval(&format!(
            "(() => {{ const label = {}; const found = [];
               const walk = root => root.querySelectorAll('*').forEach(e => {{
                 if (/^(HE-BUTTON|BUTTON|A|INPUT)$/.test(e.tagName) && (e.value || e.textContent || '').trim() === label) found.push(e);
                 if (e.shadowRoot) walk(e.shadowRoot); }});
               walk(document); const target = found.pop();
               if (!target) return 'missing'; target.click(); return 'clicked'; }})()",
            serde_json::to_string(label).expect("label serialises")
        ))?;
        if clicked == "clicked" {
            Ok(())
        } else {
            Err(format!("Partner Center has no `{label}` control"))
        }
    }

    fn close(&self) {
        let _ = osascript(&format!(
            "tell application \"Google Chrome\" to close window id {}",
            self.window
        ));
    }
}

fn osascript(script: &str) -> Result<String> {
    let mut child = command("osascript")
        .arg("-")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|error| format!("osascript: {error}"))?;
    child
        .stdin
        .take()
        .ok_or("osascript has no stdin")?
        .write_all(script.as_bytes())
        .map_err(|error| format!("osascript: {error}"))?;
    let result = child
        .wait_with_output()
        .map_err(|error| format!("osascript: {error}"))?;
    if result.status.success() {
        Ok(String::from_utf8_lossy(&result.stdout).trim().to_owned())
    } else {
        Err(String::from_utf8_lossy(&result.stderr).trim().to_owned())
    }
}

/// An AppleScript string literal.
fn applescript_string(value: &str) -> String {
    let mut quoted = String::with_capacity(value.len() + 2);
    quoted.push('"');
    for character in value.chars() {
        match character {
            '"' => quoted.push_str("\\\""),
            '\\' => quoted.push_str("\\\\"),
            '\n' => quoted.push_str("\\n"),
            '\r' => quoted.push_str("\\r"),
            '\t' => quoted.push_str("\\t"),
            other => quoted.push(other),
        }
    }
    quoted.push('"');
    quoted
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn applescript_strings_escape_quotes_backslashes_and_newlines() {
        assert_eq!(
            applescript_string("a \"b\" \\ c\nd"),
            "\"a \\\"b\\\" \\\\ c\\nd\""
        );
    }

    #[test]
    fn packages_are_ready_once_new_versions_are_in_and_old_ones_marked() {
        let page = "Drag your packages here\nWindows 10 packages\n\
            Butter-Paper-Windows-x64.msix\tShow details\n\
            This package will be removed after you save this page because you have a higher-versioned package.\n\
            Version\tv0.2.2.0\nArchitecture\tX64\n\
            Butter-Paper-Windows-arm64.msix\tShow details\n\
            This package will be removed after you save this page because you have a higher-versioned package.\n\
            Version\tv0.2.2.0\nArchitecture\tArm64\n\
            Butter-Paper-Windows-x64.msix\tShow details\nVersion\tv0.2.3.0\nArchitecture\tX64\n\
            Butter-Paper-Windows-arm64.msix\tShow details\nVersion\tv0.2.3.0\nArchitecture\tArm64\n";
        assert!(packages_ready(page, "v0.2.3.0", 2));
        assert!(!packages_ready(page, "v0.2.3.0", 3), "a package is missing");
        let analysing = format!("Butter-Paper-Windows-x64.msix\n99\nAnalyzing package\n{page}");
        assert!(!packages_ready(&analysing, "v0.2.3.0", 2));
        let kept = page.replacen("This package will be removed", "Kept", 1);
        assert!(
            !packages_ready(&kept, "v0.2.3.0", 2),
            "an old package stays"
        );
    }
}
