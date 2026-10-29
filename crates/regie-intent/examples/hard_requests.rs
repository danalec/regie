//! Requests that sit near the boundary between two families.
//!
//! Run with:
//!
//! ```text
//! cargo run -p regie-intent --example hard_requests
//! ```
//!
//! The model chooses one value out of a closed enumeration of families, so a
//! request that sits near a boundary is answered with a specification that is
//! valid but not always the one the request meant: "extract a date in ISO
//! format" is answered with a *validation* of an ISO date, not with an
//! extraction, and "validate a NIF" with the Portuguese NIF.  The answer is
//! never wrong as a specification - it is wrong as an answer.
//!
//! A request the model cannot settle on is not answered at all: it comes back
//! as an ambiguity that carries the candidates it could not choose between.

use regie_core::spec::{self, CandidateSpec, IntentSpec};
use regie_core::Regex;
use regie_intent::IntentModel;

/// Two samples of a specification, each with the label that names it.
type Samples = (&'static str, &'static str, &'static str, &'static str);

/// The width of the label column of the sample lines.
const LABEL_WIDTH: usize = 16;

fn main() {
    let model = IntentModel::embedded();

    // Every request is close to a family boundary.  Each case carries the
    // value the request is about and a second sample that shows where the
    // boundary lies.
    let cases: [(&str, Option<Samples>); 12] = [
        (
            "validate a CNPJ",
            Some((
                "value",
                "12.345.678/0001-95",
                "one digit short",
                "12.345.678/0001-9",
            )),
        ),
        (
            "validate a CPF",
            Some(("value", "123.456.789-09", "no check digits", "123.456.789")),
        ),
        (
            "validate a NIF",
            Some(("value", "123456789", "punctuated", "123.456.789")),
        ),
        (
            "validate a brazilian phone number",
            Some(("value", "+55 11 91234-5678", "free text", "not a phone")),
        ),
        (
            "validate a brazilian postal code",
            Some(("value", "01310-100", "truncated", "01310")),
        ),
        (
            "extract a date in ISO format",
            Some((
                "value",
                "2026-10-29",
                "in a sentence",
                "the date is 2026-10-29",
            )),
        ),
        (
            "extract the quoted text",
            Some(("value", "say \"hi\"", "no quotes", "no quotes here")),
        ),
        (
            "extract the numbers",
            Some(("value", "process 12345", "no digits", "no digits here")),
        ),
        (
            "block messages containing the word urgent",
            Some(("value", "an urgent message", "no term", "a quiet message")),
        ),
        (
            "check if this is a valid email address",
            Some(("value", "ana@corp.com", "not an address", "not an email")),
        ),
        ("validate a number", None),
        ("asdkjhaskjdhaskjdh", None),
    ];

    for (request, samples) in cases {
        println!("request: {request:?}");
        match model.interpret(request) {
            IntentSpec::Confident(candidate) => {
                println!("  outcome:   confident");
                println!("  spec:      {}", describe(&candidate));
                match samples {
                    Some((label, value, other, other_value)) => {
                        show(&candidate, label, value, other, other_value)
                    }
                    None => println!("  samples:   (none)"),
                }
            }
            IntentSpec::Ambiguous {
                reason,
                suggestions,
            } => {
                println!("  outcome:   ambiguous ({reason:?})");
                if suggestions.is_empty() {
                    println!("  candidates: (none)");
                } else {
                    println!("  candidates:");
                    for suggestion in suggestions.iter() {
                        println!("    {}", describe(suggestion));
                    }
                }
            }
        }
        println!();
    }
}

/// A one-line description of a specification: the family it belongs to, the
/// name of the family and the fields that tell two candidates apart.
fn describe(candidate: &CandidateSpec) -> String {
    match candidate {
        CandidateSpec::Validate {
            kind,
            strictness,
            charset,
            max_len,
        } => format!(
            "validation {} (strictness={strictness:?}, charset={charset:?}, max_len={max_len:?})",
            spec::validation_kind_name(*kind)
        ),
        CandidateSpec::Extract {
            kind,
            boundary,
            capture,
        } => format!(
            "extraction {} (boundary={boundary:?}, capture={capture:?})",
            spec::extraction_kind_name(*kind)
        ),
        CandidateSpec::Filter { action, terms } => {
            let listed: Vec<&str> = terms.iter().filter_map(|term| term.as_str()).collect();
            format!("filter (action={action:?}, terms={listed:?})")
        }
        CandidateSpec::Custom { template, params } => format!(
            "custom (template={template:?}, min_len={:?}, max_len={:?})",
            params.min_len, params.max_len
        ),
    }
}

/// Compiles the specification and runs it against the two samples, so the
/// example shows the whole path: request, specification, automaton, verdict.
///
/// A specification that refuses the first sample answers a different question
/// than the request asked, and the reader sees it right here.
fn show(candidate: &CandidateSpec, label: &str, value: &str, other: &str, other_value: &str) {
    let re = match Regex::from_spec(candidate) {
        Ok(re) => re,
        Err(err) => {
            println!("  compiled:  failed ({err})");
            std::process::exit(1);
        }
    };

    for (name, sample) in [(label, value), (other, other_value)] {
        let verdict = if re.is_match(sample.as_bytes()) {
            "match"
        } else {
            "no match"
        };
        println!(
            "  {:<LABEL_WIDTH$}{sample:?} -> {verdict}",
            format!("{name}:")
        );
    }
}
