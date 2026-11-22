//! MS-OVBA 2.4.1 compression: the examples of section 3.2 and round-trip
//! properties.

use proptest::prelude::*;
use umya_spreadsheet::vba::compression::{
    compress,
    decompress,
    is_lossless,
};

/// 3.2.1 No Compression Example
const NO_COMPRESSION_TEXT: &[u8] = b"abcdefghijklmnopqrstuv.";
const NO_COMPRESSION_BYTES: &[u8] = &[
    0x01, 0x19, 0xB0, 0x00, 0x61, 0x62, 0x63, 0x64, 0x65, 0x66, 0x67, 0x68, 0x00, 0x69, 0x6A, 0x6B,
    0x6C, 0x6D, 0x6E, 0x6F, 0x70, 0x00, 0x71, 0x72, 0x73, 0x74, 0x75, 0x76, 0x2E,
];

/// 3.2.2 Normal Compression Example
const NORMAL_TEXT: &[u8] = b"#aaabcdefaaaaghijaaaaaklaaamnopqaaaaaaaaaaaarstuvwxyzaaa";
const NORMAL_BYTES: &[u8] = &[
    0x01, 0x2F, 0xB0, 0x00, 0x23, 0x61, 0x61, 0x61, 0x62, 0x63, 0x64, 0x65, 0x82, 0x66, 0x00, 0x70,
    0x61, 0x67, 0x68, 0x69, 0x6A, 0x01, 0x38, 0x08, 0x61, 0x6B, 0x6C, 0x00, 0x20, 0x6D, 0x6E, 0x6F,
    0x70, 0x06, 0x71, 0x02, 0x70, 0x04, 0x00, 0x72, 0x73, 0x74, 0x75, 0x76, 0x10, 0x77, 0x78, 0x79,
    0x7A, 0x00, 0x2C,
];

/// 3.2.3 Maximum Compression Example
const MAXIMUM_TEXT: &[u8] =
    b"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const MAXIMUM_BYTES: &[u8] = &[0x01, 0x03, 0xB0, 0x02, 0x61, 0x45, 0x00];

#[test]
fn specification_examples_decompress() {
    assert_eq!(
        decompress(NO_COMPRESSION_BYTES).unwrap(),
        NO_COMPRESSION_TEXT
    );
    assert_eq!(decompress(NORMAL_BYTES).unwrap(), NORMAL_TEXT);
    assert_eq!(MAXIMUM_TEXT.len(), 73);
    assert_eq!(decompress(MAXIMUM_BYTES).unwrap(), MAXIMUM_TEXT);
}

#[test]
fn specification_examples_compress_to_the_same_bytes() {
    assert_eq!(compress(NO_COMPRESSION_TEXT), NO_COMPRESSION_BYTES);
    assert_eq!(compress(NORMAL_TEXT), NORMAL_BYTES);
    assert_eq!(compress(MAXIMUM_TEXT), MAXIMUM_BYTES);
}

/// Deterministic bytes without usable repetitions.
fn noise(len: usize, seed: u64) -> Vec<u8> {
    let mut state = seed | 1;
    (0..len)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            (state >> 24) as u8
        })
        .collect()
}

/// The reference "Matching" algorithm of MS-OVBA 2.4.1.3.19.4, written the
/// slow, obvious way, to check that the fast match finder picks the same
/// tokens.
fn reference_compress(input: &[u8]) -> Vec<u8> {
    fn bit_count(difference: usize) -> u32 {
        let mut bits = 4;
        while (1usize << bits) < difference {
            bits += 1;
        }
        bits
    }
    let mut out = vec![0x01u8];
    for chunk in input.chunks(4096) {
        let start = out.len();
        let end = start + 4098;
        out.extend([0, 0]);
        let mut pos = 0usize;
        while pos < chunk.len() && out.len() < end {
            let flag_at = out.len();
            out.push(0);
            for bit in 0..8 {
                if pos >= chunk.len() || out.len() >= end {
                    break;
                }
                let mut best_len = 0usize;
                let mut best_candidate = 0usize;
                let mut candidate = pos;
                while candidate > 0 {
                    candidate -= 1;
                    let mut len = 0usize;
                    while pos + len < chunk.len() && chunk[candidate + len] == chunk[pos + len] {
                        len += 1;
                    }
                    if len > best_len {
                        best_len = len;
                        best_candidate = candidate;
                    }
                }
                if best_len >= 3 {
                    let bits = bit_count(pos);
                    let length = best_len.min((0xFFFF >> bits) + 3);
                    let offset = pos - best_candidate;
                    if out.len() + 1 < end {
                        let token = (((offset - 1) as u16) << (16 - bits)) | (length - 3) as u16;
                        out.extend(token.to_le_bytes());
                        out[flag_at] |= 1 << bit;
                        pos += length;
                    } else {
                        out.resize(end, 0); // "SET CompressedCurrent TO CompressedEnd"
                    }
                } else {
                    out.push(chunk[pos]);
                    pos += 1;
                }
            }
        }
        if pos < chunk.len() {
            out.truncate(start + 2);
            out.extend_from_slice(chunk);
            out.resize(end, 0);
            out[start..start + 2].copy_from_slice(&0x3FFFu16.to_le_bytes());
        } else {
            let header = ((out.len() - start - 3) as u16) | 0xB000;
            out[start..start + 2].copy_from_slice(&header.to_le_bytes());
        }
    }
    out
}

#[test]
fn incompressible_full_chunk_is_stored_raw() {
    let data = noise(4096, 0x1234_5678_9ABC_DEF1);
    let compressed = compress(&data);
    // signature + header + 4096 raw bytes
    assert_eq!(compressed.len(), 1 + 2 + 4096);
    assert_eq!(&compressed[1..3], &[0xFF, 0x3F]);
    assert_eq!(&compressed[3..], &data[..]);
    assert!(is_lossless(&data, &compressed));
    assert_eq!(decompress(&compressed).unwrap(), data);
}

#[test]
fn incompressible_partial_last_chunk_is_zero_padded() {
    // The one documented lossy corner of the format (2.4.1.3.10).
    let data = noise(4096 + 4000, 42);
    let compressed = compress(&data);
    assert_eq!(compressed.len(), 1 + 2 * (2 + 4096));
    assert!(!is_lossless(&data, &compressed));
    let out = decompress(&compressed).unwrap();
    assert_eq!(out.len(), 2 * 4096);
    assert_eq!(&out[..data.len()], &data[..]);
    assert!(out[data.len()..].iter().all(|b| *b == 0));
}

#[test]
fn multi_chunk_text_round_trips() {
    let mut text = String::new();
    for i in 0..2000 {
        text.push_str(&format!(
            "    Cells({i}, {}).Value = \"行{i}\" & vbCrLf\r\n",
            i % 7
        ));
    }
    let data = text.into_bytes();
    assert!(data.len() > 3 * 4096);
    let compressed = compress(&data);
    assert!(compressed.len() < data.len() / 2);
    assert!(is_lossless(&data, &compressed));
    assert_eq!(decompress(&compressed).unwrap(), data);
    assert_eq!(compressed, reference_compress(&data));
}

#[test]
fn chunk_boundaries_round_trip() {
    for len in [1usize, 2, 3, 4095, 4096, 4097, 8191, 8192, 8193, 12288] {
        let data: Vec<u8> = (0..len).map(|i| b"abcabcabd\r\n"[i % 11]).collect();
        let compressed = compress(&data);
        assert_eq!(decompress(&compressed).unwrap(), data, "len {len}");
        assert_eq!(compressed, reference_compress(&data), "len {len}");
    }
}

#[test]
fn decompress_rejects_or_survives_arbitrary_garbage() {
    for seed in 0..200u64 {
        let mut data = noise(1 + (seed as usize * 37) % 9000, seed + 1);
        data[0] = 0x01;
        // Force valid chunk signatures so that the token decoder is reached.
        if data.len() > 2 {
            data[2] = (data[2] & 0x8F) | 0x30;
        }
        let _ = decompress(&data);
    }
}

/// Mixes compressible and incompressible stretches.
fn mixed_bytes() -> impl Strategy<Value = Vec<u8>> {
    let piece = prop_oneof![
        // repeated short pattern
        (proptest::collection::vec(any::<u8>(), 1..12), 1usize..600).prop_map(
            |(pattern, times)| pattern
                .iter()
                .cycle()
                .take(pattern.len() * times)
                .copied()
                .collect::<Vec<u8>>()
        ),
        // noise
        (0usize..5000, any::<u64>()).prop_map(|(len, seed)| noise(len, seed)),
        // text-like bytes
        proptest::collection::vec(
            prop_oneof![Just(b' '), Just(b'\r'), Just(b'\n'), 0x41u8..0x5B],
            0..3000
        ),
    ];
    proptest::collection::vec(piece, 0..6).prop_map(|pieces| pieces.concat())
}

fn check_round_trip(data: &[u8]) -> Result<(), TestCaseError> {
    let compressed = compress(data);
    let out = decompress(&compressed).map_err(|e| TestCaseError::fail(e.to_string()))?;
    if is_lossless(data, &compressed) {
        prop_assert_eq!(&out[..], data);
    } else {
        // Only an incompressible, partial last chunk may grow, and only by
        // zero bytes up to the chunk size.
        prop_assert!(data.len() % 4096 != 0);
        prop_assert_eq!(out.len(), data.len().div_ceil(4096) * 4096);
        prop_assert_eq!(&out[..data.len()], data);
        prop_assert!(out[data.len()..].iter().all(|b| *b == 0));
    }
    // Every chunk header carries the signature and a size within bounds.
    let mut pos = 1usize;
    let mut chunks = 0usize;
    while pos < compressed.len() {
        let header = u16::from_le_bytes([compressed[pos], compressed[pos + 1]]);
        prop_assert_eq!(header & 0x7000, 0x3000);
        let size = (header & 0x0FFF) as usize + 3;
        if header & 0x8000 == 0 {
            prop_assert_eq!(size, 4098);
        }
        pos += size;
        chunks += 1;
    }
    prop_assert_eq!(pos, compressed.len());
    prop_assert_eq!(chunks, data.len().div_ceil(4096));
    Ok(())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(96))]

    #[test]
    fn round_trip_arbitrary_bytes(data in proptest::collection::vec(any::<u8>(), 0..10_000)) {
        check_round_trip(&data)?;
    }

    #[test]
    fn round_trip_mixed_content(data in mixed_bytes()) {
        check_round_trip(&data)?;
    }

    #[test]
    fn fast_matcher_equals_reference_algorithm(data in mixed_bytes()) {
        let data = &data[..data.len().min(9000)];
        prop_assert_eq!(compress(data), reference_compress(data));
    }

    #[test]
    fn decompress_never_panics(data in proptest::collection::vec(any::<u8>(), 0..2048)) {
        let _ = decompress(&data);
        let mut with_signature = vec![0x01u8];
        with_signature.extend_from_slice(&data);
        let _ = decompress(&with_signature);
    }
}
