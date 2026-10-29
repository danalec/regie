//! What the intent layer does when it does not settle on one specification.
//!
//! Run with:
//!
//! ```text
//! cargo run -p regie-intent --example ambiguity
//! ```
//!
//! The model refuses to guess.  It returns the reason it could not decide and
//! the candidates it considered, and the caller turns them into a question for
//! the user.  A request that is out of distribution comes back with no
//! suggestion at all: there is nothing worth offering.

use regie_core::spec::{AmbiguityReason, CandidateSpec, IntentSpec};
use regie_intent::IntentModel;

fn main() {
    let model = IntentModel::embedded();

    // The first three requests are in the corpus but do not say enough; the
    // last two are out of distribution - the model has no word for them.
    let requests = [
        "extrair o texto",
        "data",
        "validar algo",
        "me ajuda",
        "qwertyuiop zzzz",
    ];

    for request in requests {
        println!("request: {request:?}");
        match model.interpret(request) {
            IntentSpec::Confident(candidate) => {
                println!("  confident: {}", candidate.kind_name());
            }
            IntentSpec::Ambiguous {
                reason,
                suggestions,
            } => {
                println!("  reason:      {reason:?}");
                let names: Vec<&str> = suggestions.iter().map(|s| s.kind_name()).collect();
                if names.is_empty() {
                    println!("  suggestions: (none)");
                } else {
                    println!("  suggestions: {}", names.join(", "));
                }
                ask(reason, suggestions.as_slice());
            }
        }
        println!();
    }
}

/// Renders the question a caller would put to the user.
fn ask(reason: AmbiguityReason, suggestions: &[CandidateSpec]) {
    println!("  caller asks:");
    match reason {
        AmbiguityReason::ConflictingKinds => {
            println!("    Which one did you mean?");
        }
        AmbiguityReason::LowConfidence if suggestions.is_empty() => {
            println!("    I could not turn this request into a specification.");
            println!("    Could you rephrase it?");
        }
        _ => {
            println!("    I am not sure. Which one did you mean?");
        }
    }
    for (index, suggestion) in suggestions.iter().enumerate() {
        println!("      {}) {}", index + 1, suggestion.kind_name());
    }
}
