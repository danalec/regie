//! The catalogue of syntax the engine refuses, with the exact message it
//! returns for each.
//!
//! Run with:
//!
//! ```text
//! cargo run -p regie-core --example refusals
//! ```

use regie_core::{Regex, RegexError};

/// One refused construct: what it is called, and the pattern that spells it.
const ROWS: [(&str, &str); 20] = [
    ("look-ahead", "(?=ahead)"),
    ("negative look-ahead", "(?!ahead)"),
    ("look-behind", "(?<=behind)"),
    ("negative look-behind", "(?<!behind)"),
    ("backreference", r"(a)\1"),
    ("named group", "(?P<name>x)"),
    ("named group, angle brackets", "(?<name>x)"),
    ("recursion", "(?R)"),
    ("conditional", "(?(1)a|b)"),
    ("possessive quantifier", "a*+"),
    ("unknown escape", r"\q"),
    ("reversed range", "[z-a]"),
    ("reversed bounds", "a{3,2}"),
    ("unclosed group", "(abc"),
    ("stray parenthesis", "abc)"),
    ("stray quantifier", "*abc"),
    ("comment group", "(?#text)"),
    ("unicode escape", r"\u0041"),
    ("unicode property", r"\p{L}"),
    ("unknown flag", "(?x)"),
];

fn main() {
    println!(
        "{:<32} {:<14} {:<18} MESSAGE",
        "CONSTRUCT", "PATTERN", "VARIANT"
    );
    for (construct, pattern) in ROWS {
        match Regex::new(pattern) {
            Ok(_) => println!(
                "{:<32} {:<14} {:<18} the pattern was accepted",
                construct, pattern, "<compiled>"
            ),
            Err(err) => println!(
                "{:<32} {:<14} {:<18} {}",
                construct,
                pattern,
                variant(&err),
                err
            ),
        }
    }
}

/// The name of the variant of an error.
///
/// The enum is not exhaustive, so the last arm is the one that covers a
/// variant added later.
fn variant(err: &RegexError) -> &'static str {
    match err {
        RegexError::PatternTooLong { .. } => "PatternTooLong",
        RegexError::Syntax { .. } => "Syntax",
        RegexError::Unsupported { .. } => "Unsupported",
        RegexError::ProgramTooLarge { .. } => "ProgramTooLarge",
        RegexError::TooManyGroups { .. } => "TooManyGroups",
        RegexError::RepetitionTooLarge { .. } => "RepetitionTooLarge",
        RegexError::NestingTooDeep { .. } => "NestingTooDeep",
        RegexError::InvalidClass { .. } => "InvalidClass",
        RegexError::DfaTooLarge { .. } => "DfaTooLarge",
        RegexError::TooManyClasses { .. } => "TooManyClasses",
        RegexError::ThreadLimitExceeded { .. } => "ThreadLimitExceeded",
        _ => "<unknown>",
    }
}
