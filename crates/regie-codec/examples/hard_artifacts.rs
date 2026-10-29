//! A hard pattern, encoded into an artifact, decoded back and compared.
//!
//! Run with:
//!
//! ```text
//! cargo run -p regie-codec --example hard_artifacts --features alloc
//! ```
//!
//! The pattern is an alternation of an address and a phone number, so the
//! automaton has many more states and byte classes than the toy examples.
//! The program compiles it, encodes the automaton, decodes the artifact back
//! and asks both automata the same questions. It then corrupts the artifact
//! in four ways and prints the exact error the decoder returns for each.
//!
//! What the format guarantees: the artifact carries the same automaton, byte
//! for byte, and the CRC-32 it ends with catches a corrupted image before it
//! can be linked into a firmware.

#[cfg(feature = "alloc")]
fn main() {
    demo::run();
}

#[cfg(not(feature = "alloc"))]
fn main() {
    println!("this example needs the `alloc` feature:");
    println!("cargo run -p regie-codec --example hard_artifacts --features alloc");
}

#[cfg(feature = "alloc")]
mod demo {
    use regie_codec::{decode, encode, encoded_len, BYTE_CLASS_LEN, HEADER_LEN};
    use regie_core::Regex;

    /// The pattern the artifact is built from.
    const PATTERN: &str = r"(?:[a-z]+@[a-z]+\.[a-z]{2,3}|[0-9]{3}-[0-9]{4})";

    /// The subjects both automata are asked about.
    const SUBJECTS: [&str; 10] = [
        "ana@corp.com",
        "ana@corp.io",
        "ana@corp",
        "ana@corp.comm",
        "ANA@CORP.COM",
        "555-1234",
        "5551234",
        "555-12345",
        "555-123",
        "nobody here",
    ];

    /// Compiles the pattern, encodes it, decodes it back and compares.
    pub fn run() {
        let regex = must(Regex::new(PATTERN), "cannot compile the pattern");
        let dfa = must(regex.build_dfa(), "cannot build the automaton");
        let mut artifact = vec![0u8; encoded_len(&dfa)];
        must(encode(&dfa, &mut artifact), "cannot encode the automaton");
        let restored = must(decode(&artifact), "cannot decode the artifact");

        println!("pattern: {PATTERN}");
        println!(
            "automaton: {} states, {} byte classes",
            dfa.state_count(),
            dfa.class_count()
        );
        println!("artifact: {} bytes", artifact.len());
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
        println!(
            "  byte table    {}",
            dfa.byte_classes() == restored.byte_classes()
        );
        println!(
            "  accept bitmap {}",
            dfa.accept_bitmap() == restored.accept_bitmap()
        );
        println!(
            "  transitions   {}",
            dfa.transitions() == restored.transitions()
        );
        println!(
            "  end anchor    {}",
            dfa.accepts_at_end_only() == restored.accepts_at_end_only()
        );
        println!();

        println!("both automata answer the same way");
        println!(
            "  {:<20}{:<10}{:<10}verdict",
            "subject", "original", "decoded"
        );
        for subject in SUBJECTS {
            let original = dfa.is_match(subject.as_bytes());
            let decoded = restored.is_match(subject.as_bytes());
            let same = original == decoded;
            let verdict = if same { "agree" } else { "DIFFER" };
            println!(
                "  {:<20}{:<10}{:<10}{}",
                format!("{subject:?}"),
                original,
                decoded,
                verdict
            );
        }
        println!();

        println!("the artifact is the same automaton, byte for byte");
        println!("  the CRC-32 catches a corrupted image before it is linked into firmware");
        println!();

        println!("corrupting the artifact");
        let size = artifact.len();
        corrupt("a flipped magic byte", flip(&artifact, 0, 0x01));
        corrupt("a truncated blob", artifact[..size - 1].to_vec());
        corrupt(
            "a flipped payload byte",
            flip(&artifact, HEADER_LEN + BYTE_CLASS_LEN / 2, 0x01),
        );
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
}
