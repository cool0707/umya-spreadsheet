//! Best-effort detection of "VBA stomping": a module whose stored source
//! text no longer corresponds to the compiled p-code next to it.
//!
//! The performance cache is version dependent and undocumented, so only one
//! robust fact is used: the cache of a module contains a line table (found
//! through the magic value `0xCAFE`), and the number of lines in it equals
//! the number of source lines that are not `Attribute` lines. When the line
//! table says something else than the source, the source was replaced after
//! the module was compiled. Anything that cannot be parsed with confidence
//! yields "unknown" (`None`), never a suspicion.

use super::source::split_lines;

fn u16_at(data: &[u8], at: usize) -> Option<u16> {
    let b = data.get(at..at.checked_add(2)?)?;
    Some(u16::from_le_bytes([b[0], b[1]]))
}

fn u32_at(data: &[u8], at: usize) -> Option<u32> {
    let b = data.get(at..at.checked_add(4)?)?;
    Some(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

/// Number of lines recorded in the p-code line table of a module's
/// performance cache (`cache` = the module stream up to MODULEOFFSET).
///
/// `vba_version` is the `Version` field of `_VBA_PROJECT`. Only the layout
/// used by VBA 6 and 7 (version >= 0x6B, little endian) is understood.
pub(crate) fn pcode_line_count(cache: &[u8], vba_version: u16) -> Option<usize> {
    if vba_version < 0x006B || vba_version == 0xFFFF {
        return None;
    }
    // Offset 0x19 holds the offset of the "code" block; the line table
    // header starts 0x3C bytes into it.
    let base = u32_at(cache, 0x19)? as usize;
    let table = base.checked_add(0x3C)?;
    if u16_at(cache, table)? != 0xCAFE {
        return None;
    }
    let count = u16_at(cache, table + 4)? as usize;
    // Each line record takes 12 bytes; the table must fit into the cache.
    let end = table.checked_add(6)?.checked_add(count.checked_mul(12)?)?;
    if end > cache.len() {
        return None;
    }
    Some(count)
}

/// `(logical, physical)` number of code lines of a module source:
/// `Attribute` lines are not code lines; `logical` counts a statement that
/// is continued with ` _` once.
pub(crate) fn source_line_counts(source: &str) -> (usize, usize) {
    let mut physical = 0usize;
    let mut logical = 0usize;
    let mut continued = false;
    for line in split_lines(source) {
        let trimmed = line.trim_start();
        let is_attribute = trimmed
            .get(..10)
            .is_some_and(|p| p.eq_ignore_ascii_case("attribute "));
        if is_attribute && !continued {
            continue;
        }
        physical += 1;
        if !continued {
            logical += 1;
        }
        let t = line.trim_end();
        continued = t == "_" || t.ends_with(" _") || t.ends_with("\t_");
    }
    (logical, physical)
}

/// `Some(true)` when the p-code line table contradicts the source text,
/// `Some(false)` when they agree, `None` when nothing can be said.
pub(crate) fn is_stomped(cache: &[u8], vba_version: u16, source: &str) -> Option<bool> {
    let lines = pcode_line_count(cache, vba_version)?;
    let (logical, physical) = source_line_counts(source);
    Some(lines < logical || lines > physical)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_code_lines() {
        let src = "Attribute VB_Name = \"M\"\r\nSub A()\r\nAttribute A.VB_ProcData.VB_Invoke_Func = \" \\n14\"\r\n'\r\n\r\n    x = 1 + _\r\n        2\r\nEnd Sub\r\n";
        assert_eq!(source_line_counts(src), (5, 6));
        assert_eq!(source_line_counts("Attribute VB_Name = \"M\"\r\n"), (0, 0));
        assert_eq!(source_line_counts("Attribute VB_Name = \"M\"\n\n"), (1, 1));
    }

    #[test]
    fn unknown_layouts_give_no_verdict() {
        assert_eq!(pcode_line_count(&[], 0x97), None);
        assert_eq!(pcode_line_count(&[0u8; 64], 0x97), None);
        assert_eq!(pcode_line_count(&[0u8; 64], 0xFFFF), None);
        assert_eq!(is_stomped(&[0u8; 8], 0x97, "x"), None);
    }

    #[test]
    fn synthetic_line_table() {
        let mut cache = vec![0u8; 0x200];
        cache[0x19..0x1D].copy_from_slice(&0x40u32.to_le_bytes());
        let table = 0x40 + 0x3C;
        cache[table..table + 2].copy_from_slice(&0xCAFEu16.to_le_bytes());
        cache[table + 4..table + 6].copy_from_slice(&3u16.to_le_bytes());
        assert_eq!(pcode_line_count(&cache, 0x97), Some(3));
        let three = "Attribute VB_Name = \"M\"\r\nSub A()\r\nx = 1\r\nEnd Sub\r\n";
        assert_eq!(is_stomped(&cache, 0x97, three), Some(false));
        assert_eq!(
            is_stomped(&cache, 0x97, "Attribute VB_Name = \"M\"\r\n"),
            Some(true)
        );
        assert_eq!(
            is_stomped(&cache, 0x97, "a\r\nb\r\nc\r\nd\r\ne\r\n"),
            Some(true)
        );
    }
}
