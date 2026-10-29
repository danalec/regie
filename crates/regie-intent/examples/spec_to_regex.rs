//! The whole path of the intent layer, from a request to a running automaton.
//!
//! Run with:
//!
//! ```text
//! cargo run -p regie-intent --example spec_to_regex
//! ```
//!
//! The model never emits pattern text: it picks one enumerated specification
//! and the deterministic compiler of `regie-core` turns that specification
//! into an automaton.  A validation family checks the *shape* of a value, so a
//! CPF with a wrong checksum is accepted - this is a matcher, not an
//! accountant.  The quoted family carries two capture groups: one for the
//! double-quote form and one for the single-quote form.

use regie_core::spec::{self, CandidateSpec, IntentSpec};
use regie_core::Regex;
use regie_intent::IntentModel;

fn main() {
    let model = IntentModel::embedded();

    // One request per family, with the samples the automaton is checked
    // against: the first one has to match, the second one has to be refused.
    let cases: [(&str, &str, &str); 7] = [
        ("validar um CPF", "123.456.789-09", "not a cpf"),
        ("check if this is a valid email address", "a@b.co", "nope"),
        ("extrair os numeros entre colchetes", "[42] here", "nope"),
        ("extrair as palavras entre aspas", "say \"hi\"", "no quotes"),
        (
            "extract the url from the text",
            "see https://x.io",
            "no url",
        ),
        ("bloquear a palavra spam", "this is spam", "this is fine"),
        ("criar um padrão de 6 alfanuméricos", "abc123", "abc12"),
    ];

    for (request, positive, negative) in cases {
        println!("request: {request:?}");
        let candidate = match model.interpret(request) {
            IntentSpec::Confident(candidate) => candidate,
            IntentSpec::Ambiguous { reason, .. } => {
                println!("  intent:   ambiguous ({reason:?})");
                println!();
                continue;
            }
        };
        println!("  intent:   confident");
        println!("  spec:     {}", describe(&candidate));

        let re = match Regex::from_spec(&candidate) {
            Ok(re) => re,
            Err(err) => {
                println!("  compiled: failed ({err})");
                println!();
                continue;
            }
        };
        let groups = re.group_count();
        println!(
            "  compiled: {groups} capture group{}",
            if groups == 1 { "" } else { "s" }
        );
        show(&re, positive);
        show(&re, negative);
        println!();
    }
}

/// A one-line description of a specification, with the fields that matter.
fn describe(candidate: &CandidateSpec) -> String {
    match candidate {
        CandidateSpec::Validate {
            kind,
            strictness,
            charset,
            max_len,
        } => format!(
            "validate {} (strictness={strictness:?}, charset={charset:?}, max_len={max_len:?})",
            spec::validation_kind_name(*kind)
        ),
        CandidateSpec::Extract {
            kind,
            boundary,
            capture,
        } => format!(
            "extract {} (boundary={boundary:?}, capture={capture:?})",
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

/// Runs one sample through the compiled automaton and prints the verdict.
fn show(re: &Regex, sample: &str) {
    if !re.is_match(sample.as_bytes()) {
        println!("  {sample:?} -> no match");
        return;
    }
    match re
        .captures(sample.as_bytes())
        .and_then(|captures| captures.get(1))
    {
        Some(group) => match sample.get(group.start..group.end) {
            Some(text) => println!("  {sample:?} -> match, group 1 = {text:?}"),
            None => println!("  {sample:?} -> match"),
        },
        None => println!("  {sample:?} -> match"),
    }
}
