//! The `search` subcommand.
//!
//! The subcommand runs the verified search of `regie-search`: the intent
//! model proposes a set of specifications, every candidate is verified
//! against the examples the user gave, and only a candidate that accepts
//! all the positive examples and rejects all the negative ones can become
//! the outcome.
//!
//! ```text
//! search: confident
//! candidate: validate brazilian_cpf strictness=standard charset=digits max_len=14
//! rank 1: validate brazilian_cpf score=2 positives=1/1 negatives_rejected=1/1
//! explored: 12 candidates, 11 rejected
//! ```
//!
//! The outcome is the verified candidate when there is exactly one, and an
//! ambiguous answer otherwise. The exit code follows that verdict: `0` for a
//! confident outcome, `1` for an ambiguous one.

use crate::render;
use crate::{CliError, EXIT_NO_MATCH, EXIT_OK};
use regie_core::spec::IntentSpec;
use regie_search::{search, Examples};

/// Runs the `search` subcommand.
///
/// Returns [`EXIT_OK`] when the search settled on one specification, and
/// [`EXIT_NO_MATCH`] when it stayed ambiguous.
pub fn run_search(request: &str, positive: &[String], negative: &[String]) -> Result<u8, CliError> {
    let positives: Vec<&str> = positive.iter().map(|example| example.as_str()).collect();
    let negatives: Vec<&str> = negative.iter().map(|example| example.as_str()).collect();
    let report = search(request, Examples::new(&positives, &negatives));

    match &report.outcome {
        IntentSpec::Confident(candidate) => {
            println!("search: confident");
            println!("candidate: {}", render::candidate_summary(candidate));
        }
        IntentSpec::Ambiguous {
            reason,
            suggestions,
        } => {
            println!("search: ambiguous");
            println!("reason: {}", render::ambiguity_reason_name(*reason));
            for (index, suggestion) in suggestions.iter().enumerate() {
                println!(
                    "suggestion {}: {}",
                    index + 1,
                    render::candidate_summary(suggestion)
                );
            }
        }
    }

    for (index, candidate) in report.ranked.iter().enumerate() {
        println!(
            "rank {}: {} score={} positives={}/{} negatives_rejected={}/{}",
            index + 1,
            render::candidate_short_name(&candidate.spec),
            candidate.score,
            candidate.positives_matched,
            positive.len(),
            candidate.negatives_rejected,
            negative.len()
        );
    }

    println!(
        "explored: {} candidates, {} rejected",
        report.explored, report.rejected
    );

    Ok(if report.is_confident() {
        EXIT_OK
    } else {
        EXIT_NO_MATCH
    })
}
