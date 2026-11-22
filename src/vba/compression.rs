//! The run-length compression used by VBA streams (\[MS-OVBA\] 2.4.1).
//!
//! A *compressed container* is the signature byte `0x01` followed by
//! chunks. Each chunk holds at most 4096 bytes of decompressed data and is
//! either a sequence of literal / copy tokens or, when that would not fit
//! into 4096 bytes, the raw data.
//!
//! [`compress`] produces exactly the bytes the reference algorithm of the
//! specification produces (same greedy longest-match rule, the nearest
//! candidate winning ties); it only finds the candidates faster.

#![allow(
    clippy::cast_possible_truncation,
    reason = "positions and lengths within a chunk (at most 4098 bytes) fit in 16 bits"
)]

use super::error::{
    VbaError,
    format_err,
};

/// Number of decompressed bytes per chunk.
const CHUNK_SIZE: usize = 4096;
/// `0b011` in bits 12..14 of every chunk header.
const CHUNK_SIGNATURE: u16 = 0x3000;
/// Bit 15 of the chunk header: the chunk contains tokens (not raw data).
const CHUNK_COMPRESSED: u16 = 0x8000;

/// Number of bits used for the offset part of a copy token when
/// `difference` bytes of the chunk have already been produced
/// (\[MS-OVBA\] 2.4.1.3.19.1 "`CopyToken` Help").
fn copy_token_bit_count(difference: usize) -> u32 {
    let mut bit_count = 4u32;
    while (1usize << bit_count) < difference {
        bit_count += 1;
    }
    bit_count
}

/// Compresses `input` into a compressed container.
///
/// An empty input yields a container that consists of the signature byte
/// only.
///
/// Note the one lossy corner of the format (\[MS-OVBA\] 2.4.1.3.10): when the
/// *last* chunk is shorter than 4096 bytes and cannot be stored as tokens, it
/// is stored raw and padded with `0x00` up to 4096 bytes, so
/// [`decompress`] returns those extra zero bytes. [`is_lossless`] tells
/// whether that happened.
#[must_use]
pub fn compress(input: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(input.len() / 2 + 16);
    out.push(0x01);
    let mut finder = MatchFinder::new();
    for chunk in input.chunks(CHUNK_SIZE) {
        compress_chunk(chunk, &mut finder, &mut out);
    }
    out
}

/// Returns `true` when `decompress(compress(input))` gives back exactly
/// `input` (see the note on [`compress`]).
#[must_use]
pub fn is_lossless(input: &[u8], compressed: &[u8]) -> bool {
    let tail = input.len() % CHUNK_SIZE;
    if tail == 0 {
        return true;
    }
    // Only a raw final chunk is padded; a raw chunk is always 4098 bytes.
    let full_chunks = input.len() / CHUNK_SIZE;
    let mut pos = 1usize;
    for _ in 0..full_chunks {
        let Some(header) = read_u16(compressed, pos) else {
            return false;
        };
        pos += (header & 0x0FFF) as usize + 3;
    }
    match read_u16(compressed, pos) {
        Some(header) => header & CHUNK_COMPRESSED != 0,
        None => false,
    }
}

fn read_u16(data: &[u8], pos: usize) -> Option<u16> {
    let bytes = data.get(pos..pos.checked_add(2)?)?;
    Some(u16::from_le_bytes([bytes[0], bytes[1]]))
}

fn compress_chunk(chunk: &[u8], finder: &mut MatchFinder, out: &mut Vec<u8>) {
    let chunk_start = out.len();
    // CompressedEnd: a chunk is at most 2 header bytes + 4096 data bytes.
    let compressed_end = chunk_start + 2 + CHUNK_SIZE;
    out.extend_from_slice(&[0, 0]);

    finder.reset();
    let mut pos = 0usize;
    let mut inserted = 0usize;
    'chunk: while pos < chunk.len() && out.len() < compressed_end {
        let flag_index = out.len();
        out.push(0);
        let mut flags = 0u8;
        for bit in 0..8 {
            if pos >= chunk.len() || out.len() >= compressed_end {
                break;
            }
            // Make every position before `pos` a match candidate.
            while inserted < pos {
                finder.insert(chunk, inserted);
                inserted += 1;
            }
            if let Some((offset, length)) = finder.find(chunk, pos) {
                if out.len() + 1 < compressed_end {
                    let bit_count = copy_token_bit_count(pos);
                    let token = (((offset - 1) as u16) << (16 - bit_count)) | (length - 3) as u16;
                    out.extend_from_slice(&token.to_le_bytes());
                    flags |= 1 << bit;
                    pos += length;
                } else {
                    // The token does not fit: the chunk is full.
                    out[flag_index] = flags;
                    break 'chunk;
                }
            } else {
                out.push(chunk[pos]);
                pos += 1;
            }
        }
        out[flag_index] = flags;
    }

    if pos < chunk.len() {
        // Tokens need more than 4096 bytes: store the chunk raw, padded
        // with zeros to exactly 4096 bytes.
        out.truncate(chunk_start + 2);
        out.extend_from_slice(chunk);
        out.resize(compressed_end, 0);
        let header = (CHUNK_SIZE as u16 + 2 - 3) | CHUNK_SIGNATURE;
        out[chunk_start..chunk_start + 2].copy_from_slice(&header.to_le_bytes());
    } else {
        let size = out.len() - chunk_start;
        let header = ((size - 3) as u16) | CHUNK_SIGNATURE | CHUNK_COMPRESSED;
        out[chunk_start..chunk_start + 2].copy_from_slice(&header.to_le_bytes());
    }
}

const HASH_BITS: u32 = 12;
const NO_POS: u16 = u16::MAX;

/// Finds, for a position in a chunk, the longest earlier occurrence of the
/// bytes that follow (the "Matching" algorithm, \[MS-OVBA\] 2.4.1.3.19.4).
///
/// Candidates are kept in hash chains keyed by their first three bytes; a
/// candidate that does not share three bytes can never produce a copy token,
/// so skipping it does not change the result.
struct MatchFinder {
    head: Vec<u16>,
    prev: Vec<u16>,
}

impl MatchFinder {
    fn new() -> Self {
        MatchFinder {
            head: vec![NO_POS; 1 << HASH_BITS],
            prev: vec![NO_POS; CHUNK_SIZE],
        }
    }

    fn reset(&mut self) {
        self.head.fill(NO_POS);
    }

    fn hash(chunk: &[u8], pos: usize) -> usize {
        let v = u32::from(chunk[pos])
            | (u32::from(chunk[pos + 1]) << 8)
            | (u32::from(chunk[pos + 2]) << 16);
        (v.wrapping_mul(0x9E37_79B1) >> (32 - HASH_BITS)) as usize
    }

    fn insert(&mut self, chunk: &[u8], pos: usize) {
        if pos + 3 > chunk.len() {
            return;
        }
        let h = Self::hash(chunk, pos);
        self.prev[pos] = self.head[h];
        self.head[h] = pos as u16;
    }

    /// Returns `(offset, length)` of the copy sequence for `pos`, if any.
    fn find(&self, chunk: &[u8], pos: usize) -> Option<(usize, usize)> {
        let remaining = chunk.len() - pos;
        if pos == 0 || remaining < 3 {
            return None;
        }
        let mut best_len = 0usize;
        let mut best_candidate = 0usize;
        let mut candidate = self.head[Self::hash(chunk, pos)];
        // Chains run from the nearest candidate to the farthest, which is the
        // order of the reference algorithm; `>` keeps the nearest on ties.
        while candidate != NO_POS {
            let c = candidate as usize;
            let mut len = 0usize;
            while len < remaining && chunk[c + len] == chunk[pos + len] {
                len += 1;
            }
            if len > best_len {
                best_len = len;
                best_candidate = c;
                if len == remaining {
                    break;
                }
            }
            candidate = self.prev[c];
        }
        if best_len < 3 {
            return None;
        }
        let maximum_length = (0xFFFFusize >> copy_token_bit_count(pos)) + 3;
        Some((pos - best_candidate, best_len.min(maximum_length)))
    }
}

/// Decompresses a compressed container.
///
/// Malformed input (missing signature byte, bad chunk signature, copy tokens
/// pointing outside the chunk, truncated tokens) is reported as
/// [`VbaError::Format`]; the function never panics.
pub fn decompress(input: &[u8]) -> Result<Vec<u8>, VbaError> {
    match input.first() {
        Some(0x01) => {}
        Some(other) => {
            return format_err(format!(
                "compressed container starts with 0x{other:02X} instead of 0x01"
            ));
        }
        None => return format_err("compressed container is empty"),
    }
    let mut out = Vec::with_capacity(input.len().saturating_mul(2));
    let mut pos = 1usize;
    while pos < input.len() {
        let Some(header) = read_u16(input, pos) else {
            return format_err("truncated chunk header in compressed container");
        };
        if header & 0x7000 != CHUNK_SIGNATURE {
            return format_err(format!(
                "invalid chunk signature in compressed container (header 0x{header:04X})"
            ));
        }
        let data_start = pos + 2;
        if header & CHUNK_COMPRESSED == 0 {
            // Raw chunk: always 4096 bytes of data.
            let end = input.len().min(data_start + CHUNK_SIZE);
            out.extend_from_slice(&input[data_start..end]);
            pos = end;
            continue;
        }
        let size = (header & 0x0FFF) as usize + 3;
        let chunk_end = input.len().min(pos + size);
        decompress_tokens(&input[data_start..chunk_end], &mut out)?;
        pos = chunk_end;
    }
    Ok(out)
}

fn decompress_tokens(data: &[u8], out: &mut Vec<u8>) -> Result<(), VbaError> {
    let chunk_start = out.len();
    let mut i = 0usize;
    while i < data.len() {
        let flags = data[i];
        i += 1;
        for bit in 0..8 {
            if i >= data.len() {
                break;
            }
            if flags & (1 << bit) == 0 {
                out.push(data[i]);
                i += 1;
                continue;
            }
            if i + 1 >= data.len() {
                return format_err("truncated copy token in compressed chunk");
            }
            let token = u16::from_le_bytes([data[i], data[i + 1]]);
            i += 2;
            let difference = out.len() - chunk_start;
            if difference == 0 {
                return format_err("copy token at the start of a compressed chunk");
            }
            let bit_count = copy_token_bit_count(difference);
            let length_mask = 0xFFFFu16 >> bit_count;
            let length = (token & length_mask) as usize + 3;
            let offset = ((token & !length_mask) >> (16 - bit_count)) as usize + 1;
            if offset > difference {
                return format_err("copy token points before the start of its chunk");
            }
            if difference + length > CHUNK_SIZE {
                return format_err("compressed chunk expands to more than 4096 bytes");
            }
            let source = out.len() - offset;
            for k in 0..length {
                let byte = out[source + k];
                out.push(byte);
            }
        }
    }
    if out.len() - chunk_start > CHUNK_SIZE {
        return format_err("compressed chunk expands to more than 4096 bytes");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bit_counts_follow_the_table() {
        assert_eq!(copy_token_bit_count(1), 4);
        assert_eq!(copy_token_bit_count(16), 4);
        assert_eq!(copy_token_bit_count(17), 5);
        assert_eq!(copy_token_bit_count(32), 5);
        assert_eq!(copy_token_bit_count(33), 6);
        assert_eq!(copy_token_bit_count(2048), 11);
        assert_eq!(copy_token_bit_count(2049), 12);
        assert_eq!(copy_token_bit_count(4096), 12);
    }

    #[test]
    fn empty_input() {
        assert_eq!(compress(b""), vec![0x01]);
        assert_eq!(decompress(&[0x01]).unwrap(), Vec::<u8>::new());
        assert!(decompress(&[]).is_err());
        assert!(decompress(&[0x02, 0x00]).is_err());
    }

    #[test]
    fn malformed_input_is_an_error() {
        // Truncated header.
        assert!(decompress(&[0x01, 0x19]).is_err());
        // Wrong chunk signature.
        assert!(decompress(&[0x01, 0x03, 0x80, 0x00, 0x61]).is_err());
        // Copy token as the very first token.
        assert!(decompress(&[0x01, 0x02, 0xB0, 0x01, 0x00, 0x00]).is_err());
        // Copy token pointing before the chunk start.
        assert!(decompress(&[0x01, 0x03, 0xB0, 0x02, 0x61, 0x00, 0x30]).is_err());
        // Truncated copy token.
        assert!(decompress(&[0x01, 0x02, 0xB0, 0x02, 0x61, 0x45]).is_err());
    }
}
