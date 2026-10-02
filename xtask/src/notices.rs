//! THIRD_PARTY_NOTICES.md for a package: the app's own notes, every Rust
//! crate in the target's locked dependency graph with its licence texts,
//! the fonts, PDFium's notices and the phone helper's.

use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    fs,
    path::{Path, PathBuf},
};

use serde::Deserialize;

use crate::{
    Args, Result, app_crate, command, output, pdfium, phone, read_text, root, sha256,
    target::Target, write,
};

pub fn run(args: &mut Args) -> Result {
    let target = Target::parse(&args.required("--target")?)?;
    let out = PathBuf::from(args.required("--out")?);
    args.finish()?;
    let pdfium = pdfium::fetch(target)?;
    let helper = phone::build(target)?;
    write(&out, generate(target, &read_text(&pdfium.notices)?, &helper.notices)?)
}

pub fn generate(target: Target, pdfium_notices: &str, phone_notices: &str) -> Result<String> {
    let mut text = read_text(&app_crate().join("NOTICE.md"))?;
    text.push_str("\n# Rust crates\n\n");
    text.push_str(&rust_crates(target)?);
    text.push_str("\n# Fonts\n\n");
    let mut fonts: Vec<_> = fs::read_dir(app_crate().join("assets/fonts"))
        .map_err(|error| error.to_string())?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.extension().is_some_and(|extension| extension == "txt"))
        .collect();
    fonts.sort();
    for path in fonts {
        let name = path.file_name().unwrap().to_string_lossy();
        text.push_str(&format!("## {name}\n\n{}\n\n", read_text(&path)?.trim()));
    }
    text.push_str("\n# PDFium\n\n");
    text.push_str(pdfium_notices.trim_end());
    text.push_str("\n\n");
    text.push_str(phone_notices.trim_end());
    text.push('\n');
    Ok(text)
}

#[derive(Deserialize)]
struct Metadata {
    packages: Vec<Package>,
    workspace_members: Vec<String>,
    resolve: Resolve,
}

#[derive(Deserialize)]
struct Package {
    id: String,
    name: String,
    version: String,
    license: Option<String>,
    license_file: Option<String>,
    #[serde(default)]
    authors: Vec<String>,
    manifest_path: PathBuf,
    source: Option<String>,
}

#[derive(Deserialize)]
struct Resolve {
    nodes: Vec<Node>,
}

#[derive(Deserialize)]
struct Node {
    id: String,
    deps: Vec<Dependency>,
}

#[derive(Deserialize)]
struct Dependency {
    pkg: String,
    dep_kinds: Vec<DependencyKind>,
}

#[derive(Deserialize)]
struct DependencyKind {
    kind: Option<String>,
}

/// Packages linked into the release binaries: normal dependencies of the app
/// for `target`, without build scripts, proc macros' hosts or dev tools.
fn rust_crates(target: Target) -> Result<String> {
    let json = output(
        command("cargo")
            .args(["metadata", "--format-version", "1", "--locked", "--filter-platform", target.triple()])
            .current_dir(root()),
    )?;
    let metadata: Metadata = serde_json::from_str(&json).map_err(|error| error.to_string())?;
    let packages: HashMap<_, _> = metadata.packages.iter().map(|package| (package.id.as_str(), package)).collect();
    let nodes: HashMap<_, _> = metadata.resolve.nodes.iter().map(|node| (node.id.as_str(), node)).collect();
    let app = metadata
        .packages
        .iter()
        .find(|package| package.name == "butter-paper" && metadata.workspace_members.contains(&package.id))
        .ok_or("cargo metadata has no butter-paper package")?;
    let mut reached = BTreeSet::new();
    let mut pending = vec![app.id.as_str()];
    while let Some(id) = pending.pop() {
        for dependency in &nodes[id].deps {
            let normal = dependency.dep_kinds.iter().any(|kind| kind.kind.is_none());
            if normal && reached.insert(dependency.pkg.as_str()) {
                pending.push(dependency.pkg.as_str());
            }
        }
    }
    let mut ordered: Vec<&Package> = reached
        .into_iter()
        .map(|id| packages[id])
        .filter(|package| !metadata.workspace_members.contains(&package.id))
        .collect();
    ordered.sort_by(|a, b| (&a.name, &a.version).cmp(&(&b.name, &b.version)));

    let mut text = String::new();
    let mut seen: HashMap<String, String> = HashMap::new();
    let mut missing = Vec::new();
    for package in ordered {
        let label = format!("{} {}", package.name, package.version);
        let licence = package.license.as_deref().unwrap_or("see licence text");
        text.push_str(&format!("## {label} ({licence})\n\n"));
        if let Some(source) = &package.source {
            text.push_str(&format!("Source: {}\n\n", source.split('#').next().unwrap_or(source)));
        }
        let mut files = licence_files(package)?;
        if files.is_empty() {
            // The crate ships no licence file: use the standard text of the
            // licence it declares (MIT preferred where there is a choice).
            missing.push(label.clone());
            let declared = package.license.as_deref().ok_or_else(|| format!("{label} declares no licence"))?;
            let (name, body) = standard_licence(declared, &package.authors)
                .ok_or_else(|| format!("{label}: no standard text for {declared}; add one to xtask/licences"))?;
            files.insert(name, body);
        }
        for (name, body) in files {
            let digest = sha256(body.trim().as_bytes());
            match seen.get(&digest) {
                Some(first) => text.push_str(&format!("{name}: identical to the text under {first}.\n\n")),
                None => {
                    text.push_str(&format!("### {name}\n\n{}\n\n", body.trim()));
                    seen.insert(digest, label.clone());
                }
            }
        }
    }
    if !missing.is_empty() {
        eprintln!("note: standard licence texts used for {} crate(s) without licence files", missing.len());
    }
    Ok(text)
}

/// A crate's licence files: its declared `license-file`, else licence-named
/// files beside its manifest, else (for crates inside a git checkout, such as
/// the GPUI fork's) the nearest licence files up to the checkout root.
fn licence_files(package: &Package) -> Result<BTreeMap<String, String>> {
    let directory = package.manifest_path.parent().unwrap();
    let mut files = BTreeMap::new();
    if let Some(file) = &package.license_file {
        let path = directory.join(file);
        files.insert(path.file_name().unwrap().to_string_lossy().into_owned(), read_text(&path)?);
        return Ok(files);
    }
    let git_checkout = package.source.as_deref().is_some_and(|source| source.starts_with("git+"));
    let mut candidate: &Path = directory;
    loop {
        for entry in fs::read_dir(candidate).map_err(|error| format!("{}: {error}", candidate.display()))? {
            let entry = entry.map_err(|error| error.to_string())?;
            let name = entry.file_name().to_string_lossy().into_owned();
            if phone::is_licence_name(&name) && entry.path().is_file() {
                if let Ok(body) = fs::read_to_string(entry.path()) {
                    files.insert(name, body);
                }
            }
        }
        // A checkout root is .../git/checkouts/<repository>/<revision>.
        let checkout_root = candidate.parent().and_then(Path::parent).is_some_and(|path| path.ends_with("checkouts"));
        match candidate.parent() {
            Some(parent) if files.is_empty() && git_checkout && !checkout_root => candidate = parent,
            _ => break,
        }
    }
    Ok(files)
}

/// The standard text for a declared SPDX licence, choosing among
/// alternatives in order of preference; `None` if none is known.
pub fn standard_licence(declared: &str, authors: &[String]) -> Option<(String, String)> {
    let normalised = declared.replace('/', " OR ").replace(['(', ')'], " ");
    if normalised.contains(" AND ") || normalised.contains(" WITH ") {
        return None;
    }
    let choices: Vec<&str> = normalised.split(" OR ").map(str::trim).collect();
    let copyright = if authors.is_empty() {
        "Copyright (c) the authors".to_string()
    } else {
        format!("Copyright (c) {}", authors.join(", "))
    };
    for (id, text) in [
        ("MIT", include_str!("../licences/MIT.txt")),
        ("Apache-2.0", include_str!("../licences/Apache-2.0.txt")),
        ("BSD-3-Clause", include_str!("../licences/BSD-3-Clause.txt")),
        ("CC0-1.0", include_str!("../licences/CC0-1.0.txt")),
    ] {
        if choices.contains(&id) {
            return Some((format!("{id} (standard text)"), text.replace("{copyright}", &copyright)));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standard_licences_prefer_mit_and_refuse_unknown_expressions() {
        let authors = vec!["A <a@example.com>".to_string()];
        for declared in ["MIT OR Apache-2.0", "MIT/Apache-2.0", "Zlib OR Apache-2.0 OR MIT", "MIT"] {
            let (name, text) = standard_licence(declared, &authors).unwrap();
            assert_eq!(name, "MIT (standard text)");
            assert!(text.contains("Copyright (c) A <a@example.com>"));
        }
        assert!(standard_licence("Apache-2.0", &[]).unwrap().1.contains("Apache License"));
        assert_eq!(standard_licence("CC0-1.0", &[]).unwrap().0, "CC0-1.0 (standard text)");
        assert!(standard_licence("GPL-3.0-or-later", &[]).is_none());
        assert!(standard_licence("MIT AND BSD-3-Clause", &[]).is_none());
    }
}
