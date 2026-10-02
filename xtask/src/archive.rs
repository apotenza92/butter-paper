//! Deterministic archives: byte-identical for the same inputs (sorted
//! entries, zero timestamps and owners).

use std::{io::Read, path::Path};

use flate2::{Compression, read::GzDecoder, write::GzEncoder};

use crate::Result;

/// One archive member: path inside the archive, bytes and Unix mode.
pub struct Entry {
    pub path: String,
    pub bytes: Vec<u8>,
    pub mode: u32,
}

/// Extracts a .tar.gz, refusing links, devices and paths outside `destination`.
pub fn extract_tar_gz(bytes: &[u8], destination: &Path) -> Result {
    let mut archive = tar::Archive::new(GzDecoder::new(bytes));
    for entry in archive.entries().map_err(|error| error.to_string())? {
        let mut entry = entry.map_err(|error| error.to_string())?;
        let kind = entry.header().entry_type();
        if !(kind.is_file() || kind.is_dir() || kind.is_pax_global_extensions() || kind.is_pax_local_extensions()) {
            return Err(format!("archive member has unsupported type: {:?}", entry.path()));
        }
        // `unpack_in` refuses absolute paths and `..` components.
        if !entry.unpack_in(destination).map_err(|error| error.to_string())? {
            return Err(format!("archive member escapes the destination: {:?}", entry.path()));
        }
    }
    Ok(())
}

fn tar_bytes(root: Option<&str>, entries: &mut [Entry]) -> Result<Vec<u8>> {
    entries.sort_by(|a, b| a.path.cmp(&b.path));
    let mut builder = tar::Builder::new(Vec::new());
    builder.mode(tar::HeaderMode::Deterministic);
    let header = |size: u64, mode: u32, kind: tar::EntryType| {
        let mut header = tar::Header::new_ustar();
        header.set_size(size);
        header.set_mode(mode);
        header.set_mtime(0);
        header.set_uid(0);
        header.set_gid(0);
        header.set_entry_type(kind);
        header
    };
    if let Some(root) = root {
        let mut directory = header(0, 0o755, tar::EntryType::Directory);
        builder
            .append_data(&mut directory, format!("{root}/"), std::io::empty())
            .map_err(|error| error.to_string())?;
    }
    for entry in entries.iter() {
        let path = match root {
            Some(root) => format!("{root}/{}", entry.path),
            None => entry.path.clone(),
        };
        let mut file = header(entry.bytes.len() as u64, entry.mode, tar::EntryType::Regular);
        builder.append_data(&mut file, path, entry.bytes.as_slice()).map_err(|error| error.to_string())?;
    }
    builder.into_inner().map_err(|error| error.to_string())
}

/// A tar.gz of `entries` (the Homebrew publication bundle).
pub fn tar_gz(entries: &mut [Entry]) -> Result<Vec<u8>> {
    let tar = tar_bytes(None, entries)?;
    let mut encoder = GzEncoder::new(Vec::new(), Compression::best());
    std::io::Write::write_all(&mut encoder, &tar).map_err(|error| error.to_string())?;
    encoder.finish().map_err(|error| error.to_string())
}

/// A tar.xz with every entry under `root/` (the Linux package), compressed
/// by the system `xz` single-threaded so the output is reproducible.
pub fn tar_xz(root: &str, entries: &mut [Entry]) -> Result<Vec<u8>> {
    let tar = tar_bytes(Some(root), entries)?;
    let mut child = std::process::Command::new("xz")
        .args(["-9e", "--threads=1", "--check=crc64", "--stdout"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .map_err(|error| format!("xz: {error}"))?;
    let mut stdin = child.stdin.take().unwrap();
    let writer = std::thread::spawn(move || std::io::Write::write_all(&mut stdin, &tar));
    let mut compressed = Vec::new();
    child.stdout.take().unwrap().read_to_end(&mut compressed).map_err(|error| error.to_string())?;
    writer.join().unwrap().map_err(|error| format!("xz: {error}"))?;
    if !child.wait().map_err(|error| error.to_string())?.success() {
        return Err("xz failed".into());
    }
    Ok(compressed)
}

/// A stored (uncompressed) zip with UTF-8 names, as PowerShell's
/// Expand-Archive reads it (the Windows package).
pub fn zip(entries: &mut [Entry]) -> Result<Vec<u8>> {
    entries.sort_by(|a, b| a.path.cmp(&b.path));
    let mut local = Vec::new();
    let mut central = Vec::new();
    for entry in entries.iter() {
        let name = entry.path.as_bytes();
        let size = u32::try_from(entry.bytes.len()).map_err(|_| format!("{} is too large for zip", entry.path))?;
        let crc = {
            let mut crc = flate2::Crc::new();
            crc.update(&entry.bytes);
            crc.sum()
        };
        let offset = u32::try_from(local.len()).map_err(|_| "zip is too large")?;
        // Local file header: version 2.0, UTF-8 names, stored, 1980-01-01.
        local.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
        for value in [20u16, 0x0800, 0, 0, 0x21] {
            local.extend_from_slice(&value.to_le_bytes());
        }
        for value in [crc, size, size] {
            local.extend_from_slice(&value.to_le_bytes());
        }
        local.extend_from_slice(&(name.len() as u16).to_le_bytes());
        local.extend_from_slice(&0u16.to_le_bytes());
        local.extend_from_slice(name);
        local.extend_from_slice(&entry.bytes);

        central.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
        for value in [0x0314u16, 20, 0x0800, 0, 0, 0x21] {
            central.extend_from_slice(&value.to_le_bytes());
        }
        for value in [crc, size, size] {
            central.extend_from_slice(&value.to_le_bytes());
        }
        for value in [name.len() as u16, 0, 0, 0, 0] {
            central.extend_from_slice(&value.to_le_bytes());
        }
        central.extend_from_slice(&((entry.mode & 0o777 | 0o100000) << 16).to_le_bytes());
        central.extend_from_slice(&offset.to_le_bytes());
        central.extend_from_slice(name);
    }
    let count = u16::try_from(entries.len()).map_err(|_| "too many zip entries")?;
    let mut zip = local;
    let central_offset = zip.len() as u32;
    zip.extend_from_slice(&central);
    zip.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
    for value in [0u16, 0, count, count] {
        zip.extend_from_slice(&value.to_le_bytes());
    }
    zip.extend_from_slice(&(central.len() as u32).to_le_bytes());
    zip.extend_from_slice(&central_offset.to_le_bytes());
    zip.extend_from_slice(&0u16.to_le_bytes());
    Ok(zip)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entries() -> Vec<Entry> {
        vec![
            Entry { path: "b.txt".into(), bytes: b"bee\n".to_vec(), mode: 0o644 },
            Entry { path: "a.exe".into(), bytes: b"MZ".to_vec(), mode: 0o755 },
        ]
    }

    #[test]
    fn archives_are_reproducible_and_sorted() {
        assert_eq!(zip(&mut entries()).unwrap(), zip(&mut entries()).unwrap());
        assert_eq!(tar_gz(&mut entries()).unwrap(), tar_gz(&mut entries()).unwrap());
        let zip = zip(&mut entries()).unwrap();
        let a = zip.windows(5).position(|w| w == b"a.exe").unwrap();
        let b = zip.windows(5).position(|w| w == b"b.txt").unwrap();
        assert!(a < b);
        assert_eq!(&zip[zip.len() - 22..zip.len() - 18], &0x0605_4b50u32.to_le_bytes());
    }

    #[test]
    fn tar_gz_round_trips_and_extraction_stays_inside_the_destination() {
        let bytes = tar_gz(&mut entries()).unwrap();
        let directory = std::env::temp_dir().join(format!("bp-xtask-archive-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).unwrap();
        extract_tar_gz(&bytes, &directory).unwrap();
        assert_eq!(std::fs::read(directory.join("b.txt")).unwrap(), b"bee\n");
        std::fs::remove_dir_all(&directory).unwrap();
    }
}
