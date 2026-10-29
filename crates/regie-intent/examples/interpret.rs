//! Turning a natural-language request into a structured specification.
//!
//! Run with:
//!
//! ```text
//! cargo run -p regie-intent --example interpret
//! ```
//!
//! The model is local, deterministic and tiny. It answers with an enumerated
//! [`CandidateSpec`], never with pattern text, and it refuses to guess when
//! the request is not clear enough.

use regie_core::spec::CandidateSpec;
use regie_core::Regex;
use regie_intent::IntentModel;

fn main() {
    let model = IntentModel::embedded();

    let requests = [
        "validar um CPF",
        "check if this is a valid email address",
        "extrair os numeros entre colchetes",
        "extrair as palavras entre aspas",
        "bloquear a palavra spam",
        "extract the url from the text",
        "asdkjhaskjdhaskjdh",
    ];

    for request in requests {
        println!("request: {request:?}");
        let spec = model.interpret(request);

        match spec {
            regie_core::spec::IntentSpec::Confident(candidate) => {
                println!("  -> confident: {}", candidate.kind_name());
                show(&candidate);
            }
            regie_core::spec::IntentSpec::Ambiguous {
                reason,
                suggestions,
            } => {
                println!("  -> ambiguous: {reason:?}");
                for suggestion in suggestions.iter() {
                    println!("       candidate: {}", suggestion.kind_name());
                }
            }
        }
        println!();
    }
}

/// Compiles the specification and runs it against a sample, so the example
/// shows the whole path: request, specification, automaton, match.
fn show(candidate: &CandidateSpec) {
    let re = match Regex::from_spec(candidate) {
        Ok(re) => re,
        Err(err) => {
            println!("       (could not compile: {err})");
            return;
        }
    };

    let samples: &[&[u8]] = match candidate {
        CandidateSpec::Validate { kind, .. } => match kind {
            regie_core::spec::ValidationKind::BrazilianCpf => &[b"123.456.789-09", b"nope"],
            regie_core::spec::ValidationKind::Email => &[b"ana@corp.com", b"nope"],
            regie_core::spec::ValidationKind::Url => &[b"https://example.com", b"nope"],
            _ => &[b"sample", b"nope"],
        },
        CandidateSpec::Extract { .. } => &[b"the value is [42] here", b"nothing here"],
        CandidateSpec::Filter { .. } => &[b"this is spam", b"this is fine"],
        CandidateSpec::Custom { .. } => &[b"sample", b"nope"],
    };

    for sample in samples {
        let matched = re.is_match(sample);
        let text = String::from_utf8_lossy(sample);
        println!("       {text:?} -> {matched}");
    }
}
