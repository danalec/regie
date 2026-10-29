//! # regie-codec
//!
//! Serialization of compiled [`regie_core::dfa::Dfa`] automata into a compact,
//! self-describing binary artifact.
//!
//! The point of this crate is firmware: a pattern is compiled once, on a
//! workstation, and the resulting automaton is linked into the image as a
//! `static` blob. At boot the target does **not** compile anything, it only
//! decodes the blob and matches against it.
//!
//! ```
//! use regie_core::dfa::Dfa;
//! use regie_codec::{decode, encode, encoded_len};
//!
//! // Two byte classes: class 0 is "anything else", class 1 is `a` or `b`.
//! let mut byte_class = [0u8; 256];
//! byte_class[b'a' as usize] = 1;
//! byte_class[b'b' as usize] = 1;
//!
//! // Three states: 0 = dead, 1 = start, 2 = accepting.
//! let dfa = Dfa::from_parts(
//!     2,
//!     3,
//!     &byte_class,
//!     &[0b0000_0100],
//!     &[0, 0, 0, 2, 0, 0],
//! )
//! .unwrap();
//!
//! let mut artifact = vec![0u8; encoded_len(&dfa)];
//! let written = encode(&dfa, &mut artifact).unwrap();
//! assert_eq!(written, artifact.len());
//!
//! let restored = decode(&artifact).unwrap();
//! assert!(restored.is_match(b"a"));
//! assert!(!restored.is_match(b"c"));
//! ```
//!
//! ## The `RGDF` artifact format
//!
//! Every artifact starts with a fixed 16-byte header, followed by a payload and
//! a 4-byte checksum. **All multi-byte integers are little-endian**, whatever
//! the endianness of the host that produced or consumed the artifact.
//!
//! | Offset | Size | Field | Description |
//! |---|---|---|---|
//! | `0` | 4 | magic | Always `b"RGDF"`. |
//! | `4` | 2 | version | Format version, `u16`. Currently [`VERSION`] (`1`). |
//! | `6` | 1 | endianness | Integer encoding marker. `0x01` means little-endian. |
//! | `7` | 1 | flags | Bit field, see below. |
//! | `8` | 2 | class_count | Number of byte equivalence classes, `u16`. |
//! | `10` | 2 | state_count | Number of states (including dead and start), `u16`. |
//! | `12` | 4 | payload_len | Length of the payload section in bytes, `u32`. |
//! | `16` | 256 | byte_class | The byte-to-class table: one class per byte value. |
//! | `272` | `ceil(state_count / 8)` | accept_bitmap | One bit per state, least significant bit first. |
//! | `272 + bitmap` | `2 * state_count * class_count` | transitions | Transition targets, `u16` each, row-major over `(state, class)`. |
//! | `272 + bitmap + transitions` | 4 | checksum | CRC-32 (IEEE) of every preceding byte, `u16`-free, `u32`. |
//!
//! The total length of an artifact is therefore
//!
//! ```text
//! 20 + 256 + ceil(state_count / 8) + 2 * state_count * class_count
//! ```
//!
//! which is exactly what [`encoded_len`] returns and exactly the number of
//! bytes [`encode`] writes. There is no padding anywhere.
//!
//! ### Flags
//!
//! | Bit | Name | Meaning |
//! |---|---|---|
//! | `0` | [`FLAG_HAS_ASSERTS`] | Reserved. A bare automaton carries no assertion information, so version 1 always writes `0` here. |
//! | `1` | [`FLAG_ACCEPT_AT_END`] | Set when the automaton only accepts a match at the end of the input. This is how a `$` anchor survives the round trip. |
//! | `2..7` | — | Reserved. A decoder rejects an artifact that sets any of them. |
//!
//! ### Checksum
//!
//! The checksum is CRC-32 (IEEE 802.3, the polynomial `0xEDB88320` in its
//! reflected form, initial value `0xFFFFFFFF`, final XOR `0xFFFFFFFF` — the
//! same function `zlib` calls `crc32`). It is computed over every byte of the
//! artifact that precedes the checksum field itself, so it covers the header as
//! well as the payload.
//!
//! ### Guarantees
//!
//! * **Deterministic.** The same automaton always encodes to exactly the same
//!   bytes; there is no timestamp, no padding and no host-dependent ordering.
//! * **Strict.** A decoder rejects a truncated artifact, an artifact with a
//!   trailing byte, a wrong magic, an unknown version, unknown flag bits, an
//!   automaton larger than `regie-core` supports, and any payload whose
//!   checksum does not match. No malformed input can panic.
//! * **Allocator free.** The default feature set uses no allocator at all.
//!   [`decode`] needs roughly 33 KiB of stack for the largest automaton this
//!   build supports (the transition buffer plus the automaton itself).
//!
//! ## Features
//!
//! * `alloc` — enables [`encode_to_vec`], which returns an owned buffer.
//! * `std` — implements [`std::error::Error`] for [`CodecError`]. Implies
//!   `alloc`.

#![cfg_attr(not(feature = "std"), no_std)]
#![doc(html_root_url = "https://docs.rs/regie-codec/0.1.0")]
#![forbid(unsafe_code)]
#![deny(missing_docs)]

#[cfg(feature = "alloc")]
extern crate alloc;

mod format;

pub use crate::format::{
    decode, encode, encoded_len, parse_header, verify, Header, BYTE_CLASS_LEN, CHECKSUM_LEN,
    ENDIANNESS_LITTLE, FLAG_ACCEPT_AT_END, FLAG_HAS_ASSERTS, HEADER_LEN,
};

#[cfg(feature = "alloc")]
pub use crate::format::encode_to_vec;

use core::fmt;

use regie_core::RegexError;

/// The magic bytes that start every artifact: `b"RGDF"`.
pub const MAGIC: [u8; 4] = *b"RGDF";

/// The version of the artifact format produced by this build.
///
/// A decoder rejects an artifact whose version differs from this value; see
/// [`CodecError::UnsupportedVersion`].
pub const VERSION: u16 = 1;

/// The capacity limit that an artifact exceeded.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Capacity {
    /// The automaton declares more states than `regie-core` supports.
    States,
    /// The automaton declares more byte equivalence classes than `regie-core`
    /// supports.
    Classes,
}

/// Everything that can go wrong while encoding or decoding an artifact.
///
/// The variants are explicit so that an embedded caller can react to a
/// truncated blob differently than to a corrupt one, without parsing a
/// message.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[non_exhaustive]
pub enum CodecError {
    /// The output buffer is too small to hold the artifact.
    TooSmall {
        /// The number of bytes the artifact needs.
        needed: usize,
        /// The number of bytes the output buffer holds.
        actual: usize,
    },
    /// The artifact does not start with [`MAGIC`].
    BadMagic {
        /// The four bytes that were found instead of the magic.
        found: [u8; 4],
    },
    /// The artifact was produced by a different format version.
    UnsupportedVersion {
        /// The version found in the artifact.
        found: u16,
    },
    /// The endianness marker is not the one this build writes.
    BadEndianness {
        /// The marker found in the artifact.
        found: u8,
    },
    /// The artifact sets flag bits that this version does not define.
    UnsupportedFlags {
        /// The flag byte found in the artifact.
        found: u8,
    },
    /// The artifact ends before a field that the header promises.
    Truncated {
        /// The number of bytes the artifact needs.
        needed: usize,
        /// The number of bytes the artifact holds.
        actual: usize,
    },
    /// The lengths in the header disagree with the size of the artifact.
    LengthMismatch {
        /// The length the header implies.
        expected: usize,
        /// The length that was found.
        actual: usize,
    },
    /// The checksum does not match the content of the artifact.
    BadChecksum {
        /// The checksum computed over the artifact.
        expected: u32,
        /// The checksum stored in the artifact.
        found: u32,
    },
    /// The automaton does not fit the fixed capacity of `regie-core`.
    CapacityExceeded {
        /// Which limit was exceeded.
        kind: Capacity,
        /// The limit of this build.
        limit: usize,
        /// The value the artifact claimed.
        actual: usize,
    },
    /// The header describes an automaton that cannot exist.
    InvalidAutomaton {
        /// A short, static description of the problem.
        reason: &'static str,
    },
    /// `regie-core` rejected the decoded parts.
    Core(RegexError),
}

impl fmt::Display for CodecError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CodecError::TooSmall { needed, actual } => {
                write!(
                    f,
                    "output buffer is too small: {actual} bytes available, {needed} needed"
                )
            }
            CodecError::BadMagic { found } => write!(
                f,
                "bad magic: expected `RGDF`, found {:02x?}{:02x?}{:02x?}{:02x?}",
                found[0], found[1], found[2], found[3]
            ),
            CodecError::UnsupportedVersion { found } => {
                write!(
                    f,
                    "unsupported artifact version {found}, this build reads version {VERSION}"
                )
            }
            CodecError::BadEndianness { found } => {
                write!(
                    f,
                    "unsupported endianness marker {found:#04x}, expected {ENDIANNESS_LITTLE:#04x}"
                )
            }
            CodecError::UnsupportedFlags { found } => {
                write!(f, "unsupported flag bits {found:#04x}")
            }
            CodecError::Truncated { needed, actual } => {
                write!(
                    f,
                    "artifact is truncated: {actual} bytes available, {needed} needed"
                )
            }
            CodecError::LengthMismatch { expected, actual } => {
                write!(
                    f,
                    "artifact length mismatch: expected {expected} bytes, found {actual}"
                )
            }
            CodecError::BadChecksum { expected, found } => {
                write!(
                    f,
                    "bad checksum: computed {expected:#010x}, artifact says {found:#010x}"
                )
            }
            CodecError::CapacityExceeded {
                kind,
                limit,
                actual,
            } => match kind {
                Capacity::States => {
                    write!(f, "automaton has {actual} states, the limit is {limit}")
                }
                Capacity::Classes => {
                    write!(
                        f,
                        "automaton has {actual} byte classes, the limit is {limit}"
                    )
                }
            },
            CodecError::InvalidAutomaton { reason } => write!(f, "invalid automaton: {reason}"),
            CodecError::Core(error) => write!(f, "core rejected the decoded automaton: {error}"),
        }
    }
}

impl From<RegexError> for CodecError {
    fn from(error: RegexError) -> Self {
        CodecError::Core(error)
    }
}

#[cfg(feature = "std")]
impl std::error::Error for CodecError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            CodecError::Core(error) => Some(error),
            _ => None,
        }
    }
}
