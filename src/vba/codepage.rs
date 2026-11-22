//! Windows code page <-> UTF-8 conversion for the MBCS text of a VBA project.

use encoding_rs::Encoding;

use super::error::VbaError;

/// Code page used when a brand-new project is created without an explicit
/// choice (Japanese Windows, `Shift_JIS` / Windows-31J).
pub(crate) const DEFAULT_CODEPAGE: u16 = 932;

/// Maps a Windows code page number to the `encoding_rs` implementation.
///
/// Supported: 874, 932, 936, 949, 950, 1250–1258, 866, 10000 (Mac Roman),
/// 10001 (Mac Japanese, treated as `Shift_JIS`), 10007 (Mac Cyrillic), 20866,
/// 21866, 28592–28608 (ISO 8859), 51932, 54936 and — for reading only —
/// 65001.
pub(crate) fn encoding_for(codepage: u16) -> Option<&'static Encoding> {
    use encoding_rs::{
        BIG5,
        EUC_JP,
        EUC_KR,
        GB18030,
        GBK,
        IBM866,
        ISO_8859_2,
        ISO_8859_3,
        ISO_8859_4,
        ISO_8859_5,
        ISO_8859_6,
        ISO_8859_7,
        ISO_8859_8,
        ISO_8859_10,
        ISO_8859_13,
        ISO_8859_14,
        ISO_8859_15,
        ISO_8859_16,
        KOI8_R,
        KOI8_U,
        MACINTOSH,
        SHIFT_JIS,
        UTF_8,
        WINDOWS_874,
        WINDOWS_1250,
        WINDOWS_1251,
        WINDOWS_1252,
        WINDOWS_1253,
        WINDOWS_1254,
        WINDOWS_1255,
        WINDOWS_1256,
        WINDOWS_1257,
        WINDOWS_1258,
        X_MAC_CYRILLIC,
    };
    Some(match codepage {
        874 => WINDOWS_874,
        // Mac OS Japanese (10001) is Shift_JIS with a handful of vendor
        // differences.
        932 | 10001 => SHIFT_JIS,
        936 => GBK,
        949 => EUC_KR,
        950 => BIG5,
        1250 => WINDOWS_1250,
        1251 => WINDOWS_1251,
        1252 => WINDOWS_1252,
        1253 => WINDOWS_1253,
        1254 => WINDOWS_1254,
        1255 => WINDOWS_1255,
        1256 => WINDOWS_1256,
        1257 => WINDOWS_1257,
        1258 => WINDOWS_1258,
        866 => IBM866,
        10000 => MACINTOSH,
        10007 => X_MAC_CYRILLIC,
        20866 => KOI8_R,
        21866 => KOI8_U,
        28592 => ISO_8859_2,
        28593 => ISO_8859_3,
        28594 => ISO_8859_4,
        28595 => ISO_8859_5,
        28596 => ISO_8859_6,
        28597 => ISO_8859_7,
        28598 => ISO_8859_8,
        28600 => ISO_8859_10,
        28603 => ISO_8859_13,
        28604 => ISO_8859_14,
        28605 => ISO_8859_15,
        28606 => ISO_8859_16,
        51932 => EUC_JP,
        54936 => GB18030,
        65001 => UTF_8,
        _ => return None,
    })
}

/// Returns the encoding for a code page a project may be *written* in.
/// UTF-8 (65001) is not a valid VBA project code page.
pub(crate) fn writable_encoding(codepage: u16) -> Result<&'static Encoding, VbaError> {
    if codepage == 65001 {
        return Err(VbaError::Encoding(
            "code page 65001 (UTF-8) is not a valid VBA project code page".into(),
        ));
    }
    encoding_for(codepage)
        .ok_or_else(|| VbaError::Encoding(format!("unsupported code page {codepage}")))
}

fn readable_encoding(codepage: u16) -> Result<&'static Encoding, VbaError> {
    encoding_for(codepage)
        .ok_or_else(|| VbaError::Encoding(format!("unsupported code page {codepage}")))
}

/// Decodes MBCS bytes; malformed sequences become U+FFFD.
pub(crate) fn decode(codepage: u16, bytes: &[u8]) -> Result<String, VbaError> {
    let encoding = readable_encoding(codepage)?;
    let (text, _) = encoding.decode_without_bom_handling(bytes);
    Ok(text.into_owned())
}

/// Decodes MBCS bytes and fails on malformed sequences. `what` names the
/// text in the error message.
pub(crate) fn decode_strict(codepage: u16, bytes: &[u8], what: &str) -> Result<String, VbaError> {
    let encoding = readable_encoding(codepage)?;
    match encoding.decode_without_bom_handling_and_without_replacement(bytes) {
        Some(text) => Ok(text.into_owned()),
        None => Err(VbaError::Encoding(format!(
            "{what} contains bytes that are not valid in code page {codepage}"
        ))),
    }
}

/// Encodes text; a character that the code page cannot represent is an
/// error (never silently replaced). `what` names the text in the message.
pub(crate) fn encode(codepage: u16, text: &str, what: &str) -> Result<Vec<u8>, VbaError> {
    let encoding = if codepage == 65001 {
        encoding_rs::UTF_8
    } else {
        writable_encoding(codepage)?
    };
    let (bytes, _, had_unmappable) = encoding.encode(text);
    if !had_unmappable {
        return Ok(bytes.into_owned());
    }
    // Find the offending character for a useful message.
    let mut line = 1usize;
    let mut buf = [0u8; 4];
    for ch in text.chars() {
        if ch == '\n' {
            line += 1;
        }
        let (_, _, bad) = encoding.encode(ch.encode_utf8(&mut buf));
        if bad {
            return Err(VbaError::Encoding(format!(
                "{what}: character {ch:?} (U+{:04X}) on line {line} cannot be represented in code page {codepage}",
                ch as u32
            )));
        }
    }
    Err(VbaError::Encoding(format!(
        "{what} cannot be represented in code page {codepage}"
    )))
}

/// UTF-16LE bytes of `text`.
pub(crate) fn utf16le(text: &str) -> Vec<u8> {
    text.encode_utf16().flat_map(u16::to_le_bytes).collect()
}

/// Decodes UTF-16LE bytes (lossy; an odd trailing byte is ignored).
pub(crate) fn from_utf16le(bytes: &[u8]) -> String {
    let units: Vec<u16> = bytes
        .chunks_exact(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .collect();
    String::from_utf16_lossy(&units)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cp932_round_trip() {
        let text = "Sub あいさつ()\r\n    MsgBox \"こんにちは、世界\"\r\nEnd Sub\r\n";
        let bytes = encode(932, text, "test").unwrap();
        assert_ne!(bytes, text.as_bytes());
        assert_eq!(decode(932, &bytes).unwrap(), text);
        assert_eq!(decode_strict(932, &bytes, "test").unwrap(), text);
    }

    #[test]
    fn required_code_pages_exist() {
        for cp in [
            932u16, 936, 949, 950, 1250, 1251, 1252, 1253, 1254, 1255, 1256, 1257, 1258,
        ] {
            assert!(writable_encoding(cp).is_ok(), "code page {cp}");
        }
        assert!(writable_encoding(65001).is_err());
        assert!(writable_encoding(12345).is_err());
    }

    #[test]
    fn unmappable_characters_are_reported() {
        let err = encode(932, "x = \"😀\"", "module M").unwrap_err();
        let text = err.to_string();
        assert!(text.contains("U+1F600"), "{text}");
        assert!(encode(1252, "日本", "m").is_err());
    }

    #[test]
    fn utf16_helpers() {
        assert_eq!(utf16le("Aあ"), vec![0x41, 0x00, 0x42, 0x30]);
        assert_eq!(from_utf16le(&[0x41, 0x00, 0x42, 0x30]), "Aあ");
    }
}
