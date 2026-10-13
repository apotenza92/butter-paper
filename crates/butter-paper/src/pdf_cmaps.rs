//! Adobe's predefined Unicode CMaps (`UniGB-UCS2-H`, `UniJIS-UTF16-H`, ...):
//! a code in such a CMap is the character's UTF-16, and the tables here give
//! its CID, which a CIDFont's widths are keyed by. Packed from Adobe's
//! cmap-resources by `assets/cmaps/pack.py`.

use std::sync::OnceLock;

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) enum UnicodeCMap {
    Gb,
    Cns,
    Jis,
    Ks,
}

/// One range of code points with consecutive CIDs.
#[derive(Clone, Copy)]
struct Range {
    start: u32,
    count: u32,
    cid: u32,
}

impl UnicodeCMap {
    /// The CMap a font's `/Encoding` names, and whether its codes are
    /// UTF-16 (two or four bytes) rather than UCS-2. Vertical CMaps are not
    /// supported.
    pub(crate) fn from_encoding(name: &[u8]) -> Option<(Self, bool)> {
        let name = std::str::from_utf8(name).ok()?;
        let (collection, form) = name
            .strip_prefix("Uni")?
            .strip_suffix("-H")?
            .split_once('-')?;
        let cmap = match collection {
            "GB" => Self::Gb,
            "CNS" => Self::Cns,
            "JIS" => Self::Jis,
            "KS" => Self::Ks,
            _ => return None,
        };
        match form {
            "UCS2" => Some((cmap, false)),
            "UTF16" => Some((cmap, true)),
            _ => None,
        }
    }

    fn packed(self) -> &'static [u8] {
        match self {
            Self::Gb => include_bytes!("../assets/cmaps/UniGB-UTF16-H.bin"),
            Self::Cns => include_bytes!("../assets/cmaps/UniCNS-UTF16-H.bin"),
            Self::Jis => include_bytes!("../assets/cmaps/UniJIS-UTF16-H.bin"),
            Self::Ks => include_bytes!("../assets/cmaps/UniKS-UTF16-H.bin"),
        }
    }

    fn ranges(self) -> &'static [Range] {
        static TABLES: [OnceLock<Vec<Range>>; 4] = [
            OnceLock::new(),
            OnceLock::new(),
            OnceLock::new(),
            OnceLock::new(),
        ];
        TABLES[self as usize].get_or_init(|| unpack(self.packed()).unwrap_or_default())
    }

    /// The CID for a character, if the CMap maps it.
    pub(crate) fn cid(self, character: char) -> Option<u32> {
        let code_point = u32::from(character);
        let ranges = self.ranges();
        let index = ranges.partition_point(|range| range.start <= code_point);
        let range = ranges.get(index.checked_sub(1)?)?;
        (code_point < range.start + range.count).then(|| range.cid + (code_point - range.start))
    }
}

fn unpack(packed: &[u8]) -> Option<Vec<Range>> {
    let mut bytes = packed.strip_prefix(b"BPCM")?.iter();
    let mut varint = || -> Option<u32> {
        let mut value = 0u32;
        for shift in (0..35).step_by(7) {
            let byte = *bytes.next()?;
            value |= u32::from(byte & 0x7f) << shift;
            if byte & 0x80 == 0 {
                return Some(value);
            }
        }
        None
    };
    let count = varint()?;
    let mut ranges = Vec::with_capacity(count as usize);
    let (mut end, mut cid_end) = (0u32, 0i64);
    for _ in 0..count {
        let start = end.checked_add(varint()?)?;
        let length = varint()?;
        let zigzag = i64::from(varint()?);
        let delta = if zigzag % 2 == 0 {
            zigzag / 2
        } else {
            -(zigzag + 1) / 2
        };
        let cid = u32::try_from(cid_end + delta).ok()?;
        ranges.push(Range {
            start,
            count: length,
            cid,
        });
        end = start.checked_add(length)?;
        cid_end = i64::from(cid) + i64::from(length);
    }
    Some(ranges)
}

/// The character a UTF-16 code (two or four bytes, as one number) stands
/// for.
pub(crate) fn utf16_character(code: u32) -> Option<char> {
    if code > 0xffff {
        let (high, low) = ((code >> 16) as u16, code as u16);
        char::decode_utf16([high, low]).next()?.ok()
    } else {
        char::from_u32(code)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodings_name_their_cmaps() {
        assert_eq!(
            UnicodeCMap::from_encoding(b"UniJIS-UCS2-H"),
            Some((UnicodeCMap::Jis, false))
        );
        assert_eq!(
            UnicodeCMap::from_encoding(b"UniGB-UTF16-H"),
            Some((UnicodeCMap::Gb, true))
        );
        assert_eq!(UnicodeCMap::from_encoding(b"UniKS-UTF16-V"), None);
        assert_eq!(UnicodeCMap::from_encoding(b"Identity-H"), None);
    }

    #[test]
    fn characters_map_to_their_adobe_cids() {
        // From the CMap files: <0020> <007e> 1 in each; 中 and あ.
        for cmap in [
            UnicodeCMap::Gb,
            UnicodeCMap::Cns,
            UnicodeCMap::Jis,
            UnicodeCMap::Ks,
        ] {
            assert_eq!(cmap.cid(' '), Some(1), "{cmap:?}");
            if cmap != UnicodeCMap::Jis {
                assert_eq!(cmap.cid('~'), Some(95), "{cmap:?}");
            }
        }
        assert_eq!(UnicodeCMap::Gb.cid('中'), Some(4559));
        assert_eq!(UnicodeCMap::Jis.cid('あ'), Some(843));
        assert_eq!(UnicodeCMap::Jis.cid('~'), Some(100));
        assert_eq!(UnicodeCMap::Jis.cid('\u{e000}'), None);
        // A supplementary-plane character, coded as a surrogate pair.
        assert_eq!(utf16_character(0xD840_DC0B), Some('\u{2000b}'));
        assert!(UnicodeCMap::Jis.cid('\u{2000b}').is_some());
    }
}
