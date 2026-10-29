//! Verified hypothesis search: a request plus examples, turned into a
//! specification that is known to fit the examples.
//!
//! Run with:
//!
//! ```text
//! cargo run -p regie-search --example search
//! ```
//!
//! The search never invents a pattern. It starts from the candidates the
//! intent model proposes, mutates them along the axes of their family, and
//! keeps the ones that match every positive example and reject every negative
//! one. When no candidate fits, the answer is `Ambiguous`, not a guess.

use regie_core::spec::IntentSpec;
use regie_search::{search, Examples};

fn main() {
    // A request the intent model understands, with examples that pin it down.
    let positives = ["123.456.789-09", "987.654.321-00"];
    let negatives = ["123.456.789", "not a cpf at all"];
    report("validar um CPF", &positives, &negatives);

    // A request with a single example: several families could fit it.
    let positives = ["123-45"];
    let negatives = [];
    report("validate something", &positives, &negatives);

    // A request whose examples no candidate can satisfy: the search refuses.
    let positives = ["this is not a cpf"];
    let negatives = ["123.456.789-09"];
    report("validar um CPF", &positives, &negatives);
}

/// Runs one search and prints what it settled on.
fn report(request: &str, positives: &[&str], negatives: &[&str]) {
    println!("request: {request:?}");
    println!("  positives: {positives:?}");
    println!("  negatives: {negatives:?}");

    let examples = Examples::new(positives, negatives);
    let report = search(request, examples);

    println!(
        "  explored: {}, rejected: {}",
        report.explored, report.rejected
    );

    match report.outcome {
        IntentSpec::Confident(candidate) => {
            println!("  outcome: confident {}", candidate.kind_name());
        }
        IntentSpec::Ambiguous {
            reason,
            suggestions,
        } => {
            println!("  outcome: ambiguous ({reason:?})");
            for suggestion in suggestions.iter() {
                println!("    candidate: {}", suggestion.kind_name());
            }
        }
    }

    for (index, candidate) in report.ranked.iter().enumerate() {
        println!(
            "  rank {}: {} score={} positives={}/{} negatives_rejected={}/{}",
            index + 1,
            candidate.spec.kind_name(),
            candidate.score,
            candidate.positives_matched,
            positives.len(),
            candidate.negatives_rejected,
            negatives.len(),
        );
    }

    println!();
}
