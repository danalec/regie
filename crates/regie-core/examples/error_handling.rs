//! The error type as a value: one line of the program per variant of
//! `RegexError`, with the reaction a caller can attach to it.
//!
//! Run with:
//!
//! ```text
//! cargo run -p regie-core --example error_handling
//! ```

use regie_core::{Regex, RegexError};

fn main() {
    let errors = [
        RegexError::PatternTooLong {
            limit: 512,
            actual: 513,
        },
        RegexError::Syntax {
            position: 4,
            message: "unbalanced ')'",
        },
        RegexError::Unsupported {
            position: 2,
            feature: "look-ahead",
        },
        RegexError::ProgramTooLarge { limit: 512 },
        RegexError::TooManyGroups { limit: 8 },
        RegexError::RepetitionTooLarge { limit: 255 },
        RegexError::NestingTooDeep { limit: 32 },
        RegexError::InvalidClass {
            position: 0,
            message: "character range is reversed",
        },
        RegexError::DfaTooLarge { limit: 256 },
        RegexError::TooManyClasses { limit: 24 },
        RegexError::ThreadLimitExceeded { limit: 256 },
    ];

    println!("{:<22} {:<26} MESSAGE", "VARIANT", "REACTION");
    for err in &errors {
        react(err);
    }

    from_the_engine();
}

/// The reaction of a caller to each variant.
///
/// The enum is not exhaustive, so the last arm is the one that covers a
/// variant added later: it keeps the caller compiling without hiding the
/// case.
fn react(err: &RegexError) {
    let (variant, reaction) = match err {
        RegexError::PatternTooLong { .. } => ("PatternTooLong", "shorten the pattern".to_owned()),
        RegexError::Syntax { position, .. } => {
            ("Syntax", format!("fix the pattern at byte {position}"))
        }
        RegexError::Unsupported { feature, .. } => ("Unsupported", format!("do not use {feature}")),
        RegexError::ProgramTooLarge { .. } => {
            ("ProgramTooLarge", "use a smaller pattern".to_owned())
        }
        RegexError::TooManyGroups { limit } => {
            ("TooManyGroups", format!("use fewer than {limit} groups"))
        }
        RegexError::RepetitionTooLarge { limit } => {
            ("RepetitionTooLarge", format!("lower the bound to {limit}"))
        }
        RegexError::NestingTooDeep { limit } => {
            ("NestingTooDeep", format!("nest less than {limit} levels"))
        }
        RegexError::InvalidClass { position, .. } => {
            ("InvalidClass", format!("fix the class at byte {position}"))
        }
        RegexError::DfaTooLarge { .. } => ("DfaTooLarge", "keep the Pike VM".to_owned()),
        RegexError::TooManyClasses { limit } => {
            ("TooManyClasses", format!("use fewer than {limit} classes"))
        }
        RegexError::ThreadLimitExceeded { limit } => (
            "ThreadLimitExceeded",
            format!("use fewer than {limit} threads"),
        ),
        other => ("<new variant>", format!("handle it here ({other})")),
    };
    println!("{variant:<22} {reaction:<26} {err}");
}

/// An error as the engine returns it, rather than one built by hand.
fn from_the_engine() {
    println!();
    println!("an error as the engine returns it:");
    match Regex::new("(?=ahead)") {
        Ok(_) => println!("the pattern was accepted"),
        Err(err) => react(&err),
    }
}
