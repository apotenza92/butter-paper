//! The approved PDFium build, published once as the `pdfium-7881` release and
//! pinned here by SHA-256 (xtask/pdfium.json).

use std::{collections::BTreeMap, path::PathBuf};

use serde::Deserialize;

use crate::{Args, Result, download, target::Target, target_dir};

#[derive(Deserialize)]
struct Pins {
    release: String,
    targets: BTreeMap<String, TargetPins>,
}

#[derive(Deserialize)]
struct TargetPins {
    library: Asset,
    notices: Asset,
}

#[derive(Deserialize)]
struct Asset {
    name: String,
    sha256: String,
}

pub struct Pdfium {
    pub library: PathBuf,
    pub notices: PathBuf,
}

fn pins() -> Pins {
    serde_json::from_str(include_str!("../pdfium.json")).expect("xtask/pdfium.json is valid")
}

/// Downloads (or reuses) the verified library and notices for `target`.
pub fn fetch(target: Target) -> Result<Pdfium> {
    let pins = pins();
    let entry = pins
        .targets
        .get(target.triple())
        .ok_or_else(|| format!("no approved PDFium for {}", target.triple()))?;
    let directory = target_dir().join("pdfium").join(target.triple());
    let library = directory.join(target.pdfium_library());
    let notices = directory.join("NOTICES.md");
    download(&format!("{}{}", pins.release, entry.library.name), &library, &entry.library.sha256)?;
    download(&format!("{}{}", pins.release, entry.notices.name), &notices, &entry.notices.sha256)?;
    Ok(Pdfium { library, notices })
}

pub fn run(args: &mut Args) -> Result {
    let target = match args.value("--target")? {
        Some(triple) => Target::parse(&triple)?,
        None => Target::host()?,
    };
    args.finish()?;
    let pdfium = fetch(target)?;
    println!("{}", pdfium.library.display());
    eprintln!("Run the app or tests with BP_PDFIUM_LIBRARY set to that path.");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_release_target_has_a_pinned_library_and_notices() {
        let pins = pins();
        assert!(pins.release.ends_with("/releases/download/pdfium-7881/"));
        for target in crate::target::ALL {
            let entry = &pins.targets[target.triple()];
            assert_eq!(entry.library.name, format!("{}-{}", target.triple(), target.pdfium_library()));
            for digest in [&entry.library.sha256, &entry.notices.sha256] {
                assert!(digest.len() == 64 && digest.bytes().all(|b| b.is_ascii_hexdigit()));
            }
        }
        assert_eq!(pins.targets.len(), 6);
    }
}
