//! Matching against an automaton that was compiled ahead of time.
//!
//! Run with:
//!
//! ```text
//! cargo run -p regie-codec --example precompiled_dfa
//! ```
//!
//! This is the firmware path. The automaton was compiled on a workstation, by
//! the `emit_artifact` example, and is linked into the image as a `static`
//! blob. At run time the target does not compile anything: it decodes the
//! blob and matches against it.
//!
//! The decode path needs no allocator, which is why the artifact is embedded
//! with `include_bytes!` and handed to [`regie_codec::decode`] as a slice.

use regie_codec::decode;

/// The compiled automaton, linked into the binary.
const ARTIFACT: &[u8] = include_bytes!("assets/ana_at_corp.rdf");

fn main() {
    let dfa = match decode(ARTIFACT) {
        Ok(dfa) => dfa,
        Err(error) => {
            eprintln!("could not decode the artifact: {error}");
            std::process::exit(1);
        }
    };

    println!(
        "artifact: {} bytes, {} states, {} byte classes",
        ARTIFACT.len(),
        dfa.state_count(),
        dfa.class_count()
    );
    println!("accepts at the end only: {}", dfa.accepts_at_end_only());

    for candidate in [
        &b"ana@corp.com"[..],
        &b"mail ana@corp.com here"[..],
        &b"ana@other.com"[..],
        &b"ana@corp.com.br"[..],
        &b"nobody here"[..],
    ] {
        let text = String::from_utf8_lossy(candidate);
        println!("  {text:<24} -> {}", dfa.is_match(candidate));
    }

    // The end anchor survived the round trip: a match may not be followed by
    // more bytes.
    assert!(dfa.is_match(b"ana@corp.com"));
    assert!(!dfa.is_match(b"ana@corp.com.br"));
    assert!(dfa.accepts_at_end_only());
}
