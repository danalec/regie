//! The `dfa` and `decode` subcommands.
//!
//! `dfa` compiles a pattern into a deterministic automaton and writes it to a
//! file as a `regie-codec` artifact; `decode` reads such an artifact back and
//! matches an input against it. Together they are the workstation side of the
//! firmware workflow: compile once on a host, ship the bytes, match on the
//! target.

use crate::{CliError, EXIT_NO_MATCH, EXIT_OK};
use regie_core::Regex;

/// Runs the `dfa` subcommand.
///
/// Compiles the pattern, builds its deterministic automaton, encodes it and
/// writes the artifact to `output`.
pub fn run_dfa(pattern: &str, output: &str) -> Result<u8, CliError> {
    let regex = Regex::new(pattern).map_err(|source| CliError::Pattern {
        pattern: pattern.to_string(),
        source,
    })?;
    let dfa = regex.build_dfa().map_err(CliError::Spec)?;

    let mut artifact = vec![0u8; regie_codec::encoded_len(&dfa)];
    let written = regie_codec::encode(&dfa, &mut artifact).map_err(CliError::Codec)?;
    artifact.truncate(written);

    std::fs::write(output, &artifact).map_err(|source| CliError::Io {
        path: output.to_string(),
        source,
    })?;

    println!(
        "wrote {written} bytes to {output} ({} states, {} byte classes)",
        dfa.state_count(),
        dfa.class_count()
    );
    Ok(EXIT_OK)
}

/// Runs the `decode` subcommand.
///
/// Reads the artifact, decodes it and matches the input against the decoded
/// automaton. The automaton carries no capture groups, so only the verdict is
/// reported.
pub fn run_decode(artifact: &str, input: &str) -> Result<u8, CliError> {
    let bytes = std::fs::read(artifact).map_err(|source| CliError::Io {
        path: artifact.to_string(),
        source,
    })?;
    let dfa = regie_codec::decode(&bytes).map_err(CliError::Codec)?;

    println!(
        "artifact: {} bytes, {} states, {} byte classes",
        bytes.len(),
        dfa.state_count(),
        dfa.class_count()
    );

    if dfa.is_match(input.as_bytes()) {
        println!("match");
        Ok(EXIT_OK)
    } else {
        println!("no match");
        Ok(EXIT_NO_MATCH)
    }
}
