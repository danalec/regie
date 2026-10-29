//! The `match` and `test` subcommands.
//!
//! Both compile a pattern with `regie-core` and run it against an input;
//! they differ only in how they report the verdict.

use crate::render;
use crate::{CliError, EXIT_NO_MATCH, EXIT_OK};
use regie_core::Regex;

/// Runs the `match` subcommand.
///
/// Prints the span of the leftmost match and the span of every capture
/// group. Returns [`EXIT_NO_MATCH`] when the pattern does not match.
pub fn run_match(pattern: &str, input: &str) -> Result<u8, CliError> {
    let regex = compile(pattern)?;

    match regex.captures(input.as_bytes()) {
        Some(captures) => {
            render::print_match(&regex, &captures, input.as_bytes());
            Ok(EXIT_OK)
        }
        None => {
            println!("no match");
            Ok(EXIT_NO_MATCH)
        }
    }
}

/// Runs the `test` subcommand.
///
/// Prints `match` or `no match`, and returns [`EXIT_OK`] or
/// [`EXIT_NO_MATCH`].
pub fn run_test(pattern: &str, input: &str) -> Result<u8, CliError> {
    let regex = compile(pattern)?;

    if regex.is_match(input.as_bytes()) {
        println!("match");
        Ok(EXIT_OK)
    } else {
        println!("no match");
        Ok(EXIT_NO_MATCH)
    }
}

/// Compiles a pattern, reporting a rejected pattern as an error.
fn compile(pattern: &str) -> Result<Regex, CliError> {
    Regex::new(pattern).map_err(|source| CliError::Pattern {
        pattern: pattern.to_string(),
        source,
    })
}
