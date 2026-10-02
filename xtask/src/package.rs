//! Release packages: a signed and notarised macOS app (Butter Paper and
//! Butter Paper Beta), a Windows zip and a Linux tar.xz, each named as the
//! updater and Homebrew expect.

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use serde_json::json;

use crate::{
    Args, Result, app_crate, archive::{self, Entry}, command, copy, create_dir, fresh_dir, notices,
    output, pdfium, phone, read, read_text, root, exec, set_mode, sha256,
    target::{Arch, Channel, Os, Target},
    target_dir,
    version::{self, Version},
    write,
};

pub const MINIMUM_MACOS: &str = "13.0";
const SIGNING_IDENTITY: &str = "Developer ID Application: Alexander Potenza (27JL2VERNC)";
const TEAM_ID: &str = "27JL2VERNC";
const APP: &str = "butter-paper";
const WORKER: &str = "butter-paper-pdf-worker";
const CAMERA: &str = "butter-paper-signature-camera";

pub fn run(args: &mut Args) -> Result {
    let target = Target::parse(&args.required("--target")?)?;
    let channels = args
        .value("--channel")?
        .unwrap_or_else(|| "stable".into())
        .split(',')
        .map(Channel::parse)
        .collect::<Result<Vec<_>>>()?;
    let out = PathBuf::from(args.required("--out")?);
    let sign = args.flag("--sign");
    let binaries = args.value("--binaries")?.map(PathBuf::from);
    args.finish()?;
    let version = version::current()?;
    if target.os != Os::Macos && channels != [Channel::Stable] {
        return Err("Butter Paper Beta is macOS-only".into());
    }
    if channels.contains(&Channel::Stable) && version.is_beta() {
        return Err(format!("{version} is a beta; package the Beta channel only"));
    }
    if sign && target.os != Os::Macos {
        return Err("only macOS packages are signed".into());
    }
    for path in package(target, &channels, version, &out, sign, binaries.as_deref())? {
        println!("{}", path.display());
    }
    Ok(())
}

pub fn package(
    target: Target,
    channels: &[Channel],
    version: Version,
    out: &Path,
    sign: bool,
    binaries: Option<&Path>,
) -> Result<Vec<PathBuf>> {
    let binaries = match binaries {
        Some(directory) => directory.to_path_buf(),
        None => build(target)?,
    };
    let pdfium = pdfium::fetch(target)?;
    let helper = phone::build(target)?;
    let notices = notices::generate(target, &read_text(&pdfium.notices)?, &helper.notices)?;
    let inputs = Inputs {
        app: binaries.join(target.exe(APP)),
        worker: binaries.join(target.exe(WORKER)),
        phone: helper.executable,
        pdfium: pdfium.library,
        notices,
    };
    create_dir(out)?;
    let work = target_dir().join("package").join(target.triple());
    fresh_dir(&work)?;
    match target.os {
        Os::Macos => channels
            .iter()
            .map(|&channel| macos(target, channel, version, &inputs, &work, out, sign))
            .collect(),
        Os::Windows => Ok(vec![windows(target, version, &inputs, out)?]),
        Os::Linux => Ok(vec![linux(target, version, &inputs, out)?]),
    }
}

struct Inputs {
    app: PathBuf,
    worker: PathBuf,
    phone: PathBuf,
    pdfium: PathBuf,
    notices: String,
}

fn build(target: Target) -> Result<PathBuf> {
    exec(command("cargo")
        .args(["build", "--release", "--locked", "--no-default-features", "--package", "butter-paper"])
        .args(["--target", target.triple(), "--bin", APP, "--bin", WORKER])
        .current_dir(root()))?;
    Ok(target_dir().join(target.triple()).join("release"))
}

fn source_revision() -> Result<String> {
    output(command("git").args(["rev-parse", "HEAD"]).current_dir(root()))
}

/// MANIFEST.json for the Windows and Linux install scripts: the package's
/// identity and every other file's size and SHA-256.
fn manifest(target: Target, version: Version, files: &BTreeMap<String, (Vec<u8>, u32)>) -> Result<Vec<u8>> {
    let records: serde_json::Map<String, serde_json::Value> = files
        .iter()
        .map(|(name, (bytes, _))| (name.clone(), json!({ "bytes": bytes.len(), "sha256": sha256(bytes) })))
        .collect();
    let manifest = json!({
        "schemaVersion": 1,
        "product": "Butter Paper",
        "target": target.triple(),
        "version": version.to_string(),
        "sourceRevision": source_revision()?,
        "files": records,
    });
    Ok(format!("{}\n", serde_json::to_string_pretty(&manifest).unwrap()).into_bytes())
}

fn entries(files: BTreeMap<String, (Vec<u8>, u32)>) -> Vec<Entry> {
    files.into_iter().map(|(path, (bytes, mode))| Entry { path, bytes, mode }).collect()
}

fn windows(target: Target, version: Version, inputs: &Inputs, out: &Path) -> Result<PathBuf> {
    let mut files = BTreeMap::new();
    for (name, path, dll) in [
        (target.exe(APP), &inputs.app, false),
        (target.exe(WORKER), &inputs.worker, false),
        (target.exe(phone::EXECUTABLE), &inputs.phone, false),
        ("pdfium.dll".into(), &inputs.pdfium, true),
    ] {
        let bytes = read(path)?;
        check_pe(&bytes, target, dll).map_err(|error| format!("{name}: {error}"))?;
        files.insert(name, (bytes, 0o755));
    }
    files.insert("butter-paper.ico".into(), (read(&root().join("assets/app/icon.ico"))?, 0o644));
    files.insert("THIRD_PARTY_NOTICES.md".into(), (inputs.notices.clone().into_bytes(), 0o644));
    files.insert(
        "README.md".into(),
        (
            "# Butter Paper\n\nRun install.ps1 (right-click, Run with PowerShell) to install for this user; \
             uninstall.ps1 removes it. Butter Paper then keeps itself up to date.\n\n\
             This build is not code-signed, so Windows may show a SmartScreen warning.\n\n\
             Runtime dependencies: Microsoft Visual C++ runtime.\n"
                .as_bytes()
                .to_vec(),
            0o644,
        ),
    );
    let arch = target.package_architecture();
    let render = |template: &str| template.replace("@VERSION@", &version.to_string()).replace("@ARCH@", arch);
    let uninstall = render(include_str!("../templates/windows/uninstall.ps1"));
    files.insert("uninstall.ps1".into(), (uninstall.into_bytes(), 0o644));
    // install.ps1 lists the package (itself included) and the manifest's files.
    let mut names: Vec<String> = files.keys().cloned().chain(["install.ps1".to_string()]).collect();
    names.sort();
    let quoted = |names: &[String]| names.iter().map(|name| format!("'{name}'")).collect::<Vec<_>>().join(", ");
    let manifest_names = quoted(&names);
    names.push("MANIFEST.json".into());
    let install = render(include_str!("../templates/windows/install.ps1"))
        .replace("@PACKAGE_FILES@", &quoted(&names))
        .replace("@MANIFEST_FILES@", &manifest_names);
    files.insert("install.ps1".into(), (install.into_bytes(), 0o644));
    let manifest = manifest(target, version, &files)?;
    files.insert("MANIFEST.json".into(), (manifest, 0o644));
    let path = out.join(target.asset_name(Channel::Stable));
    write(&path, archive::zip(&mut entries(files))?)?;
    Ok(path)
}

fn linux(target: Target, version: Version, inputs: &Inputs, out: &Path) -> Result<PathBuf> {
    let mut files = BTreeMap::new();
    for (name, path) in [
        (APP, &inputs.app),
        (WORKER, &inputs.worker),
        (phone::EXECUTABLE, &inputs.phone),
        ("libpdfium.so", &inputs.pdfium),
    ] {
        let bytes = read(path)?;
        check_elf(&bytes, target, name == "libpdfium.so").map_err(|error| format!("{name}: {error}"))?;
        files.insert(name.to_string(), (bytes, 0o755));
    }
    files.insert("butter-paper.png".into(), (read(&root().join("assets/app/linux/1024x1024.png"))?, 0o644));
    files.insert(
        "butter-paper.desktop".into(),
        (include_str!("../templates/linux/butter-paper.desktop").as_bytes().to_vec(), 0o644),
    );
    files.insert("THIRD_PARTY_NOTICES.md".into(), (inputs.notices.clone().into_bytes(), 0o644));
    files.insert(
        "README.md".into(),
        (
            "# Butter Paper\n\nRun ./install-user.sh to install for this user (no root needed); \
             ./uninstall-user.sh removes it. Butter Paper then keeps itself up to date.\n\n\
             Runtime dependencies: glibc, libfontconfig, X11 and Wayland system libraries.\n"
                .as_bytes()
                .to_vec(),
            0o644,
        ),
    );
    let mut payload: Vec<String> = files.keys().cloned().collect();
    payload.extend(["install-user.sh", "uninstall-user.sh", "MANIFEST.json"].map(String::from));
    let render = |template: &str| {
        template
            .replace("@VERSION@", &version.to_string())
            .replace("@PAYLOAD_WORDS@", &payload.iter().map(|name| format!("'{name}'")).collect::<Vec<_>>().join(" "))
            .replace("@PAYLOAD_CASES@", &payload.iter().map(|name| format!("'{name}'")).collect::<Vec<_>>().join("|"))
            .replace("@PAYLOAD_COUNT@", &payload.len().to_string())
    };
    files.insert("install-user.sh".into(), (render(include_str!("../templates/linux/install-user.sh")).into_bytes(), 0o755));
    files.insert("uninstall-user.sh".into(), (render(include_str!("../templates/linux/uninstall-user.sh")).into_bytes(), 0o755));
    let manifest = manifest(target, version, &files)?;
    files.insert("MANIFEST.json".into(), (manifest, 0o644));
    let root_name = format!("butter-paper-linux-{}-{version}", target.package_architecture());
    let path = out.join(target.asset_name(Channel::Stable));
    write(&path, archive::tar_xz(&root_name, &mut entries(files))?)?;
    Ok(path)
}

fn macos(
    target: Target,
    channel: Channel,
    version: Version,
    inputs: &Inputs,
    work: &Path,
    out: &Path,
    sign: bool,
) -> Result<PathBuf> {
    let product = channel.product_name();
    let staging = work.join(match channel {
        Channel::Stable => "stable",
        Channel::Beta => "beta",
    });
    fresh_dir(&staging)?;
    let app = staging.join(format!("{product}.app"));
    let contents = app.join("Contents");
    let swift_arch = match target.arch {
        Arch::Arm64 => "arm64",
        Arch::X64 => "x86_64",
    };
    let camera = staging.join(CAMERA);
    exec(command("xcrun")
        .args(["swiftc", "-swift-version", "5", "-O", "-target"])
        .arg(format!("{swift_arch}-apple-macos{MINIMUM_MACOS}"))
        .arg(app_crate().join("macos/SignatureCamera.swift"))
        .arg("-o")
        .arg(&camera))?;

    let executables = [
        (contents.join("MacOS").join(product), &inputs.app),
        (contents.join("MacOS").join(WORKER), &inputs.worker),
        (contents.join("MacOS").join(CAMERA), &camera),
        (contents.join("MacOS").join(phone::EXECUTABLE), &inputs.phone),
    ];
    for (destination, source) in &executables {
        let bytes = read(source)?;
        check_macho(&bytes, target, MACHO_EXECUTE).map_err(|error| format!("{}: {error}", source.display()))?;
        write(destination, bytes)?;
        set_mode(destination, 0o755)?;
    }
    let library = contents.join("Frameworks/libpdfium.dylib");
    let bytes = read(&inputs.pdfium)?;
    check_macho(&bytes, target, MACHO_DYLIB).map_err(|error| format!("libpdfium.dylib: {error}"))?;
    write(&library, bytes)?;
    set_mode(&library, 0o755)?;

    let icons = match channel {
        Channel::Stable => root().join("assets/app"),
        Channel::Beta => root().join("assets/app/beta"),
    };
    let resources = contents.join("Resources");
    copy(&icons.join("icon.icns"), &resources.join("icon.icns"))?;
    compile_icon(&icons.join(format!("macos/{product}.icon")), &staging.join("icon"), &resources)?;
    write(&resources.join("THIRD_PARTY_NOTICES.md"), &inputs.notices)?;
    write(&contents.join("Info.plist"), info_plist(channel, version))?;

    if sign {
        sign_and_notarise(channel, &app, &contents, &staging)?;
    }
    let path = out.join(target.asset_name(channel));
    if path.exists() {
        std::fs::remove_file(&path).map_err(|error| error.to_string())?;
    }
    exec(command("/usr/bin/ditto").args(["-c", "-k", "--keepParent"]).arg(&app).arg(&path))?;
    Ok(path)
}

/// macOS 26 draws the system icon background and full-size artwork only from
/// a compiled Icon Composer catalog; a bare icon.icns is shrunk onto a tile.
fn compile_icon(source: &Path, work: &Path, resources: &Path) -> Result {
    fresh_dir(work)?;
    let icon = work.join("Icon.icon");
    exec(command("cp").arg("-R").arg(source).arg(&icon))?;
    let partial = work.join("partial-info.plist");
    exec(command("xcrun")
        .arg("actool")
        .arg(&icon)
        .arg("--compile")
        .arg(work)
        .args(["--output-format", "human-readable-text", "--output-partial-info-plist"])
        .arg(&partial)
        .args(["--app-icon", "Icon", "--include-all-app-icons", "--target-device", "mac"])
        .args(["--minimum-deployment-target", "26.0", "--platform", "macosx"]))?;
    let generated = read_text(&partial)?;
    if !generated.contains("<key>CFBundleIconName</key>") || !generated.contains("<string>Icon</string>") {
        return Err("actool did not compile the application icon".into());
    }
    copy(&work.join("Assets.car"), &resources.join("Assets.car"))
}

pub fn info_plist(channel: Channel, version: Version) -> String {
    let product = channel.product_name();
    let values = [
        ("CFBundleDisplayName", product.to_string()),
        ("CFBundleExecutable", product.to_string()),
        ("CFBundleIconFile", "icon.icns".to_string()),
        ("CFBundleIconName", "Icon".to_string()),
        ("CFBundleIdentifier", channel.bundle_identifier().to_string()),
        ("CFBundleName", product.to_string()),
        // macOS wants X.Y.Z here; a beta is told apart by its build number.
        ("CFBundleShortVersionString", version.core()),
        ("CFBundleVersion", version.build_number().to_string()),
        ("LSMinimumSystemVersion", MINIMUM_MACOS.to_string()),
    ];
    let entries: String = values
        .iter()
        .map(|(key, value)| format!("  <key>{key}</key>\n  <string>{value}</string>\n"))
        .collect();
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleDevelopmentRegion</key>
  <string>en</string>
{entries}  <key>CFBundleDocumentTypes</key>
  <array>
    <dict>
      <key>CFBundleTypeName</key>
      <string>PDF Document</string>
      <key>CFBundleTypeRole</key>
      <string>Editor</string>
      <key>LSHandlerRank</key>
      <string>Alternate</string>
      <key>LSItemContentTypes</key>
      <array>
        <string>com.adobe.pdf</string>
      </array>
    </dict>
  </array>
  <key>CFBundleInfoDictionaryVersion</key>
  <string>6.0</string>
  <key>CFBundlePackageType</key>
  <string>APPL</string>
  <key>NSHighResolutionCapable</key>
  <true/>
  <key>NSCameraUsageDescription</key>
  <string>Butter Paper uses the camera only when you choose to take a signature photo.</string>
  <key>NSLocalNetworkUsageDescription</key>
  <string>Butter Paper uses your local network only when you choose to transfer a signature from your phone.</string>
</dict>
</plist>
"#
    )
}

/// Signs every code object (embedded code first, the bundle last) with the
/// hardened runtime, notarises, staples, then checks the result as the
/// updater and Gatekeeper will. Needs BP_SIGNING_IDENTITY and
/// BP_NOTARY_PROFILE (a notarytool keychain profile).
fn sign_and_notarise(channel: Channel, app: &Path, contents: &Path, work: &Path) -> Result {
    let identity = std::env::var("BP_SIGNING_IDENTITY").map_err(|_| "BP_SIGNING_IDENTITY is not set")?;
    if identity != SIGNING_IDENTITY {
        return Err(format!("BP_SIGNING_IDENTITY must be {SIGNING_IDENTITY}"));
    }
    let profile = std::env::var("BP_NOTARY_PROFILE").map_err(|_| "BP_NOTARY_PROFILE is not set")?;
    let identifier = channel.bundle_identifier();
    let entitlements = |name: &str, camera: bool| -> Result<PathBuf> {
        let path = work.join(format!("{name}.entitlements"));
        let body = if camera { "\n\t<key>com.apple.security.device.camera</key>\n\t<true/>\n" } else { "" };
        write(
            &path,
            format!(
                "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n<plist version=\"1.0\">\n<dict>{body}</dict>\n</plist>\n"
            ),
        )?;
        Ok(path)
    };
    let codesign = |path: &Path, id: String, entitlements: &Path| {
        exec(command("codesign")
            .args(["--force", "--options", "runtime", "--timestamp", "--sign", &identity, "--identifier", &id])
            .arg("--entitlements")
            .arg(entitlements)
            .arg(path))
    };
    let plain = entitlements("plain", false)?;
    codesign(&contents.join("Frameworks/libpdfium.dylib"), format!("{identifier}.pdfium"), &plain)?;
    codesign(&contents.join("MacOS").join(WORKER), format!("{identifier}.pdf-worker"), &plain)?;
    codesign(&contents.join("MacOS").join(phone::EXECUTABLE), format!("{identifier}.signature-phone"), &plain)?;
    codesign(
        &contents.join("MacOS").join(CAMERA),
        format!("{identifier}.signature-camera"),
        &entitlements("camera", true)?,
    )?;
    codesign(app, identifier.to_string(), &plain)?;

    let submission = work.join("notarisation.zip");
    exec(command("/usr/bin/ditto").args(["-c", "-k", "--keepParent"]).arg(app).arg(&submission))?;
    notarise(&submission, &profile)?;
    exec(command("xcrun").args(["stapler", "staple"]).arg(app))?;

    let requirement = format!(
        "=identifier \"{identifier}\" and anchor apple generic and certificate leaf[subject.OU] = \"{TEAM_ID}\""
    );
    exec(command("codesign").args(["--verify", "--deep", "--strict", "-R", &requirement]).arg(app))?;
    exec(command("spctl").args(["--assess", "--type", "execute"]).arg(app))?;
    exec(command("xcrun").args(["stapler", "validate"]).arg(app))
}

/// Submits and waits. If only the status polling loses the network (seen on
/// hosted runners), waits again on the same submission instead of resubmitting.
fn notarise(archive: &Path, profile: &str) -> Result {
    let submit = command("xcrun")
        .args(["notarytool", "submit"])
        .arg(archive)
        .args(["--keychain-profile", profile, "--wait", "--output-format", "json"])
        .output()
        .map_err(|error| error.to_string())?;
    let mut text = String::from_utf8_lossy(&submit.stdout).into_owned() + &String::from_utf8_lossy(&submit.stderr);
    if !submit.status.success() {
        let id = submission_id(&text).ok_or_else(|| format!("notarisation failed: {text}"))?;
        let mut accepted = false;
        for _ in 0..6 {
            std::thread::sleep(std::time::Duration::from_secs(30));
            let wait = command("xcrun")
                .args(["notarytool", "wait", &id, "--keychain-profile", profile, "--output-format", "json"])
                .output()
                .map_err(|error| error.to_string())?;
            text = String::from_utf8_lossy(&wait.stdout).into_owned();
            if wait.status.success() {
                accepted = true;
                break;
            }
        }
        if !accepted {
            return Err(format!("notarisation did not finish: {text}"));
        }
    }
    let result: serde_json::Value = serde_json::from_str(text.lines().rev().find(|l| l.starts_with('{')).unwrap_or(""))
        .map_err(|_| format!("notarytool did not return JSON: {text}"))?;
    if result["status"] != "Accepted" {
        return Err(format!("notarisation was not accepted: {text}"));
    }
    Ok(())
}

fn submission_id(text: &str) -> Option<String> {
    let start = text.find("/notary/v2/submissions/")? + "/notary/v2/submissions/".len();
    let id: String = text[start..].chars().take_while(|c| c.is_ascii_hexdigit() || *c == '-').collect();
    (id.len() == 36).then_some(id)
}

const MACHO_EXECUTE: u32 = 2;
const MACHO_DYLIB: u32 = 6;

fn u32_le(bytes: &[u8], offset: usize) -> Result<u32> {
    bytes
        .get(offset..offset + 4)
        .map(|b| u32::from_le_bytes(b.try_into().unwrap()))
        .ok_or_else(|| "truncated binary".to_string())
}

fn u16_le(bytes: &[u8], offset: usize) -> Result<u16> {
    bytes
        .get(offset..offset + 2)
        .map(|b| u16::from_le_bytes(b.try_into().unwrap()))
        .ok_or_else(|| "truncated binary".to_string())
}

/// A thin 64-bit Mach-O for `target` of `file_type` whose minimum macOS is
/// no newer than the package declares.
pub fn check_macho(bytes: &[u8], target: Target, file_type: u32) -> Result {
    if u32_le(bytes, 0)? != 0xfeed_facf {
        return Err("not a thin 64-bit Mach-O".into());
    }
    let cpu = match target.arch {
        Arch::Arm64 => 0x0100_000c,
        Arch::X64 => 0x0100_0007,
    };
    if u32_le(bytes, 4)? != cpu {
        return Err(format!("architecture does not match {}", target.triple()));
    }
    if u32_le(bytes, 12)? != file_type {
        return Err("wrong Mach-O file type".into());
    }
    let count = u32_le(bytes, 16)?;
    let mut offset = 32usize;
    let end = offset + u32_le(bytes, 20)? as usize;
    let mut versions = Vec::new();
    for _ in 0..count {
        let command = u32_le(bytes, offset)?;
        let size = u32_le(bytes, offset + 4)? as usize;
        if size < 8 || offset + size > end {
            return Err("malformed load commands".into());
        }
        match command {
            0x32 => versions.push(u32_le(bytes, offset + 12)?), // LC_BUILD_VERSION
            0x24 => versions.push(u32_le(bytes, offset + 8)?),  // LC_VERSION_MIN_MACOSX
            _ => {}
        }
        offset += size;
    }
    let [minimum] = versions[..] else {
        return Err("must declare exactly one minimum macOS version".into());
    };
    let (major, minor) = MINIMUM_MACOS.split_once('.').unwrap();
    let declared = (major.parse::<u32>().unwrap() << 16) | (minor.parse::<u32>().unwrap() << 8);
    if minimum > declared {
        return Err(format!(
            "requires macOS {}.{} but the package declares {MINIMUM_MACOS}",
            minimum >> 16,
            (minimum >> 8) & 0xff
        ));
    }
    Ok(())
}

/// A 64-bit PE image (DLL or executable) for `target`.
pub fn check_pe(bytes: &[u8], target: Target, dll: bool) -> Result {
    if bytes.get(..2) != Some(b"MZ") {
        return Err("not a PE file".into());
    }
    let pe = u32_le(bytes, 0x3c)? as usize;
    if bytes.get(pe..pe + 4) != Some(b"PE\0\0") {
        return Err("not a PE file".into());
    }
    let machine = match target.arch {
        Arch::Arm64 => 0xaa64,
        Arch::X64 => 0x8664,
    };
    if u16_le(bytes, pe + 4)? != machine || u16_le(bytes, pe + 24)? != 0x20b {
        return Err(format!("not a 64-bit PE image for {}", target.triple()));
    }
    if (u16_le(bytes, pe + 22)? & 0x2000 != 0) != dll {
        return Err("wrong PE image type".into());
    }
    Ok(())
}

/// A 64-bit little-endian ELF executable or shared object for `target`.
pub fn check_elf(bytes: &[u8], target: Target, shared_object: bool) -> Result {
    if bytes.get(..4) != Some(b"\x7fELF") || bytes.get(4..6) != Some(&[2, 1]) {
        return Err("not a 64-bit little-endian ELF file".into());
    }
    let machine = match target.arch {
        Arch::Arm64 => 183,
        Arch::X64 => 62,
    };
    if u16_le(bytes, 18)? != machine {
        return Err(format!("ELF machine does not match {}", target.triple()));
    }
    // Rust executables are position independent (ET_DYN); the library must be.
    let kind = u16_le(bytes, 16)?;
    if kind != 3 && (shared_object || kind != 2) {
        return Err("wrong ELF type".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn info_plist_carries_the_identity_and_both_versions() {
        let plist = info_plist(Channel::Beta, Version::parse("0.1.1-beta.2").unwrap());
        assert!(plist.contains("<key>CFBundleExecutable</key>\n  <string>Butter Paper Beta</string>"));
        assert!(plist.contains("<string>com.butterpaper.desktop.beta</string>"));
        assert!(plist.contains("<key>CFBundleShortVersionString</key>\n  <string>0.1.1</string>"));
        assert!(plist.contains("<key>CFBundleVersion</key>\n  <string>100100002</string>"));
        assert!(plist.contains("<key>LSMinimumSystemVersion</key>\n  <string>13.0</string>"));
    }

    #[test]
    fn binary_checks_reject_the_wrong_architecture() {
        let mut elf = vec![0u8; 64];
        elf[..6].copy_from_slice(b"\x7fELF\x02\x01");
        elf[16] = 3;
        elf[18] = 62;
        let x64 = Target::parse("x86_64-unknown-linux-gnu").unwrap();
        let arm = Target::parse("aarch64-unknown-linux-gnu").unwrap();
        assert!(check_elf(&elf, x64, true).is_ok());
        assert!(check_elf(&elf, arm, true).is_err());
        let mut pe = vec![0u8; 0x100];
        pe[..2].copy_from_slice(b"MZ");
        pe[0x3c] = 0x80;
        pe[0x80..0x84].copy_from_slice(b"PE\0\0");
        pe[0x84..0x86].copy_from_slice(&0x8664u16.to_le_bytes());
        pe[0x96..0x98].copy_from_slice(&0x2000u16.to_le_bytes());
        pe[0x98..0x9a].copy_from_slice(&0x20bu16.to_le_bytes());
        let windows = Target::parse("x86_64-pc-windows-msvc").unwrap();
        assert!(check_pe(&pe, windows, true).is_ok());
        assert!(check_pe(&pe, windows, false).is_err());
        assert!(check_pe(&pe, Target::parse("aarch64-pc-windows-msvc").unwrap(), true).is_err());
    }

    #[test]
    fn notarisation_submission_ids_are_recovered_from_errors() {
        let text = "Error: https://appstoreconnect.apple.com/notary/v2/submissions/0123abcd-0123-4567-89ab-0123456789ab/status timed out";
        assert_eq!(submission_id(text).as_deref(), Some("0123abcd-0123-4567-89ab-0123456789ab"));
        assert_eq!(submission_id("no id"), None);
    }

    #[test]
    fn install_templates_have_only_known_placeholders() {
        for template in [
            include_str!("../templates/linux/install-user.sh"),
            include_str!("../templates/linux/uninstall-user.sh"),
            include_str!("../templates/windows/install.ps1"),
            include_str!("../templates/windows/uninstall.ps1"),
        ] {
            let mut rest = template;
            while let Some(start) = rest.find('@') {
                let tail = &rest[start + 1..];
                if let Some(end) = tail.find('@') {
                    let name = &tail[..end];
                    if !name.is_empty() && name.bytes().all(|b| b.is_ascii_uppercase() || b == b'_') {
                        assert!(
                            ["VERSION", "ARCH", "PAYLOAD_WORDS", "PAYLOAD_CASES", "PAYLOAD_COUNT", "PACKAGE_FILES", "MANIFEST_FILES"]
                                .contains(&name),
                            "{name}"
                        );
                    }
                }
                rest = tail;
            }
            assert!(!template.contains("gpui-migration"));
        }
    }
}
