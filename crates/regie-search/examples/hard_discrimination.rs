//! Example sets that separate families that look alike.
//!
//! Run with:
//!
//! ```text
//! cargo run -p regie-search --example hard_discrimination
//! ```
//!
//! The examples decide, not the model.  A request like "validate a CPF" or
//! "extract the bracketed text" names a shape that several families share,
//! and the examples are what tell them apart: only a candidate that matches
//! every positive example and refuses every negative one survives.  When no
//! candidate survives, the search says so instead of guessing.

use regie_core::spec::{self, CandidateSpec, IntentSpec};
use regie_core::Regex;
use regie_search::{search, Examples, SearchReport};

fn main() {
    // Each case is a request, an example set and the question it answers.
    let cases: [(&str, &str, &[&str], &[&str]); 5] = [
        (
            "the CPF family against the ones that look like it",
            "validate a CPF",
            &["123.456.789-09", "987.654.321-00"],
            &["12.345.678/0001-95", "+55 11 91234-5678", "12345"],
        ),
        (
            "the ISO shape of a date",
            "validate a date",
            &["2026-10-29", "1999-01-31"],
            &["29/10/2026", "31/01/1999"],
        ),
        (
            "the Brazilian shape of a date, which no family accepts",
            "validate a date",
            &["29/10/2026", "31/01/1999"],
            &["2026-10-29", "1999-01-31"],
        ),
        (
            "a quoted value against a bracketed one",
            "extract the bracketed text",
            &["\"value\"", "\"outro\""],
            &["[value]", "[outro]"],
        ),
        (
            "two CPFs, one demanded to match and the other to be refused",
            "validate a CPF",
            &["123.456.789-09"],
            &["987.654.321-00"],
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

        prove(&report, positives, negatives);
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

/// Compiles the winner and runs it against the examples: the claim is shown.
///
/// When the search did not settle there is no winner, so the best candidate
/// of the ranking is shown instead - and it is shown failing, which is the
/// reason the search could not settle.
fn prove(report: &SearchReport, positives: &[&str], negatives: &[&str]) {
    let (label, candidate) = match report.candidate() {
        Some(candidate) => ("the winner is", candidate),
        None => match report.ranked.get(0) {
            Some(best) => ("no winner, the best candidate is", &best.spec),
            None => {
                println!("  proof: no candidate survived, there is nothing to prove");
                return;
            }
        },
    };
    let re = match Regex::from_spec(candidate) {
        Ok(re) => re,
        Err(err) => {
            println!("  proof: the candidate did not compile ({err})");
            return;
        }
    };
    println!("  proof:");
    println!("    {label} {}", describe(candidate));
    for example in positives.iter().chain(negatives.iter()) {
        let verdict = if re.is_match(example.as_bytes()) {
            "match"
        } else {
            "no match"
        };
        println!("    {example:?} -> {verdict}");
    }
}
