//! Encoding an automaton, decoding it back, and corrupting the bytes.
//!
//! Run with:
//!
//! ```text
//! cargo run -p regie-codec --example round_trip
//! ```
//!
//! The program encodes a small automaton, decodes the artifact and compares
//! the two on a list of inputs. It then corrupts the artifact in four ways
//! and prints the exact error the decoder returns for each.

use regie_codec::{decode, encode, encoded_len};
use regie_core::dfa::Dfa;

fn main() {
    let dfa = automaton();
    let mut artifact = vec![0u8; encoded_len(&dfa)];
    must(encode(&dfa, &mut artifact), "cannot encode");
    let restored = must(decode(&artifact), "cannot decode");

    let size = artifact.len();
    let states = dfa.state_count();
    let classes = dfa.class_count();
    println!("artifact: {size} bytes, {states} states, {classes} byte classes");
    println!();

    println!("the decoded automaton has the same parts as the original");
    println!(
        "  states {} == {}",
        dfa.state_count(),
        restored.state_count()
    );
    println!(
        "  classes {} == {}",
        dfa.class_count(),
        restored.class_count()
    );
    let table_ok = dfa.byte_classes() == restored.byte_classes();
    let bitmap_ok = dfa.accept_bitmap() == restored.accept_bitmap();
    let trans_ok = dfa.transitions() == restored.transitions();
    let anchor_ok = dfa.accepts_at_end_only() == restored.accepts_at_end_only();
    println!("  byte table    {table_ok}");
    println!("  accept bitmap {bitmap_ok}");
    println!("  transitions   {trans_ok}");
    println!("  end anchor    {anchor_ok}");
    println!();

    println!("both automata answer the same way");
    println!("  {:<12}{:<11}restored", "input", "original");
    for input in [&b""[..], &b"a"[..], &b"b"[..], &b"ab"[..], &b"c"[..]] {
        let text = String::from_utf8_lossy(input);
        let original = dfa.is_match(input);
        let back = restored.is_match(input);
        println!("  {:<12}{:<11}{}", format!("\"{text}\""), original, back);
    }
    println!();

    println!("corrupting the artifact");
    corrupt("a flipped magic byte", flip(&artifact, 0, 0x01));
    corrupt("a truncated blob", artifact[..size - 1].to_vec());
    corrupt("a flipped payload byte", flip(&artifact, 279, 0x01));
    corrupt("a corrupted checksum byte", flip(&artifact, size - 1, 0x01));
}

/// Shows what the decoder says about a corrupted artifact.
fn corrupt(what: &str, broken: Vec<u8>) {
    println!("  {what} ({} bytes)", broken.len());
    match decode(&broken) {
        Ok(_) => println!("    the decoder accepted the corrupted artifact"),
        Err(error) => {
            println!("    {error:?}");
            println!("    {error}");
        }
    }
}

/// Returns a copy of `bytes` with one bit flipped in the byte at `offset`.
fn flip(bytes: &[u8], offset: usize, mask: u8) -> Vec<u8> {
    let mut broken = bytes.to_vec();
    broken[offset] ^= mask;
    broken
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
