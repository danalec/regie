//! The byte layout of the `RGDF` artifact, field by field.
//!
//! Run with:
//!
//! ```text
//! cargo run -p regie-codec --example artifact_layout
//! ```
//!
//! The program encodes a three-state automaton, dumps the real bytes of the
//! artifact and reads every field back out of them, the way a decoder would.
//! The CRC-32 is recomputed and compared with the checksum the artifact
//! carries.

use regie_codec::{
    decode, encode, encoded_len, BYTE_CLASS_LEN, FLAG_ACCEPT_AT_END, FLAG_HAS_ASSERTS, HEADER_LEN,
};
use regie_core::dfa::Dfa;

fn main() {
    let dfa = automaton();
    let mut artifact = vec![0u8; encoded_len(&dfa)];
    must(encode(&dfa, &mut artifact), "cannot encode");

    let states = dfa.state_count();
    let classes = dfa.class_count();
    let class_at = HEADER_LEN;
    let bitmap_at = class_at + BYTE_CLASS_LEN;
    let trans_at = bitmap_at + states.div_ceil(8);
    let sum_at = trans_at + states * classes * 2;
    let size = artifact.len();

    println!("artifact: {size} bytes, {states} states, {classes} byte classes");
    println!();
    println!("hex dump");
    for (index, chunk) in artifact.chunks(16).enumerate() {
        println!("  {:04x}  {}", index * 16, hex(chunk));
    }
    println!();

    let version = u16::from_le_bytes([artifact[4], artifact[5]]);
    let endianness = artifact[6];
    let flags = artifact[7];
    let class_count = u16::from_le_bytes([artifact[8], artifact[9]]);
    let state_count = u16::from_le_bytes([artifact[10], artifact[11]]);
    let payload = u32::from_le_bytes([artifact[12], artifact[13], artifact[14], artifact[15]]);
    let stored = u32::from_le_bytes([
        artifact[sum_at],
        artifact[sum_at + 1],
        artifact[sum_at + 2],
        artifact[sum_at + 3],
    ]);

    println!("every field, read out of the bytes above");
    let fields = [
        ("magic", 0, 4, "\"RGDF\""),
        ("version", 4, 6, ""),
        ("endianness", 6, 7, ""),
        ("flags", 7, 8, ""),
        ("class_count", 8, 10, ""),
        ("state_count", 10, 12, ""),
        ("payload_len", 12, 16, ""),
        ("byte_class", class_at, bitmap_at, "class per byte value"),
        ("accept_bitmap", bitmap_at, trans_at, "state 2 accepts"),
        ("transitions", trans_at, sum_at, "3 states x 2 classes"),
        ("checksum", sum_at, size, ""),
    ];
    for (name, start, end, text) in fields {
        let bytes = if end - start > 8 {
            format!("{} bytes", end - start)
        } else {
            hex(&artifact[start..end])
        };
        let value = match name {
            "version" => version.to_string(),
            "endianness" => format!("little-endian ({endianness:#04x})"),
            "flags" => flag_text(flags).to_string(),
            "class_count" => class_count.to_string(),
            "state_count" => state_count.to_string(),
            "payload_len" => format!("{payload} bytes"),
            "checksum" => format!("{stored:#010x}"),
            _ => text.to_string(),
        };
        println!("  {name:<14}[{start:>3}..{end:<4}]{bytes:<26}{value}");
    }
    println!();

    println!("the transition table, row major over (state, class)");
    for state in 0..states {
        print!("  state {state}:");
        for class in 0..classes {
            let offset = trans_at + (state * classes + class) * 2;
            let bytes = [artifact[offset], artifact[offset + 1]];
            print!("  {}", u16::from_le_bytes(bytes));
        }
        println!();
    }
    println!();

    let computed = crc32(&artifact[..sum_at]);
    println!("checksum");
    println!(
        "  stored    [285..289]  {}  {stored:#010x}",
        hex(&artifact[sum_at..])
    );
    println!("  computed  over bytes [0..{sum_at}]  {computed:#010x}");
    println!("  the two agree: {}", stored == computed);
    println!();

    let mut anchored = dfa;
    anchored.set_accept_at_end(true);
    let mut anchored_artifact = vec![0u8; encoded_len(&anchored)];
    must(encode(&anchored, &mut anchored_artifact), "cannot encode");
    let flag_bytes = hex(&anchored_artifact[7..8]);
    println!("the same automaton with the end anchor set");
    println!("  flags [7..8] {flag_bytes} accept_at_end");
    println!(
        "  {FLAG_HAS_ASSERTS:#04x} is reserved, {FLAG_ACCEPT_AT_END:#04x} accepts at the end only"
    );
    println!();

    let restored = must(decode(&artifact), "cannot decode");
    println!("decode agrees: {} states", restored.state_count());
}

/// Builds the automaton the example encodes: state 0 is dead, state 1 is the
/// start state, state 2 accepts, and byte class 1 is `a` or `b`.
fn automaton() -> Dfa {
    let mut byte_class = [0u8; 256];
    byte_class[b'a' as usize] = 1;
    byte_class[b'b' as usize] = 1;
    let accept = [0b0000_0100u8]; // one bit per state, bit 2 is state 2
    let transitions = [0u16, 0, 0, 2, 0, 0]; // one row per state, one column per class
    let parts = Dfa::from_parts(2, 3, &byte_class, &accept, &transitions);
    must(parts, "bad automaton")
}

/// Names the bits that are set in the flag byte.
fn flag_text(flags: u8) -> &'static str {
    match flags {
        0 => "no flag set",
        FLAG_HAS_ASSERTS => "has_asserts",
        FLAG_ACCEPT_AT_END => "accept_at_end",
        _ => "other bits",
    }
}

/// Stops the program when a call fails, after printing the error.
fn must<T, E: std::fmt::Display>(result: Result<T, E>, what: &str) -> T {
    match result {
        Ok(value) => value,
        Err(error) => {
            eprintln!("{what}: {error}");
            std::process::exit(1);
        }
    }
}

/// Formats a byte string as lowercase hexadecimal.
fn hex(bytes: &[u8]) -> String {
    let mut out = String::new();
    for byte in bytes {
        out.push_str(&format!("{byte:02x} "));
    }
    out.trim_end().to_string()
}

/// CRC-32 (IEEE 802.3), the same function `zlib` calls `crc32`.
///
/// The codec keeps its own copy private, so the example carries its own to
/// show that the stored checksum can be recomputed by anyone who has the
/// bytes.
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
