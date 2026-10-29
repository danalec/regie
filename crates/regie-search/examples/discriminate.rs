//! The same request, two example sets, two different specifications.
//!
//! Run with:
//!
//! ```text
//! cargo run -p regie-search --example discriminate
//! ```
//!
//! The examples decide, not the model: the request alone leaves several
//! families open, and the examples pin one of them down.  Only a candidate
//! that matches every positive and refuses every negative example survives.

use regie_core::spec::{self, CandidateSpec, IntentSpec};
use regie_core::Regex;
use regie_search::{search, Examples, SearchReport};

fn main() {
    let request = "extrair o texto";

    // Two example sets for the same request: one that says the value sits
    // between delimiters, one that says the value is a bare number.
    let cases: [(&str, &[&str], &[&str]); 2] = [
        ("bracketed", &["[42]", "[valor]"], &["abc", "42"]),
        ("a bare number", &["42", "7"], &["abc", "xyz"]),
    ];

    for (label, positives, negatives) in cases {
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
                for suggestion in suggestions.iter() {
                    println!("    candidate: {}", describe(suggestion));
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
fn prove(report: &SearchReport, positives: &[&str], negatives: &[&str]) {
    let candidate = match report.candidate() {
        Some(candidate) => candidate,
        None => {
            println!("  proof: no candidate survived, there is nothing to prove");
            return;
        }
    };
    let re = match Regex::from_spec(candidate) {
        Ok(re) => re,
        Err(err) => {
            println!("  proof: the winner did not compile ({err})");
            return;
        }
    };
    println!("  proof:");
    for example in positives.iter().chain(negatives.iter()) {
        let verdict = if re.is_match(example.as_bytes()) {
            "match"
        } else {
            "no match"
        };
        println!("    {example:?} -> {verdict}");
    }
}
