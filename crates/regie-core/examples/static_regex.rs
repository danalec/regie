//! Compiling and matching patterns with `regie-core`.
//!
//! Run with:
//!
//! ```text
//! cargo run -p regie-core --example static_regex
//! ```
//!
//! The example shows the three ways to get a matcher out of the crate:
//! compiling a pattern, compiling a structured specification, and asking the
//! intent model for a specification (see the `regie-intent` example).

use regie_core::spec::{CandidateSpec, CharsetPolicy, Strictness, ValidationKind};
use regie_core::{Regex, RegexError};

fn main() {
    literal_matching();
    capture_groups();
    structured_specs();
    explicit_errors();
}

/// Compile a pattern and ask whether it matches.
fn literal_matching() {
    let re = match Regex::new(r"^\d{3}-\d{4}$") {
        Ok(re) => re,
        Err(err) => {
            eprintln!("could not compile the pattern: {err}");
            return;
        }
    };

    println!("pattern: {}", re.pattern());
    println!("matches \"555-1234\":  {}", re.is_match(b"555-1234"));
    println!("matches \"5551234\":   {}", re.is_match(b"5551234"));
    println!("matches \"x555-1234\": {}", re.is_match(b"x555-1234"));
}

/// Capture groups report the span of every group that took part.
fn capture_groups() {
    let re = match Regex::new(r"(?i)(\w+)@(\w+)\.com") {
        Ok(re) => re,
        Err(err) => {
            eprintln!("could not compile the pattern: {err}");
            return;
        }
    };

    let subject = b"write to ana@corp.com today";
    match re.captures(subject) {
        Some(caps) => {
            println!(
                "whole match: {:?} ({} groups)",
                caps.get(0),
                re.group_count()
            );
            for index in 1..=re.group_count() {
                let group = caps.get(index);
                let text = match group {
                    Some(span) => {
                        String::from_utf8_lossy(&subject[span.start..span.end]).into_owned()
                    }
                    None => "<no match>".to_owned(),
                };
                println!("  group {index}: {text}");
            }
        }
        None => println!("no match"),
    }
}

/// A specification carries no pattern text at all.
///
/// This is the path the intent model takes: the model can only select one of
/// the enumerated options, so it can never smuggle arbitrary syntax into the
/// engine.
fn structured_specs() {
    let spec = CandidateSpec::Validate {
        kind: ValidationKind::Email,
        strictness: Strictness::Standard,
        charset: CharsetPolicy::Any,
        max_len: None,
    };

    let re = match Regex::from_spec(&spec) {
        Ok(re) => re,
        Err(err) => {
            eprintln!("could not compile the specification: {err}");
            return;
        }
    };

    println!("spec family: {}", spec.kind_name());
    println!("matches \"ana@corp.com\": {}", re.is_match(b"ana@corp.com"));
    println!(
        "matches \"not-an-email\":  {}",
        re.is_match(b"not-an-email")
    );
}

/// Every failure is an explicit value, never a panic.
fn explicit_errors() {
    for pattern in [r"(?=ahead)", r"back\1", r"[z-a]", r"a{3,2}"] {
        match Regex::new(pattern) {
            Ok(_) => println!("{pattern:<10} compiled"),
            Err(err) => println!("{pattern:<10} rejected: {err}"),
        }
    }

    // The error type is a plain enum, so it can be matched on.
    let err = Regex::new(r"(?=ahead)").unwrap_err();
    match err {
        RegexError::Unsupported { feature, .. } => {
            println!("the engine refuses {feature} on purpose");
        }
        other => println!("unexpected error: {other}"),
    }
}
