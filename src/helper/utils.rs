#![allow(unused_imports)]

use md5::Digest as _;

pub(crate) fn md5_hash(input: impl AsRef<[u8]>) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";

    let digest = md5::Md5::digest(input);
    let mut output = String::with_capacity(digest.len() * 2);
    for byte in digest.iter().copied() {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

/// Whether `s` starts with an escape of a character, `_xHHHH_`.
fn ooxml_escape_at(s: &str) -> Option<u32> {
    let b = s.as_bytes();
    if b.len() >= 7 && b.starts_with(b"_x") && b[6] == b'_' && b[2..6].iter().all(u8::is_ascii_hexdigit) {
        u32::from_str_radix(&s[2..6], 16).ok()
    } else {
        None
    }
}

/// A string of a cell as Excel writes it in the file: the characters XML
/// cannot hold (U+0001...) as `_xHHHH_`, and text that already looks like
/// such an escape with its `_` as `_x005F_`.
pub(crate) fn escape_ooxml_string(s: &str) -> std::borrow::Cow<'_, str> {
    let bad = |c: char| matches!(c, '\u{0}'..='\u{8}' | '\u{b}' | '\u{c}' | '\u{e}'..='\u{1f}' | '\u{fffe}' | '\u{ffff}');
    if !s.contains(bad) && !s.contains("_x") {
        return s.into();
    }
    let mut out = String::with_capacity(s.len() + 8);
    for (i, c) in s.char_indices() {
        if bad(c) {
            out.push_str(&format!("_x{:04X}_", c as u32));
        } else if c == '_' && ooxml_escape_at(&s[i..]).is_some() {
            out.push_str("_x005F_");
        } else {
            out.push(c);
        }
    }
    out.into()
}

/// The string `escape_ooxml_string` wrote (and Excel writes): `_xHHHH_` back
/// to the character.
pub(crate) fn unescape_ooxml_string(s: String) -> String {
    if !s.contains("_x") {
        return s;
    }
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < s.len() {
        let rest = &s[i..];
        if let Some(c) = ooxml_escape_at(rest).and_then(char::from_u32) {
            out.push(c);
            i += 7;
            continue;
        }
        let c = rest.chars().next().unwrap();
        out.push(c);
        i += c.len_utf8();
    }
    out
}

pub(crate) fn unescape_xml_text(e: &quick_xml::events::BytesText<'_>) -> String {
    let decoded = e.decode().unwrap();
    // Line ends as XML reads them (and Excel does): a CR LF or a CR in the
    // file is a LF. A CR written as `&#13;` or `_x000D_` stays.
    let decoded = if decoded.contains('\r') { decoded.replace("\r\n", "\n").replace('\r', "\n").into() } else { decoded };
    let mut remaining = decoded.as_ref();
    let mut output = String::with_capacity(remaining.len());
    // read_text_into retains XML markup. CDATA contains literal characters,
    // including ampersands that must neither be decoded nor rejected.
    while let Some(start) = remaining.find("<![CDATA[") {
        output.push_str(&quick_xml::escape::unescape(&remaining[..start]).unwrap());
        let content = &remaining[start + 9..];
        let end = content.find("]]>").expect("unterminated CDATA section");
        output.push_str(&content[..end]);
        remaining = &content[end + 3..];
    }
    output.push_str(&quick_xml::escape::unescape(remaining).unwrap());
    output
}

/// A macro that implements the `From` trait for converting from one error type
/// to another.
///
/// # Usage
/// ```
/// # use std::{io,fmt};
///
/// use umya_spreadsheet::from_err;
///
/// #[derive(Debug)]
/// enum MyError {
///     Io(std::io::Error),
/// };
/// # impl fmt::Display for MyError {
/// #   fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
/// #       unimplemented!()
/// #   }
/// # }
/// # impl std::error::Error for MyError {}
///
/// from_err!(std::io::Error, MyError, Io);
///
/// let io_err = io::Error::new(io::ErrorKind::Other, "An I/O error occurred");
/// let my_err: MyError = io_err.into();
/// ```
#[macro_export]
macro_rules! from_err {
    ($from:ty, $to:tt, $var:tt) => {
        impl From<$from> for $to {
            #[inline]
            fn from(e: $from) -> $to {
                $to::$var(e)
            }
        }
    };
}

/// Asserts that the SHA-256 hash of a given input matches the expected
/// hexadecimal string.
///
/// # Arguments
///
/// * `$input` - The input data to hash.
/// * `$expected_hex` - The expected SHA-256 hash as a hexadecimal string.
///
/// # Panics
///
/// This macro will panic if the actual SHA-256 hash does not match the
/// expected hash.
///
/// # Examples
///
/// ```ignore
/// let data = b"Hello, world!";
/// assert_sha256!(
///     data,
///     "a591a6d40bf420404a011733cfb7b190d62c65bf0bcda32b57b277d9ad9f146e"
/// );
/// // This will not panic
///
/// assert_sha256!(data, "invalid_hash");
/// // This will panic with a message indicating the mismatch
/// ```
macro_rules! assert_sha256 {
    ($input:expr, $expected_hex:expr) => {{
        let hash = Sha256::digest($input).to_vec();
        let expected_bytes = hex_literal::hex!($expected_hex);
        assert_eq!(
            &hash,
            &expected_bytes,
            "SHA256({}) mismatch! Expected: {:?}, Actual: {:?}",
            stringify!($input),
            &expected_bytes
                .iter()
                .map(|b| format!("{:02x}", b))
                .collect::<String>(),
            &hash
                .iter()
                .map(|b| format!("{:02x}", b))
                .collect::<String>()
        );
    }};
}

/// A macro that compiles a regular expression and caches it.
///
/// # Usage
/// ```rust,ignore
/// // Unable to run because function is private
/// let re = compile_regex!(r"^\d+$");
///
/// assert!(re.is_match("123").unwrap());
/// assert!(!re.is_match("abc").unwrap());
/// ```
macro_rules! compile_regex {
    ($re:literal $(,)?) => {{
        static RE: std::sync::OnceLock<fancy_regex::Regex> = std::sync::OnceLock::new();
        RE.get_or_init(|| fancy_regex::Regex::new($re).unwrap())
    }};
}

/// Prints a byte slice as a hex string, prefixed with the variable name.
///
/// This macro takes a reference to a byte slice (`&[u8]`) and prints both
/// the variable name and its hexadecimal representation to stdout.
macro_rules! print_hex {
    ($var:expr) => {
        println!(
            "{} = {}",
            stringify!($var),
            $var.iter()
                .map(|b| format!("{:02x}", b))
                .collect::<String>()
        );
    };
}

/// Prints the SHA-256 hash of a given input as a hexadecimal string.
///
/// # Examples
///
/// ```ignore
/// let data = b"Hello, world!";
/// print_sha256_hex!(data);
/// // Output: SHA256(data) = 5eb63bbbe01eeed093cb22bb8f5acdc3
/// ```
macro_rules! print_sha256_hex {
    ($var:expr) => {
        let hash = Sha256::digest($var);
        println!(
            "SHA256({}) = {}",
            stringify!($var),
            hash.iter()
                .map(|b| format!("{:02x}", b))
                .collect::<String>()
        );
    };
}

pub(crate) use assert_sha256;
pub(crate) use compile_regex;
pub(crate) use print_hex;
pub(crate) use print_sha256_hex;
