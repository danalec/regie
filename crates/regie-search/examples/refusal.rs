//! The search refusing to settle on one specification.
//!
//! Run with:
//!
//! ```text
//! cargo run -p regie-search --example refusal
//! ```
//!
//! The search has two honest ways of giving up: no candidate satisfies the
//! examples, or more than one candidate satisfies them.  In both cases it
//! returns the reason and the candidates it considered, and it never invents
//! a specification.
//!
//! The example is called `refusal` and not `ambiguity` because Cargo writes
//! the examples of every crate of a workspace into the same directory: two
//! examples with the same name would overwrite each other.

use regie_core::spec::{self, CandidateSpec, IntentSpec};
use regie_search::{search, Examples};

fn main() {
    // The label of each case is the situation it shows.
    let cases: [(&str, &str, &[&str], &[&str]); 3] = [
        (
            "no candidate accepts the positive sample",
            "validate a CPF",
            &["zzz"],
            &["123.456.789-09"],
        ),
        (
            "the same value is demanded to match and to be refused",
            "validate a CPF",
            &["123.456.789-09"],
            &["123.456.789-09"],
        ),
        (
            "two families accept the positive sample",
            "extract the bracketed text",
            &["[42]"],
            &["abc"],
        ),
    ];

    for (label, request, positives, negatives) in cases {
        println!("request: {request:?} - {label}");
        println!("  positives: {positives:?}");
        println!("  negatives: {negatives:?}");

        let report = search(request, Examples::new(positives, negatives));
        println!(
            "  explored: {}, rejected: {}",
            report.explored, report.rejected
        );

        match report.outcome {
            IntentSpec::Confident(candidate) => {
                println!("  outcome: confident {}", describe(&candidate));
            }
            IntentSpec::Ambiguous {
                reason,
                suggestions,
            } => {
                println!("  outcome: ambiguous ({reason:?})");
                if suggestions.is_empty() {
                    println!("  suggestions: (none)");
                } else {
                    println!("  suggestions:");
                    for suggestion in suggestions.iter() {
                        println!("    {}", describe(suggestion));
                    }
                }
            }
        }

        println!("  ranking:");
        for (index, candidate) in report.ranked.iter().enumerate() {
            println!(
                "    {}. {} score={} positives={}/{} negatives={}/{}",
                index + 1,
                describe(&candidate.spec),
                candidate.score,
                candidate.positives_matched,
                positives.len(),
                candidate.negatives_rejected,
                negatives.len(),
            );
        }
        println!();
    }
}

/// A one-line description of a specification, with the axis that tells two
/// candidates of the same family apart.
fn describe(candidate: &CandidateSpec) -> String {
    match candidate {
        CandidateSpec::Validate {
            kind, strictness, ..
        } => format!(
            "validate {} ({strictness:?})",
            spec::validation_kind_name(*kind)
        ),
        CandidateSpec::Extract {
            kind,
            boundary,
            capture,
        } => format!(
            "extract {} ({boundary:?}, {capture:?})",
            spec::extraction_kind_name(*kind)
        ),
        CandidateSpec::Filter { action, .. } => format!("filter ({action:?})"),
        CandidateSpec::Custom { .. } => "custom".to_string(),
    }
}
