//! The phone signature helper: pinned qrcp plus Butter Paper's adapter patch
//! and page (crates/butter-paper/phone-helper), built with Go for a target.

use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use serde::Deserialize;

use crate::{
    Args, Result, app_crate, archive, command, copy, create_dir, download, fresh_dir, output, read,
    read_text, exec, target::Target, target_dir, write,
};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Sources {
    qrcp: Source,
    signature_pad: Source,
}

#[derive(Deserialize)]
struct Source {
    url: String,
    sha256: String,
    revision: Option<String>,
}

pub const EXECUTABLE: &str = "butter-paper-signature-phone";
const COMMAND: &str = "./cmd/butter-paper-signature-phone";

pub struct PhoneHelper {
    pub executable: PathBuf,
    pub notices: String,
}

pub fn run(args: &mut Args) -> Result {
    let target = Target::parse(&args.required("--target")?)?;
    let out = PathBuf::from(args.required("--out")?);
    args.finish()?;
    let helper = build(target)?;
    copy(&helper.executable, &out.join(target.exe(EXECUTABLE)))?;
    write(&out.join("PHONE_HELPER_NOTICES.md"), &helper.notices)?;
    println!("{}", out.join(target.exe(EXECUTABLE)).display());
    Ok(())
}

pub fn build(target: Target) -> Result<PhoneHelper> {
    let helper = app_crate().join("phone-helper");
    let sources: Sources = serde_json::from_str(&read_text(&helper.join("sources.json"))?)
        .map_err(|error| format!("phone-helper/sources.json: {error}"))?;
    let work = target_dir().join("phone-helper");
    let downloads = work.join("downloads");
    let qrcp_archive = downloads.join("qrcp.tar.gz");
    let pad_archive = downloads.join("signature-pad.tgz");
    download(&sources.qrcp.url, &qrcp_archive, &sources.qrcp.sha256)?;
    download(&sources.signature_pad.url, &pad_archive, &sources.signature_pad.sha256)?;

    let source = work.join("source").join(target.triple());
    fresh_dir(&source)?;
    archive::extract_tar_gz(&read(&qrcp_archive)?, &source)?;
    archive::extract_tar_gz(&read(&pad_archive)?, &source)?;
    let revision = sources.qrcp.revision.as_deref().ok_or("qrcp needs a revision")?;
    let qrcp = source.join(format!("qrcp-{revision}"));
    exec(command("patch")
        .args(["-p1", "--fuzz=0", "--batch", "-i"])
        .arg(helper.join("qrcp-memory-adapter.patch"))
        .current_dir(&qrcp))?;

    let cmd = qrcp.join("cmd/butter-paper-signature-phone");
    copy(&helper.join("main.go"), &cmd.join("main.go"))?;
    for entry in fs::read_dir(helper.join("web")).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        copy(&entry.path(), &cmd.join("assets").join(entry.file_name()))?;
    }
    let pad = source.join("package");
    copy(&pad.join("dist/signature_pad.umd.min.js"), &cmd.join("assets/signature_pad.umd.min.js"))?;
    copy(&pad.join("LICENSE"), &cmd.join("assets/SIGNATURE_PAD_LICENSE"))?;

    let go = |args: &[&str]| {
        let mut go = command("go");
        go.args(args)
            .current_dir(&qrcp)
            .env("CGO_ENABLED", "0")
            .env("GOOS", target.go_os())
            .env("GOARCH", target.go_arch())
            .env("GOTOOLCHAIN", "local")
            .env("GOFLAGS", "-mod=readonly")
            .env("GOCACHE", work.join("go/cache"))
            .env("GOMODCACHE", work.join("go/modules"));
        go
    };
    let executable = work.join("bin").join(target.triple()).join(target.exe(EXECUTABLE));
    create_dir(executable.parent().unwrap())?;
    exec(go(&["build", "-trimpath", "-buildvcs=false", "-o"]).arg(&executable).arg(COMMAND))?;

    let modules = output(&mut go(&[
        "list",
        "-deps",
        "-f",
        "{{with .Module}}{{.Path}}\t{{.Version}}\t{{.Dir}}{{end}}",
        COMMAND,
    ]))?;
    let go_root = PathBuf::from(output(&mut go(&["env", "GOROOT"]))?);
    let go_version = output(&mut go(&["env", "GOVERSION"]))?;
    let go_license = [go_root.join("LICENSE"), go_root.parent().unwrap_or(&go_root).join("LICENSE")]
        .into_iter()
        .find(|path| path.is_file())
        .ok_or("the Go standard library licence is missing")?;
    let mut notices = render_go_notices(&go_version, &read_text(&go_license)?, &module_licences(&modules)?)?;
    notices.push_str(&format!(
        "## Signature Pad\n\n{}\n",
        read_text(&pad.join("LICENSE"))?.trim()
    ));
    Ok(PhoneHelper { executable, notices })
}

type Modules = BTreeMap<String, (String, Vec<(String, String)>)>;

/// Licence files for each module in `go list -deps` output.
fn module_licences(listing: &str) -> Result<Modules> {
    let mut modules = Modules::new();
    for row in listing.lines().filter(|row| !row.is_empty()) {
        let [path, version, directory] = row.split('\t').collect::<Vec<_>>()[..] else {
            return Err(format!("unexpected go list row: {row}"));
        };
        if modules.contains_key(path) {
            continue;
        }
        let mut licences = Vec::new();
        let mut names: Vec<_> = fs::read_dir(directory)
            .map_err(|error| format!("{directory}: {error}"))?
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.path().is_file())
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .filter(|name| is_licence_name(name))
            .collect();
        names.sort();
        for name in names {
            licences.push((name.clone(), read_text(&Path::new(directory).join(&name))?));
        }
        modules.insert(path.to_string(), (version.to_string(), licences));
    }
    Ok(modules)
}

pub fn is_licence_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    ["licence", "license", "copying", "notice"].iter().any(|stem| {
        lower == *stem || lower.strip_prefix(stem).is_some_and(|rest| rest.starts_with(['.', '-', '_']))
    })
}

fn render_go_notices(go_version: &str, go_license: &str, modules: &Modules) -> Result<String> {
    let mut text = format!(
        "# Phone signature helper\n\n## Go standard library ({go_version})\n\n{}\n\n",
        go_license.trim()
    );
    for (path, (version, licences)) in modules {
        if licences.is_empty() {
            return Err(format!("Go module {path} has no licence file"));
        }
        let version = if version.is_empty() { "pinned source" } else { version };
        text.push_str(&format!("## {path} ({version})\n\n"));
        for (name, licence) in licences {
            text.push_str(&format!("### {name}\n\n{}\n\n", licence.trim()));
        }
    }
    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn go_notices_list_every_module_and_reject_unlicensed_ones() {
        let mut modules = Modules::new();
        modules.insert("b.example/z".into(), ("v1.0.0".into(), vec![("LICENSE".into(), "B licence\n".into())]));
        modules.insert("a.example/y".into(), (String::new(), vec![("COPYING".into(), "A licence".into())]));
        let text = render_go_notices("go1.24.1", "Go licence", &modules).unwrap();
        assert!(text.starts_with("# Phone signature helper\n\n## Go standard library (go1.24.1)\n\nGo licence"));
        let a = text.find("## a.example/y (pinned source)").unwrap();
        let b = text.find("## b.example/z (v1.0.0)").unwrap();
        assert!(a < b);
        modules.insert("c.example/x".into(), ("v2".into(), Vec::new()));
        assert!(render_go_notices("go1.24.1", "Go licence", &modules).is_err());
    }

    #[test]
    fn licence_file_names() {
        for name in ["LICENSE", "License.txt", "COPYING", "NOTICE", "LICENSE-APACHE", "licence_mit"] {
            assert!(is_licence_name(name), "{name}");
        }
        for name in ["README.md", "licensed.go", "notices.go"] {
            assert!(!is_licence_name(name), "{name}");
        }
    }
}
