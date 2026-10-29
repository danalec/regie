//! Integration tests for the `RGDF` artifact format.
//!
//! The automata are built with [`Dfa::from_parts`] on purpose: the test suite
//! must not depend on the pattern compiler of `regie-core`.

use regie_codec::{
    decode, encode, encoded_len, parse_header, Capacity, CodecError, FLAG_ACCEPT_AT_END,
    HEADER_LEN, MAGIC, VERSION,
};
use regie_core::dfa::Dfa;
use regie_core::RegexError;

/// Two byte classes: class 1 is `a` or `b`, class 0 is everything else.
fn sample_byte_classes() -> [u8; 256] {
    let mut byte_class = [0u8; 256];
    for byte in b"ab" {
        byte_class[*byte as usize] = 1;
    }
    byte_class
}

/// A three state automaton: 0 = dead, 1 = start, 2 = accepting.
///
/// It matches a haystack that contains an `a` or a `b`.
fn sample_dfa() -> Dfa {
    // Row-major over (state, class), two classes and three states.
    let transitions = [0u16, 0, 0, 2, 0, 0];
    // Only state 2 accepts.
    let accept = [0b0000_0100u8];
    Dfa::from_parts(2, 3, &sample_byte_classes(), &accept, &transitions).unwrap()
}

/// A larger automaton: four classes and twelve states, so that the accept
/// bitmap spans two bytes.
fn large_dfa() -> Dfa {
    let mut byte_class = [0u8; 256];
    for byte in b"a" {
        byte_class[*byte as usize] = 1;
    }
    for byte in b"b" {
        byte_class[*byte as usize] = 2;
    }
    for byte in b"0123456789" {
        byte_class[*byte as usize] = 3;
    }

    let states = 12usize;
    let classes = 4usize;
    let mut transitions = vec![0u16; states * classes];
    for state in 1..states {
        for class in 0..classes {
            transitions[state * classes + class] = ((state + class + 1) % states) as u16;
        }
    }

    let mut accept = [0u8; 2];
    for state in [2usize, 5, 9] {
        accept[state / 8] |= 1u8 << (state % 8);
    }

    Dfa::from_parts(
        classes as u16,
        states as u16,
        &byte_class,
        &accept,
        &transitions,
    )
    .unwrap()
}

/// The accept bitmap of `dfa`, masked to the bits of its real states.
fn masked_accept(dfa: &Dfa) -> Vec<u8> {
    let states = dfa.state_count();
    dfa.accept_bitmap()
        .iter()
        .enumerate()
        .map(|(index, &byte)| {
            let bits = states.saturating_sub(index * 8).min(8);
            let mask = if bits == 8 { 0xff } else { (1u8 << bits) - 1 };
            byte & mask
        })
        .collect()
}

/// A corpus of haystacks used to compare two automata.
fn corpus() -> Vec<Vec<u8>> {
    let mut inputs: Vec<Vec<u8>> = (0u16..=255).map(|byte| vec![byte as u8]).collect();
    for extra in [
        &b""[..],
        b"a",
        b"b",
        b"ab",
        b"ba",
        b"abc",
        b"zzz",
        b"a1",
        b"12",
        b"hello world",
        b"\x00\x01\x02",
    ] {
        inputs.push(extra.to_vec());
    }
    inputs
}

#[test]
fn round_trip_preserves_everything() {
    for dfa in [sample_dfa(), large_dfa()] {
        let mut artifact = vec![0u8; encoded_len(&dfa)];
        let written = encode(&dfa, &mut artifact).unwrap();
        assert_eq!(written, artifact.len());
        assert_eq!(written, encoded_len(&dfa));

        let restored = decode(&artifact).unwrap();

        assert_eq!(restored.state_count(), dfa.state_count());
        assert_eq!(restored.class_count(), dfa.class_count());
        assert_eq!(restored.byte_classes(), dfa.byte_classes());
        assert_eq!(restored.accept_bitmap(), masked_accept(&dfa).as_slice());
        assert_eq!(restored.transitions(), dfa.transitions());

        for input in corpus() {
            assert_eq!(
                restored.is_match(&input),
                dfa.is_match(&input),
                "is_match disagrees on {:?}",
                input
            );
            assert_eq!(
                restored.find_end(&input),
                dfa.find_end(&input),
                "find_end disagrees on {:?}",
                input
            );
        }
    }
}

#[test]
fn round_trip_keeps_the_matching_behaviour() {
    let dfa = sample_dfa();
    let mut artifact = vec![0u8; encoded_len(&dfa)];
    encode(&dfa, &mut artifact).unwrap();
    let restored = decode(&artifact).unwrap();

    assert!(restored.is_match(b"a"));
    assert!(restored.is_match(b"ab"));
    assert!(!restored.is_match(b""));
    assert!(!restored.is_match(b"c"));

    // The decoded automaton agrees with the source on every input.
    for input in corpus() {
        assert_eq!(restored.is_match(&input), dfa.is_match(&input));
    }
}

#[test]
fn encoding_is_deterministic() {
    let dfa = large_dfa();
    let mut first = vec![0u8; encoded_len(&dfa)];
    let mut second = vec![0u8; encoded_len(&dfa)];
    let a = encode(&dfa, &mut first).unwrap();
    let b = encode(&dfa, &mut second).unwrap();

    assert_eq!(a, b);
    assert_eq!(first, second);
}

#[test]
fn every_prefix_of_an_artifact_is_rejected() {
    let dfa = large_dfa();
    let mut artifact = vec![0u8; encoded_len(&dfa)];
    encode(&dfa, &mut artifact).unwrap();

    for length in 0..artifact.len() {
        let result = decode(&artifact[..length]);
        assert!(
            result.is_err(),
            "a prefix of {length} bytes was accepted: {result:?}"
        );
    }

    // The full artifact is of course accepted.
    assert!(decode(&artifact).is_ok());
}

#[test]
fn a_trailing_byte_is_rejected() {
    let dfa = sample_dfa();
    let mut artifact = vec![0u8; encoded_len(&dfa)];
    encode(&dfa, &mut artifact).unwrap();
    artifact.push(0);

    match decode(&artifact) {
        Err(CodecError::LengthMismatch { .. }) => {}
        other => panic!("expected a length mismatch, got {other:?}"),
    }
}

#[test]
fn a_corrupted_payload_fails_the_checksum() {
    let dfa = large_dfa();
    let mut artifact = vec![0u8; encoded_len(&dfa)];
    encode(&dfa, &mut artifact).unwrap();

    // Every byte of the payload is covered by the checksum.
    for index in HEADER_LEN..artifact.len() - 4 {
        let mut corrupted = artifact.clone();
        corrupted[index] ^= 0x01;
        match decode(&corrupted) {
            Err(CodecError::BadChecksum { .. }) => {}
            other => panic!("byte {index}: expected a checksum error, got {other:?}"),
        }
    }
}

#[test]
fn a_wrong_magic_is_rejected() {
    let dfa = sample_dfa();
    let mut artifact = vec![0u8; encoded_len(&dfa)];
    encode(&dfa, &mut artifact).unwrap();
    artifact[0] = b'X';

    match decode(&artifact) {
        Err(CodecError::BadMagic { found }) => assert_eq!(&found[0..1], b"X"),
        other => panic!("expected a bad magic error, got {other:?}"),
    }
}

#[test]
fn an_unknown_version_is_rejected() {
    let dfa = sample_dfa();
    let mut artifact = vec![0u8; encoded_len(&dfa)];
    encode(&dfa, &mut artifact).unwrap();
    artifact[4..6].copy_from_slice(&99u16.to_le_bytes());

    match decode(&artifact) {
        Err(CodecError::UnsupportedVersion { found }) => assert_eq!(found, 99),
        other => panic!("expected an unsupported version error, got {other:?}"),
    }
}

#[test]
fn a_foreign_endianness_marker_is_rejected() {
    let dfa = sample_dfa();
    let mut artifact = vec![0u8; encoded_len(&dfa)];
    encode(&dfa, &mut artifact).unwrap();
    artifact[6] = 0x02;

    match decode(&artifact) {
        Err(CodecError::BadEndianness { found }) => assert_eq!(found, 0x02),
        other => panic!("expected a bad endianness error, got {other:?}"),
    }
}

#[test]
fn unknown_flag_bits_are_rejected() {
    let dfa = sample_dfa();
    let mut artifact = vec![0u8; encoded_len(&dfa)];
    encode(&dfa, &mut artifact).unwrap();
    artifact[7] = 0x80;

    match decode(&artifact) {
        Err(CodecError::UnsupportedFlags { found }) => assert_eq!(found, 0x80),
        other => panic!("expected an unsupported flags error, got {other:?}"),
    }
}

#[test]
fn the_core_rejects_automata_that_do_not_fit() {
    let byte_class = sample_byte_classes();
    let accept = [0u8; 1];
    let transitions = [0u16; 16];

    match Dfa::from_parts(33, 3, &byte_class, &accept, &transitions) {
        Err(RegexError::TooManyClasses { limit }) => assert_eq!(limit, 32),
        other => panic!("expected a too many classes error, got {other:?}"),
    }

    match Dfa::from_parts(2, 257, &byte_class, &accept, &transitions) {
        Err(RegexError::DfaTooLarge { limit }) => assert_eq!(limit, 256),
        other => panic!("expected a DFA too large error, got {other:?}"),
    }
}

#[test]
fn decoding_reports_an_exceeded_capacity() {
    let dfa = sample_dfa();
    let mut artifact = vec![0u8; encoded_len(&dfa)];
    encode(&dfa, &mut artifact).unwrap();

    let mut too_many_states = artifact.clone();
    too_many_states[10..12].copy_from_slice(&257u16.to_le_bytes());
    match decode(&too_many_states) {
        Err(CodecError::CapacityExceeded {
            kind,
            limit,
            actual,
        }) => {
            assert_eq!(kind, Capacity::States);
            assert_eq!(limit, 256);
            assert_eq!(actual, 257);
        }
        other => panic!("expected a capacity error, got {other:?}"),
    }

    let mut too_many_classes = artifact.clone();
    too_many_classes[8..10].copy_from_slice(&33u16.to_le_bytes());
    match decode(&too_many_classes) {
        Err(CodecError::CapacityExceeded {
            kind,
            limit,
            actual,
        }) => {
            assert_eq!(kind, Capacity::Classes);
            assert_eq!(limit, 32);
            assert_eq!(actual, 33);
        }
        other => panic!("expected a capacity error, got {other:?}"),
    }
}

#[test]
fn the_artifact_is_little_endian_regardless_of_the_host() {
    let dfa = large_dfa();
    let mut artifact = vec![0u8; encoded_len(&dfa)];
    encode(&dfa, &mut artifact).unwrap();

    assert_eq!(&artifact[0..4], &MAGIC);
    assert_eq!(&artifact[4..6], &VERSION.to_le_bytes());
    assert_eq!(&artifact[4..6], &[1, 0]);
    assert_eq!(artifact[6], 0x01);
    assert_eq!(artifact[7], 0x00);

    let classes = dfa.class_count() as u16;
    let states = dfa.state_count() as u16;
    assert_eq!(&artifact[8..10], &classes.to_le_bytes());
    assert_eq!(&artifact[10..12], &states.to_le_bytes());
    // 0x0c = 12 states, little-endian.
    assert_eq!(&artifact[10..12], &[0x0c, 0x00]);

    let payload_len = 256 + 2 + states as usize * classes as usize * 2;
    assert_eq!(&artifact[12..16], &(payload_len as u32).to_le_bytes());
    assert_eq!(&artifact[12..16], &[0x62, 0x01, 0x00, 0x00]);

    // The transition targets are stored little-endian as well.
    let transitions_at = 16 + 256 + 2;
    let first = dfa.transitions()[0];
    assert_eq!(
        &artifact[transitions_at..transitions_at + 2],
        &first.to_le_bytes()
    );

    // The documented total length.
    let expected = 20 + 256 + 2 + states as usize * classes as usize * 2;
    assert_eq!(artifact.len(), expected);
    assert_eq!(encoded_len(&dfa), expected);
}

#[test]
fn the_checksum_is_crc32_ieee() {
    let dfa = sample_dfa();
    let mut artifact = vec![0u8; encoded_len(&dfa)];
    encode(&dfa, &mut artifact).unwrap();

    let stored = u32::from_le_bytes([
        artifact[artifact.len() - 4],
        artifact[artifact.len() - 3],
        artifact[artifact.len() - 2],
        artifact[artifact.len() - 1],
    ]);
    assert_eq!(stored, crc32(&artifact[..artifact.len() - 4]));
}

/// The reference CRC-32 (IEEE) used by the format.
fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xffff_ffffu32;
    for &byte in data {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xedb8_8320 & mask);
        }
    }
    !crc
}

#[test]
fn encode_refuses_a_small_buffer() {
    let dfa = sample_dfa();
    let needed = encoded_len(&dfa);
    let mut buffer = vec![0u8; needed - 1];

    match encode(&dfa, &mut buffer) {
        Err(CodecError::TooSmall {
            needed: want,
            actual,
        }) => {
            assert_eq!(want, needed);
            assert_eq!(actual, needed - 1);
        }
        other => panic!("expected a too small error, got {other:?}"),
    }
}

#[test]
fn the_header_describes_the_artifact() {
    let dfa = large_dfa();
    let mut artifact = vec![0u8; encoded_len(&dfa)];
    encode(&dfa, &mut artifact).unwrap();

    let header = parse_header(&artifact).unwrap();
    assert_eq!(header.class_count(), dfa.class_count());
    assert_eq!(header.state_count(), dfa.state_count());
    assert_eq!(header.flags(), 0);
    assert!(!header.has_asserts());
    assert_eq!(
        header.payload_len(),
        256 + 2 + dfa.state_count() * dfa.class_count() * 2
    );
}

#[test]
fn a_short_artifact_is_reported_as_truncated() {
    match decode(&[0u8; 3]) {
        Err(CodecError::Truncated { needed, actual }) => {
            assert_eq!(needed, HEADER_LEN + 4);
            assert_eq!(actual, 3);
        }
        other => panic!("expected a truncated error, got {other:?}"),
    }
}

#[test]
fn an_automaton_without_a_class_is_rejected() {
    let dfa = sample_dfa();
    let mut artifact = vec![0u8; encoded_len(&dfa)];
    encode(&dfa, &mut artifact).unwrap();
    artifact[8..10].copy_from_slice(&0u16.to_le_bytes());

    match decode(&artifact) {
        Err(CodecError::InvalidAutomaton { .. }) => {}
        other => panic!("expected an invalid automaton error, got {other:?}"),
    }
}

#[test]
fn a_stray_bit_in_the_accept_bitmap_is_normalized() {
    // State 2 accepts, but the bitmap also carries a bit for a state that does
    // not exist. The artifact only keeps the bits of the real states.
    let accept = [0b1111_0100u8];
    let dfa = Dfa::from_parts(2, 3, &sample_byte_classes(), &accept, &[0, 0, 0, 2, 0, 0]).unwrap();

    let mut artifact = vec![0u8; encoded_len(&dfa)];
    encode(&dfa, &mut artifact).unwrap();
    let restored = decode(&artifact).unwrap();

    assert_eq!(restored.accept_bitmap(), &[0b0000_0100]);
    assert!(restored.is_accept(2));
    assert!(!restored.is_accept(3));
}

/// An automaton that accepts an `a` and then keeps going.
///
/// State 2 accepts; every byte after the `a` moves to state 3, which never
/// accepts. The automaton therefore only matches a haystack that ends right
/// after the `a` when the end anchor is set, and matches it anywhere when the
/// anchor is not set.
fn end_anchor_dfa() -> Dfa {
    let mut byte_class = [0u8; 256];
    byte_class[b'a' as usize] = 1;

    // Row-major over (state, class), two classes and four states.
    let transitions = [0u16, 0, 0, 2, 3, 3, 3, 3];
    // Only state 2 accepts.
    let accept = [0b0000_0100u8];
    Dfa::from_parts(2, 4, &byte_class, &accept, &transitions).unwrap()
}

#[test]
fn the_end_anchor_survives_the_round_trip() {
    let mut dfa = end_anchor_dfa();
    dfa.set_accept_at_end(true);

    let mut artifact = vec![0u8; encoded_len(&dfa)];
    encode(&dfa, &mut artifact).unwrap();

    let header = parse_header(&artifact).unwrap();
    assert!(header.accept_at_end());
    assert_eq!(header.flags(), FLAG_ACCEPT_AT_END);

    let restored = decode(&artifact).unwrap();
    assert!(restored.accepts_at_end_only());
    assert!(restored.is_match(b"a"));
    assert!(
        !restored.is_match(b"a "),
        "the match has to end at the end of the input"
    );
}

#[test]
fn an_automaton_without_an_end_anchor_is_written_without_the_flag() {
    let dfa = end_anchor_dfa();
    assert!(!dfa.accepts_at_end_only());

    let mut artifact = vec![0u8; encoded_len(&dfa)];
    encode(&dfa, &mut artifact).unwrap();

    let header = parse_header(&artifact).unwrap();
    assert!(!header.accept_at_end());
    assert_eq!(header.flags(), 0);

    let restored = decode(&artifact).unwrap();
    assert!(!restored.accepts_at_end_only());
    assert!(
        restored.is_match(b"a "),
        "without the anchor the match may be followed by more bytes"
    );
}
