//! Writes a compiled automaton to a file, the way a firmware build would.
//!
//! Run with:
//!
//! ```text
//! cargo run -p regie-codec --features alloc --example emit_artifact
//! ```
//!
//! The artifact it produces is checked into the repository, so that the
//! `precompiled_dfa` example - and any firmware that uses it - can embed the
//! automaton with `include_bytes!` and never compile anything at boot.

use std::fs;
use std::path::PathBuf;

use regie_codec::encode;
use regie_core::Regex;

/// The pattern the artifact is built from.
const PATTERN: &str = r"^ana@corp\.com$";

fn main() {
    let regex = match Regex::new(PATTERN) {
        Ok(regex) => regex,
        Err(error) => {
            eprintln!("could not compile the pattern: {error}");
            std::process::exit(1);
        }
    };

    let dfa = match regex.build_dfa() {
        Ok(dfa) => dfa,
        Err(error) => {
            eprintln!("could not build the automaton: {error}");
            std::process::exit(1);
        }
    };

    let mut artifact = vec![0u8; regie_codec::encoded_len(&dfa)];
    if let Err(error) = encode(&dfa, &mut artifact) {
        eprintln!("could not encode the automaton: {error}");
        std::process::exit(1);
    }

    let path = artifact_path();
    if let Some(parent) = path.parent() {
        if let Err(error) = fs::create_dir_all(parent) {
            eprintln!("could not create {}: {error}", parent.display());
            std::process::exit(1);
        }
    }
    if let Err(error) = fs::write(&path, &artifact) {
        eprintln!("could not write {}: {error}", path.display());
        std::process::exit(1);
    }

    println!(
        "wrote {} bytes to {} ({} states, {} byte classes)",
        artifact.len(),
        path.display(),
        dfa.state_count(),
        dfa.class_count()
    );
}

/// Where the artifact lives.
fn artifact_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("examples")
        .join("assets")
        .join("ana_at_corp.rdf")
}
