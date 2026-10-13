//! Installed TrueType fonts, as a last source of glyphs for edited page text
//! (after the page's own font and the bundled families), so no character is
//! refused while some installed font can draw it.
//!
//! Only faces with TrueType outlines (`glyf`) are listed: they are embedded
//! as subsets (`font_subset`). The scan reads each face's table directory and
//! naming tables, not whole files; a face's data and character map are read
//! on first use and kept.

use std::collections::HashMap;
use std::fs::File;
use std::io::{Read as _, Seek as _, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

/// An installed font face.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SystemFace {
    pub path: PathBuf,
    /// The face's index in a collection (0 for a single font).
    pub index: u32,
    pub postscript_name: String,
    pub family: String,
    pub bold: bool,
    pub italic: bool,
    /// Where its character map is in the file.
    cmap: (u64, usize),
}

/// Installed faces, sorted by file and index so every process resolves a
/// character to the same face.
pub(crate) struct SystemFonts {
    faces: Vec<SystemFace>,
    data: Mutex<HashMap<usize, Option<Arc<Vec<u8>>>>>,
    /// Each face's character map, read on first use.
    cmaps: Mutex<HashMap<usize, Option<Arc<Vec<u8>>>>>,
    /// Answers of [`Self::covering`].
    covering: Mutex<HashMap<(char, bool, bool), Option<usize>>>,
}

/// The platform's font folders, or those `BP_SYSTEM_FONT_DIRS` lists (a
/// path list, for tests and development).
fn font_directories() -> Vec<PathBuf> {
    if let Some(directories) = std::env::var_os("BP_SYSTEM_FONT_DIRS") {
        return std::env::split_paths(&directories).collect();
    }
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let mut directories = Vec::new();
    if cfg!(target_os = "macos") {
        directories.extend(
            ["/System/Library/Fonts", "/Library/Fonts"]
                .iter()
                .map(PathBuf::from),
        );
        directories.extend(home.map(|home| home.join("Library/Fonts")));
    } else if cfg!(target_os = "windows") {
        if let Some(windows) = std::env::var_os("WINDIR") {
            directories.push(PathBuf::from(windows).join("Fonts"));
        }
        if let Some(local) = std::env::var_os("LOCALAPPDATA") {
            directories.push(PathBuf::from(local).join("Microsoft/Windows/Fonts"));
        }
    } else {
        directories.extend(
            ["/usr/share/fonts", "/usr/local/share/fonts"]
                .iter()
                .map(PathBuf::from),
        );
        if let Some(home) = home {
            directories.push(home.join(".local/share/fonts"));
            directories.push(home.join(".fonts"));
        }
    }
    directories
}

fn read_at(file: &mut File, offset: u64, length: usize) -> Option<Vec<u8>> {
    file.seek(SeekFrom::Start(offset)).ok()?;
    let mut bytes = vec![0; length];
    file.read_exact(&mut bytes).ok()?;
    Some(bytes)
}

fn u16_at(data: &[u8], offset: usize) -> Option<u16> {
    Some(u16::from_be_bytes(
        data.get(offset..offset + 2)?.try_into().ok()?,
    ))
}

fn u32_at(data: &[u8], offset: usize) -> Option<u32> {
    Some(u32::from_be_bytes(
        data.get(offset..offset + 4)?.try_into().ok()?,
    ))
}

/// A face's tables: tag, offset and length.
fn face_tables(file: &mut File, directory: u64) -> Option<Vec<([u8; 4], u64, usize)>> {
    let header = read_at(file, directory, 12)?;
    let count = usize::from(u16_at(&header, 4)?);
    let records = read_at(file, directory + 12, count * 16)?;
    (0..count)
        .map(|index| {
            let record = &records[index * 16..index * 16 + 16];
            Some((
                record[..4].try_into().ok()?,
                u64::from(u32_at(record, 8)?),
                u32_at(record, 12)? as usize,
            ))
        })
        .collect()
}

/// A name table string (Windows Unicode, then Macintosh Roman).
fn name(table: &[u8], id: u16) -> Option<String> {
    let count = usize::from(u16_at(table, 2)?);
    let strings = usize::from(u16_at(table, 4)?);
    let mut fallback = None;
    for record in (0..count).map(|index| 6 + index * 12) {
        let (platform, name_id) = (u16_at(table, record)?, u16_at(table, record + 6)?);
        if name_id != id {
            continue;
        }
        let length = usize::from(u16_at(table, record + 8)?);
        let offset = strings + usize::from(u16_at(table, record + 10)?);
        let bytes = table.get(offset..offset + length)?;
        match platform {
            3 | 0 => {
                let units = bytes
                    .chunks_exact(2)
                    .map(|pair| u16::from_be_bytes([pair[0], pair[1]]));
                return char::decode_utf16(units)
                    .collect::<Result<String, _>>()
                    .ok();
            }
            1 => fallback = Some(bytes.iter().map(|byte| char::from(*byte)).collect()),
            _ => {}
        }
    }
    fallback
}

fn scan_face(path: &Path, file: &mut File, index: u32, directory: u64) -> Option<SystemFace> {
    let tables = face_tables(file, directory)?;
    let table = |file: &mut File, tag: &[u8; 4]| {
        let (_, offset, length) = tables.iter().find(|(name, _, _)| name == tag)?;
        read_at(file, *offset, (*length).min(1 << 20))
    };
    if !tables.iter().any(|(tag, _, _)| tag == b"glyf") {
        return None;
    }
    let (_, cmap_offset, cmap_length) = *tables.iter().find(|(tag, _, _)| tag == b"cmap")?;
    let names = table(file, b"name")?;
    let os2 = table(file, b"OS/2");
    let weight = os2.as_deref().and_then(|os2| u16_at(os2, 4)).unwrap_or(400);
    let selection = os2.as_deref().and_then(|os2| u16_at(os2, 62)).unwrap_or(0);
    Some(SystemFace {
        path: path.to_path_buf(),
        index,
        postscript_name: name(&names, 6)?,
        family: name(&names, 16).or_else(|| name(&names, 1))?,
        bold: weight >= 600 || selection & 0x20 != 0,
        italic: selection & 1 != 0,
        cmap: (cmap_offset, cmap_length),
    })
}

fn scan_file(path: &Path) -> Vec<SystemFace> {
    let Ok(mut file) = File::open(path) else {
        return Vec::new();
    };
    let Some(header) = read_at(&mut file, 0, 12) else {
        return Vec::new();
    };
    if &header[..4] == b"ttcf" {
        let count = u32_at(&header, 8).unwrap_or(0).min(256);
        let Some(offsets) = read_at(&mut file, 12, count as usize * 4) else {
            return Vec::new();
        };
        (0..count)
            .filter_map(|index| {
                let directory = u64::from(u32_at(&offsets, index as usize * 4)?);
                scan_face(path, &mut file, index, directory)
            })
            .collect()
    } else {
        scan_face(path, &mut file, 0, 0).into_iter().collect()
    }
}

fn font_files(directory: &Path, files: &mut Vec<PathBuf>, depth: usize) {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if depth < 4 {
                font_files(&path, files, depth + 1);
            }
        } else if path.extension().is_some_and(|extension| {
            ["ttf", "ttc", "otf"]
                .iter()
                .any(|known| extension.eq_ignore_ascii_case(known))
        }) {
            files.push(path);
        }
    }
}

impl SystemFonts {
    /// The faces in `directories` (searched to a few levels).
    pub(crate) fn scan(directories: &[PathBuf]) -> Self {
        let mut files = Vec::new();
        for directory in directories {
            font_files(directory, &mut files, 0);
        }
        files.sort();
        files.dedup();
        let faces = files.iter().flat_map(|path| scan_file(path)).collect();
        Self {
            faces,
            data: Mutex::new(HashMap::new()),
            cmaps: Mutex::new(HashMap::new()),
            covering: Mutex::new(HashMap::new()),
        }
    }

    /// The installed fonts, scanned on first use.
    pub(crate) fn installed() -> &'static Self {
        static INSTALLED: OnceLock<SystemFonts> = OnceLock::new();
        INSTALLED.get_or_init(|| Self::scan(&font_directories()))
    }

    pub(crate) fn face(&self, index: usize) -> Option<&SystemFace> {
        self.faces.get(index)
    }

    /// The file holding face `index` (a whole collection for a `.ttc`).
    pub(crate) fn data(&self, index: usize) -> Option<Arc<Vec<u8>>> {
        let mut cache = self
            .data
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        cache
            .entry(index)
            .or_insert_with(|| {
                let face = self.faces.get(index)?;
                std::fs::read(&face.path).ok().map(Arc::new)
            })
            .clone()
    }

    /// Whether face `index` maps `character` to a glyph. Reads only the
    /// face's character map.
    pub(crate) fn has(&self, index: usize, character: char) -> bool {
        let cmap = {
            let mut cache = self
                .cmaps
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            cache
                .entry(index)
                .or_insert_with(|| {
                    let face = self.faces.get(index)?;
                    let (offset, length) = face.cmap;
                    let mut file = File::open(&face.path).ok()?;
                    read_at(&mut file, offset, length.min(16 << 20)).map(Arc::new)
                })
                .clone()
        };
        cmap.and_then(|cmap| {
            let table = ttf_parser::cmap::Table::parse(&cmap)?;
            table
                .subtables
                .into_iter()
                .filter(|subtable| subtable.is_unicode())
                .find_map(|subtable| subtable.glyph_index(u32::from(character)))
        })
        .is_some_and(|glyph| glyph.0 != 0)
    }

    /// The face named `postscript_name`, if installed.
    pub(crate) fn by_postscript_name(&self, postscript_name: &str) -> Option<usize> {
        self.faces
            .iter()
            .position(|face| face.postscript_name == postscript_name)
    }

    /// The installed families, sorted and without repeats.
    pub(crate) fn families(&self) -> Vec<String> {
        let mut families = self
            .faces
            .iter()
            .map(|face| face.family.clone())
            .collect::<Vec<_>>();
        families.sort_by_key(|family| family.to_lowercase());
        families.dedup();
        families
    }

    /// The face of `family` in the style asked for, or nearest it (the
    /// weight before the slant), if the family is installed.
    pub(crate) fn styled(&self, family: &str, bold: bool, italic: bool) -> Option<usize> {
        let mut candidates = (0..self.faces.len())
            .filter(|index| self.faces[*index].family == family)
            .collect::<Vec<_>>();
        candidates.sort_by_key(|index| {
            let face = &self.faces[*index];
            (
                face.bold != bold,
                face.italic != italic,
                face.postscript_name.len(),
            )
        });
        candidates.first().copied()
    }

    /// An installed face that draws `character`: one of `bold` and `italic`
    /// first, then any.
    pub(crate) fn covering(&self, character: char, bold: bool, italic: bool) -> Option<usize> {
        let key = (character, bold, italic);
        if let Some(known) = self
            .covering
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .get(&key)
        {
            return *known;
        }
        let styled = |face: &SystemFace| face.bold == bold && face.italic == italic;
        let found = (0..self.faces.len())
            .filter(|index| styled(&self.faces[*index]))
            .chain((0..self.faces.len()).filter(|index| !styled(&self.faces[*index])))
            .find(|index| self.has(*index, character));
        self.covering
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .insert(key, found);
        found
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bundled() -> SystemFonts {
        SystemFonts::scan(&[Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/fonts")])
    }

    #[test]
    fn scans_truetype_faces_with_their_names_and_styles() {
        let fonts = bundled();
        let tinos = fonts.by_postscript_name("Tinos-BoldItalic").unwrap();
        let face = fonts.face(tinos).unwrap();
        assert_eq!(face.family, "Tinos");
        assert!(face.bold && face.italic);
        let arimo = fonts
            .face(fonts.by_postscript_name("Arimo-Regular").unwrap())
            .unwrap();
        assert!(!arimo.bold && !arimo.italic);
        assert!(fonts.has(tinos, 'Ж'));
        assert!(!fonts.has(tinos, '中'));
    }

    #[test]
    fn covering_faces_prefer_the_style_asked_for() {
        let fonts = bundled();
        let index = fonts.covering('Ж', true, false).unwrap();
        let face = fonts.face(index).unwrap();
        assert!(face.bold && !face.italic, "{face:?}");
        let chinese = fonts.covering('中', false, false).unwrap();
        assert!(
            fonts
                .face(chinese)
                .unwrap()
                .family
                .starts_with("Noto Sans SC")
        );
        assert_eq!(fonts.covering('\u{e000}', false, false), None);
    }
}

#[cfg(test)]
mod installed_tests {
    #[test]
    #[ignore = "reads the machine's installed fonts; run to time the scan"]
    fn bench_installed_font_scan() {
        let started = std::time::Instant::now();
        let fonts = super::SystemFonts::installed();
        let scanned = started.elapsed();
        let started = std::time::Instant::now();
        let kana = fonts.covering('か', false, false);
        let missing_started = std::time::Instant::now();
        assert_eq!(fonts.covering('\u{10fffd}', false, false), None);
        eprintln!("a character no face draws: {:?}", missing_started.elapsed());
        eprintln!(
            "{} faces in {scanned:?}; か in {:?} after {:?}",
            fonts.faces.len(),
            kana.and_then(|index| fonts.face(index))
                .map(|face| &face.postscript_name),
            started.elapsed()
        );
    }
}
