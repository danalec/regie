//! Implementation of the `RGDF` artifact format.
//!
//! The layout is described byte by byte in the crate documentation. This
//! module only has to agree with that table; everything else is a
//! consequence of it.

use regie_core::dfa::{Dfa, MAX_CLASSES, MAX_DFA_STATES, MAX_DFA_TRANSITIONS};

use crate::{Capacity, CodecError, MAGIC, VERSION};

/// The size of the fixed header, in bytes.
pub const HEADER_LEN: usize = 16;

/// The size of the checksum trailer, in bytes.
pub const CHECKSUM_LEN: usize = 4;

/// The size of the byte-to-class table, in bytes.
pub const BYTE_CLASS_LEN: usize = 256;

/// The marker that identifies a little-endian artifact.
pub const ENDIANNESS_LITTLE: u8 = 0x01;

/// Flag bit: the automaton was compiled from a pattern that contains a
/// zero-width assertion.
///
/// A bare automaton carries no assertion information, so version 1 of the
/// format always writes `0` here. The bit exists so that a later version can
/// use it without changing the layout.
pub const FLAG_HAS_ASSERTS: u8 = 0x01;

/// Flag bit: a match is only accepted at the end of the input.
///
/// This is how a `$` anchor survives the round trip: an automaton that
/// carries this flag rejects a match that is followed by more bytes, exactly
/// like the pattern it was compiled from.
pub const FLAG_ACCEPT_AT_END: u8 = 0x02;

/// The flag bits this version of the format understands.
const KNOWN_FLAGS: u8 = FLAG_HAS_ASSERTS | FLAG_ACCEPT_AT_END;

/// The fixed part of the header, as stored in an artifact.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Header {
    class_count: u16,
    state_count: u16,
    payload_len: u32,
    flags: u8,
}

impl Header {
    /// The number of byte equivalence classes of the automaton.
    pub fn class_count(&self) -> usize {
        self.class_count as usize
    }

    /// The number of states of the automaton, including the dead and start
    /// states.
    pub fn state_count(&self) -> usize {
        self.state_count as usize
    }

    /// The length of the payload section, in bytes.
    pub fn payload_len(&self) -> usize {
        self.payload_len as usize
    }

    /// The raw flag byte.
    pub fn flags(&self) -> u8 {
        self.flags
    }

    /// Whether the automaton was compiled from a pattern with a zero-width
    /// assertion.
    ///
    /// Always `false` for artifacts written by version 1 of this format.
    pub fn has_asserts(&self) -> bool {
        self.flags & FLAG_HAS_ASSERTS != 0
    }

    /// Whether the automaton only accepts a match at the end of the input.
    ///
    /// This is the `$` anchor of the pattern the automaton was compiled from.
    pub fn accept_at_end(&self) -> bool {
        self.flags & FLAG_ACCEPT_AT_END != 0
    }
}

/// The number of bytes the accept bitmap of a `states`-state automaton needs.
fn bitmap_len(states: usize) -> usize {
    states.div_ceil(8)
}

/// The exact number of bytes [`encode`] writes for `dfa`.
///
/// The value never depends on the content of the automaton beyond its state
/// and class counts, and never changes between builds of this crate.
pub fn encoded_len(dfa: &Dfa) -> usize {
    HEADER_LEN
        + CHECKSUM_LEN
        + BYTE_CLASS_LEN
        + bitmap_len(dfa.state_count())
        + dfa.state_count() * dfa.class_count() * 2
}

/// Writes the artifact for `dfa` into `out` and returns the number of bytes
/// written.
///
/// The number of bytes written is always [`encoded_len`]; the function never
/// writes a partial artifact and never panics. A buffer that is too small is
/// reported as [`CodecError::TooSmall`] and left untouched.
pub fn encode(dfa: &Dfa, out: &mut [u8]) -> Result<usize, CodecError> {
    let needed = encoded_len(dfa);
    if out.len() < needed {
        return Err(CodecError::TooSmall {
            needed,
            actual: out.len(),
        });
    }

    let states = dfa.state_count();
    let classes = dfa.class_count();
    let bitmap = bitmap_len(states);
    let payload_len = BYTE_CLASS_LEN + bitmap + states * classes * 2;

    // Header.
    out[0..4].copy_from_slice(&MAGIC);
    out[4..6].copy_from_slice(&VERSION.to_le_bytes());
    out[6] = ENDIANNESS_LITTLE;
    out[7] = if dfa.accepts_at_end_only() {
        FLAG_ACCEPT_AT_END
    } else {
        0
    };
    out[8..10].copy_from_slice(&(classes as u16).to_le_bytes());
    out[10..12].copy_from_slice(&(states as u16).to_le_bytes());
    out[12..16].copy_from_slice(&(payload_len as u32).to_le_bytes());

    // Byte-to-class table.
    let mut cursor = HEADER_LEN;
    out[cursor..cursor + BYTE_CLASS_LEN].copy_from_slice(dfa.byte_classes());
    cursor += BYTE_CLASS_LEN;

    // Accept bitmap. Only the bits of the first `states` states are kept, so
    // that the artifact is a canonical image of the automaton.
    let accept = dfa.accept_bitmap();
    for (index, &byte) in accept.iter().enumerate() {
        let bits = states.saturating_sub(index * 8).min(8);
        let mask = if bits == 8 { 0xff } else { (1u8 << bits) - 1 };
        out[cursor + index] = byte & mask;
    }
    cursor += bitmap;

    // Transition table, row-major over (state, class).
    for &target in dfa.transitions() {
        out[cursor..cursor + 2].copy_from_slice(&target.to_le_bytes());
        cursor += 2;
    }

    // Checksum over everything written so far.
    let checksum = crc32(&out[..cursor]);
    out[cursor..cursor + CHECKSUM_LEN].copy_from_slice(&checksum.to_le_bytes());

    Ok(cursor + CHECKSUM_LEN)
}

/// Parses an artifact back into an automaton.
///
/// Every malformed artifact is reported as an explicit error; the function
/// never panics, whatever the input is.
pub fn decode(bytes: &[u8]) -> Result<Dfa, CodecError> {
    let header = parse_header(bytes)?;
    let states = header.state_count();
    let classes = header.class_count();
    let bitmap = bitmap_len(states);
    let expected_payload = BYTE_CLASS_LEN + bitmap + states * classes * 2;

    if header.payload_len() != expected_payload {
        return Err(CodecError::LengthMismatch {
            expected: expected_payload,
            actual: header.payload_len(),
        });
    }

    let total = HEADER_LEN + expected_payload + CHECKSUM_LEN;
    if bytes.len() < total {
        return Err(CodecError::Truncated {
            needed: total,
            actual: bytes.len(),
        });
    }
    if bytes.len() > total {
        return Err(CodecError::LengthMismatch {
            expected: total,
            actual: bytes.len(),
        });
    }

    let found = u32::from_le_bytes([
        bytes[total - 4],
        bytes[total - 3],
        bytes[total - 2],
        bytes[total - 1],
    ]);
    let expected = crc32(&bytes[..total - CHECKSUM_LEN]);
    if found != expected {
        return Err(CodecError::BadChecksum { expected, found });
    }

    let payload = &bytes[HEADER_LEN..HEADER_LEN + expected_payload];

    let mut byte_class = [0u8; BYTE_CLASS_LEN];
    byte_class.copy_from_slice(&payload[..BYTE_CLASS_LEN]);
    let accept = &payload[BYTE_CLASS_LEN..BYTE_CLASS_LEN + bitmap];

    let mut transitions = [0u16; MAX_DFA_TRANSITIONS];
    let base = BYTE_CLASS_LEN + bitmap;
    let count = states * classes;
    for (index, slot) in transitions.iter_mut().enumerate().take(count) {
        let offset = base + index * 2;
        *slot = u16::from_le_bytes([payload[offset], payload[offset + 1]]);
    }

    let mut dfa = Dfa::from_parts(
        header.class_count,
        header.state_count,
        &byte_class,
        accept,
        &transitions[..count],
    )
    .map_err(CodecError::from)?;
    // The `$` anchor is a property of the whole automaton, not of a single
    // state, so it travels in the flag byte of the header.
    dfa.set_accept_at_end(header.accept_at_end());
    Ok(dfa)
}

/// Reads and validates the header of an artifact.
///
/// The header is validated completely: magic, version, endianness marker and
/// flag bits are checked, and the state and class counts are rejected when
/// they exceed the capacity of `regie-core`. The payload is not touched.
pub fn parse_header(bytes: &[u8]) -> Result<Header, CodecError> {
    let minimum = HEADER_LEN + CHECKSUM_LEN;
    if bytes.len() < minimum {
        return Err(CodecError::Truncated {
            needed: minimum,
            actual: bytes.len(),
        });
    }

    let magic = [bytes[0], bytes[1], bytes[2], bytes[3]];
    if magic != MAGIC {
        return Err(CodecError::BadMagic { found: magic });
    }

    let version = u16::from_le_bytes([bytes[4], bytes[5]]);
    if version != VERSION {
        return Err(CodecError::UnsupportedVersion { found: version });
    }

    let endianness = bytes[6];
    if endianness != ENDIANNESS_LITTLE {
        return Err(CodecError::BadEndianness { found: endianness });
    }

    let flags = bytes[7];
    if flags & !KNOWN_FLAGS != 0 {
        return Err(CodecError::UnsupportedFlags { found: flags });
    }

    let class_count = u16::from_le_bytes([bytes[8], bytes[9]]);
    let state_count = u16::from_le_bytes([bytes[10], bytes[11]]);
    let payload_len = u32::from_le_bytes([bytes[12], bytes[13], bytes[14], bytes[15]]);

    if class_count == 0 {
        return Err(CodecError::InvalidAutomaton {
            reason: "an automaton needs at least one byte class",
        });
    }
    if class_count as usize > MAX_CLASSES {
        return Err(CodecError::CapacityExceeded {
            kind: Capacity::Classes,
            limit: MAX_CLASSES,
            actual: class_count as usize,
        });
    }
    if state_count == 0 {
        return Err(CodecError::InvalidAutomaton {
            reason: "an automaton needs at least one state",
        });
    }
    if state_count as usize > MAX_DFA_STATES {
        return Err(CodecError::CapacityExceeded {
            kind: Capacity::States,
            limit: MAX_DFA_STATES,
            actual: state_count as usize,
        });
    }

    Ok(Header {
        class_count,
        state_count,
        payload_len,
        flags,
    })
}

/// Checks that an artifact is well formed, without building an automaton.
///
/// The returned automaton of [`decode`] is the only thing this function skips;
/// every validation [`decode`] performs is performed here too.
pub fn verify(bytes: &[u8]) -> Result<(), CodecError> {
    decode(bytes).map(|_| ())
}

/// Encodes an automaton into a freshly allocated buffer.
///
/// The buffer is exactly [`encoded_len`] bytes long.
#[cfg(feature = "alloc")]
pub fn encode_to_vec(dfa: &Dfa) -> Result<alloc::vec::Vec<u8>, CodecError> {
    let mut buffer = alloc::vec![0u8; encoded_len(dfa)];
    let written = encode(dfa, &mut buffer)?;
    buffer.truncate(written);
    Ok(buffer)
}

/// CRC-32 (IEEE 802.3) over `data`.
///
/// This is the function `zlib` exposes as `crc32`: the reflected polynomial
/// `0xEDB88320`, an initial value of `0xFFFFFFFF` and a final XOR of
/// `0xFFFFFFFF`. The check value of the ASCII string `123456789` is
/// `0xCBF43926`.
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

#[cfg(test)]
mod tests {
    use super::crc32;

    #[test]
    fn crc32_matches_the_standard_check_value() {
        assert_eq!(crc32(b"123456789"), 0xcbf4_3926);
        assert_eq!(crc32(b""), 0x0000_0000);
        assert_eq!(crc32(b"a"), 0xe8b7_be43);
    }
}
