//! The reversible "Data Encryption" of \[MS-OVBA\] 2.4.3 that obfuscates the
//! `CMG` (protection state), `DPB` (password) and `GC` (visibility) values
//! of the `PROJECT` stream, plus a small source of randomness for seeds and
//! GUIDs (no external RNG crate is used).

use std::{
    collections::hash_map::RandomState,
    fmt::Write,
    hash::{
        BuildHasher,
        Hasher,
    },
    sync::atomic::{
        AtomicU64,
        Ordering,
    },
};

// `web_time`: the browser's clock on wasm32, where `std::time::SystemTime::now`
// panics.
use web_time::{
    SystemTime,
    UNIX_EPOCH,
};

/// Encryption version mandated by the specification.
const VERSION: u8 = 2;

/// Checksum of the project id string (`{GUID}` including the braces).
pub(crate) fn project_key(project_id: &str) -> u8 {
    project_id.bytes().fold(0u8, u8::wrapping_add)
}

/// Encrypts `data` (\[MS-OVBA\] 2.4.3.2) and returns the upper-case
/// hexadecimal text stored in the `PROJECT` stream.
///
/// `ignored` supplies the arbitrary filler bytes; only the first
/// `(seed & 6) / 2` of them are used (missing ones count as zero).
pub(crate) fn encrypt(data: &[u8], project_id: &str, seed: u8, ignored: &[u8]) -> String {
    let proj_key = project_key(project_id);
    let version_enc = seed ^ VERSION;
    let proj_key_enc = seed ^ proj_key;
    let mut out = vec![seed, version_enc, proj_key_enc];

    let mut unencrypted1 = proj_key;
    let mut encrypted1 = proj_key_enc;
    let mut encrypted2 = version_enc;
    let mut push = |byte: u8, out: &mut Vec<u8>| {
        let enc = byte ^ encrypted2.wrapping_add(unencrypted1);
        out.push(enc);
        encrypted2 = encrypted1;
        encrypted1 = enc;
        unencrypted1 = byte;
    };

    let ignored_length = ((seed & 6) / 2) as usize;
    for i in 0..ignored_length {
        push(ignored.get(i).copied().unwrap_or(0), &mut out);
    }
    // (The values are a few bytes long.)
    let length = u32::try_from(data.len()).unwrap_or(u32::MAX);
    for byte in length.to_le_bytes() {
        push(byte, &mut out);
    }
    for &byte in data {
        push(byte, &mut out);
    }

    let mut hex = String::with_capacity(out.len() * 2);
    for byte in out {
        let _ = write!(hex, "{byte:02X}");
    }
    hex
}

/// Decrypts a `CMG` / `DPB` / `GC` value (\[MS-OVBA\] 2.4.3.3).
///
/// Returns `None` when the text is not valid hexadecimal, is too short, has
/// the wrong version or an inconsistent length field.
pub(crate) fn decrypt(hex: &str) -> Option<Vec<u8>> {
    let bytes = parse_hex(hex)?;
    if bytes.len() < 3 {
        return None;
    }
    let seed = bytes[0];
    let version_enc = bytes[1];
    let proj_key_enc = bytes[2];
    if seed ^ version_enc != VERSION {
        return None;
    }
    let proj_key = seed ^ proj_key_enc;

    let mut unencrypted1 = proj_key;
    let mut encrypted1 = proj_key_enc;
    let mut encrypted2 = version_enc;
    let mut plain = Vec::with_capacity(bytes.len() - 3);
    for &enc in &bytes[3..] {
        let byte = enc ^ encrypted2.wrapping_add(unencrypted1);
        plain.push(byte);
        encrypted2 = encrypted1;
        encrypted1 = enc;
        unencrypted1 = byte;
    }

    let ignored_length = ((seed & 6) / 2) as usize;
    let rest = plain.get(ignored_length..)?;
    if rest.len() < 4 {
        return None;
    }
    let length = u32::from_le_bytes([rest[0], rest[1], rest[2], rest[3]]) as usize;
    let data = &rest[4..];
    if data.len() != length {
        return None;
    }
    Some(data.to_vec())
}

fn parse_hex(text: &str) -> Option<Vec<u8>> {
    let text = text.trim();
    if !text.len().is_multiple_of(2) || !text.is_ascii() {
        return None;
    }
    let raw = text.as_bytes();
    let mut out = Vec::with_capacity(raw.len() / 2);
    for pair in raw.chunks_exact(2) {
        let hi = (pair[0] as char).to_digit(16)?;
        let lo = (pair[1] as char).to_digit(16)?;
        out.push(u8::try_from(hi * 16 + lo).ok()?);
    }
    Some(out)
}

/// What the `CMG` / `DPB` values of a project say about its protection.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "the three bits of CMG and what DPB holds"
)]
pub(crate) struct Protection {
    /// `CMG` bit 0: the user locked the project for viewing.
    pub user_protected: bool,
    /// `CMG` bit 1: the host application protects the project.
    pub host_protected: bool,
    /// `CMG` bit 2: the VBA editor protects the project.
    pub vbe_protected:  bool,
    /// `DPB` holds a password (hash or legacy plain text).
    pub has_password:   bool,
    /// `CMG` or `DPB` is present but cannot be decrypted. Office reports
    /// such a project as unviewable, so it is treated as protected.
    pub undecodable:    bool,
}

impl Protection {
    /// Evaluates the raw (still encrypted) property values.
    pub(crate) fn from_properties(cmg: Option<&str>, dpb: Option<&str>) -> Protection {
        let mut protection = Protection::default();
        if let Some(cmg) = cmg {
            match decrypt(cmg) {
                Some(data) if data.len() == 4 => {
                    let state = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
                    protection.user_protected = state & 1 != 0;
                    protection.host_protected = state & 2 != 0;
                    protection.vbe_protected = state & 4 != 0;
                }
                _ => protection.undecodable = true,
            }
        }
        if let Some(dpb) = dpb {
            match decrypt(dpb) {
                // "A VBA project without a password MUST use 0x00 for Data".
                Some(data) if data == [0x00] => {}
                Some(data) if !data.is_empty() => protection.has_password = true,
                _ => protection.undecodable = true,
            }
        }
        protection
    }

    /// The project must not be read or written offline.
    pub(crate) fn is_protected(self) -> bool {
        self.user_protected
            || self.host_protected
            || self.vbe_protected
            || self.has_password
            || self.undecodable
    }
}

/// Returns 64 unpredictable bits. Not cryptographic; only used for the
/// obfuscation seeds and for fresh project GUIDs.
pub(crate) fn random_u64() -> u64 {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let mut hasher = RandomState::new().build_hasher();
    hasher.write_u64(COUNTER.fetch_add(1, Ordering::Relaxed));
    if let Ok(elapsed) = SystemTime::now().duration_since(UNIX_EPOCH) {
        hasher.write_u128(elapsed.as_nanos());
    }
    // (There are no process ids on wasm32: `process::id` panics there.)
    #[cfg(not(target_arch = "wasm32"))]
    hasher.write_u32(std::process::id());
    hasher.finish()
}

/// A fresh random (version 4) GUID in the
/// `{XXXXXXXX-XXXX-XXXX-XXXX-XXXXXXXXXXXX}` form used by the `ID=` property.
pub(crate) fn new_guid() -> String {
    let a = random_u64();
    let b = random_u64();
    let mut bytes = [0u8; 16];
    bytes[..8].copy_from_slice(&a.to_le_bytes());
    bytes[8..].copy_from_slice(&b.to_le_bytes());
    bytes[6] = (bytes[6] & 0x0F) | 0x40;
    bytes[8] = (bytes[8] & 0x3F) | 0x80;
    format!(
        "{{{:02X}{:02X}{:02X}{:02X}-{:02X}{:02X}-{:02X}{:02X}-{:02X}{:02X}-{:02X}{:02X}{:02X}{:02X}{:02X}{:02X}}}",
        bytes[0],
        bytes[1],
        bytes[2],
        bytes[3],
        bytes[4],
        bytes[5],
        bytes[6],
        bytes[7],
        bytes[8],
        bytes[9],
        bytes[10],
        bytes[11],
        bytes[12],
        bytes[13],
        bytes[14],
        bytes[15]
    )
}

/// Encrypts with a random seed and random filler bytes.
pub(crate) fn encrypt_random(data: &[u8], project_id: &str) -> String {
    let r = random_u64().to_le_bytes();
    encrypt(data, project_id, r[0], &r[1..4])
}

#[cfg(test)]
mod tests {
    use super::*;

    // Values of the PROJECT stream example in [MS-OVBA] 3.1.6; the decrypted
    // fields are listed there (ProjKey 0xDF, Ignored 0x07.., Data).
    const SPEC_CMG: &str = "0705D8E3D8EDDBF1DBF1DBF1DBF1";
    const SPEC_DPB: &str = "0E0CD1ECDFF4E7F5E7F5E7";
    const SPEC_GC: &str = "1517CAF1D6F9D7F9D706";

    /// A string whose byte sum is 0xDF, standing in for the example's
    /// project id (only the checksum enters the algorithm).
    fn id_with_key_df() -> String {
        let id = String::from_utf8(vec![0xDF_u8 - 0x60, 0x60]).unwrap();
        assert_eq!(project_key(&id), 0xDF);
        id
    }

    #[test]
    fn decrypts_the_specification_examples() {
        assert_eq!(decrypt(SPEC_CMG).unwrap(), vec![0, 0, 0, 0]);
        assert_eq!(decrypt(SPEC_DPB).unwrap(), vec![0]);
        assert_eq!(decrypt(SPEC_GC).unwrap(), vec![0xFF]);
    }

    #[test]
    fn encrypts_the_specification_examples() {
        let id = id_with_key_df();
        assert_eq!(encrypt(&[0, 0, 0, 0], &id, 0x07, &[7, 7, 7]), SPEC_CMG);
        assert_eq!(encrypt(&[0], &id, 0x0E, &[7, 7, 7]), SPEC_DPB);
        assert_eq!(encrypt(&[0xFF], &id, 0x15, &[7, 7]), SPEC_GC);
    }

    #[test]
    fn decrypts_values_written_by_excel() {
        // From a workbook saved by Excel (XlsxWriter's example vbaProject.bin).
        assert_eq!(
            decrypt("DBD966ECE4F0E4F0E4F0E4F0").unwrap(),
            vec![0, 0, 0, 0]
        );
        assert_eq!(decrypt("5557E86E18E919E919E9").unwrap(), vec![0]);
        assert_eq!(decrypt("CFCD72F0961011111111EE").unwrap(), vec![0xFF]);
        // The project key is the checksum of the ID string.
        let id = "{8D807122-0657-42C8-BC6F-4B5FD08031C9}";
        assert_eq!(project_key(id), 0x55 ^ 0xE8);
    }

    #[test]
    fn round_trips_for_every_seed() {
        let id = "{21270BA8-1724-4E42-9E4C-C761C3E756EB}";
        for seed in 0..=255u8 {
            for data in [
                &[0u8][..],
                &[0xFF],
                &[0, 0, 0, 0],
                &[1, 2, 3, 4, 5, 6, 7, 8, 9],
            ] {
                let hex = encrypt(data, id, seed, &[0xAA, 0xBB, 0xCC]);
                assert_eq!(decrypt(&hex).as_deref(), Some(data), "seed {seed}");
            }
        }
    }

    #[test]
    fn rejects_garbage() {
        assert_eq!(decrypt(""), None);
        assert_eq!(decrypt("zz"), None);
        assert_eq!(decrypt("0705"), None);
        assert_eq!(decrypt("0705D8E3D8EDDBF1DBF1DBF1DB"), None); // length mismatch
        assert_eq!(decrypt("FFFFFFFFFFFFFFFFFFFF"), None); // wrong version
    }

    #[test]
    fn protection_state() {
        let id = "{00000000-0000-0000-0000-000000000000}";
        let open = Protection::from_properties(
            Some(&encrypt(&[0, 0, 0, 0], id, 0x11, &[1, 2, 3])),
            Some(&encrypt(&[0], id, 0x22, &[1, 2, 3])),
        );
        assert!(!open.is_protected());

        let locked = Protection::from_properties(
            Some(&encrypt(&[1, 0, 0, 0], id, 0x11, &[1, 2, 3])),
            Some(&encrypt(&[0xFF; 29], id, 0x22, &[1, 2, 3])),
        );
        assert!(locked.user_protected && locked.has_password && locked.is_protected());

        let password_only =
            Protection::from_properties(None, Some(&encrypt(b"abc\0", id, 3, &[9])));
        assert!(password_only.is_protected());

        let broken = Protection::from_properties(Some("nonsense"), None);
        assert!(broken.is_protected());

        assert!(!Protection::from_properties(None, None).is_protected());
    }

    #[test]
    fn guid_shape() {
        let g = new_guid();
        assert_eq!(g.len(), 38);
        assert!(g.starts_with('{') && g.ends_with('}'));
        assert_eq!(g.as_bytes()[15], b'4');
        assert_ne!(g, new_guid());
    }
}
