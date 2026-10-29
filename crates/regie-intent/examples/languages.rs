//! The languages the model understands.
//!
//! Run with:
//!
//! ```text
//! cargo run -p regie-intent --example languages
//! ```
//!
//! The model answers in two languages: English and Brazilian Portuguese.  The
//! same request in either of them reaches the same family, and a request in
//! any other language comes back as an ambiguity - the model did not
//! understand it, and it says so instead of guessing.
//!
//! There is one exception, and it is worth showing: a request that carries
//! nothing but the acronym of a family is answered in *any* language, because
//! the acronym is the same everywhere and the model keys on it rather than on
//! the sentence around it.
//!
//! The list of languages is written down in
//! [`docs/LANGUAGES.md`](../../docs/LANGUAGES.md), and
//! `crates/regie-intent/tests/languages.rs` checks it.

use regie_core::spec::IntentSpec;
use regie_intent::IntentModel;

fn main() {
    let model = IntentModel::embedded();

    println!("the same request in the two supported languages:");
    println!();
    let pairs = [
        ("a CPF", "validate a CPF", "validar um CPF"),
        ("a CNPJ", "validate a CNPJ", "validar um CNPJ"),
        ("a NIF", "validate a NIF", "validar um NIF"),
        (
            "a Brazilian phone number",
            "validate a brazilian phone number",
            "validar telefone brasileiro",
        ),
        (
            "a Brazilian postal code",
            "validate a brazilian postal code",
            "validar CEP",
        ),
        (
            "an email address",
            "validate an email address",
            "validar email",
        ),
        ("a date", "validate a date", "validar data"),
        (
            "the quoted text",
            "extract the quoted text",
            "extrair as palavras entre aspas",
        ),
        (
            "the numbers",
            "extract the numbers",
            "extrair o numero do processo",
        ),
        (
            "the text between brackets",
            "extract the text between brackets",
            "extrair os numeros entre colchetes",
        ),
        (
            "a word to block",
            "block messages containing the word spam",
            "bloquear a palavra spam",
        ),
        (
            "a custom shape",
            "build a custom pattern of 6 alphanumeric characters",
            "criar um padrão de 6 caracteres alfanuméricos",
        ),
    ];

    for (what, english, portuguese) in pairs {
        let left = model.interpret(english);
        let right = model.interpret(portuguese);
        println!("{what}:");
        println!("  english     {english:?}");
        println!("  portuguese  {portuguese:?}");
        println!("  english     -> {}", verdict(&left));
        println!("  portuguese  -> {}", verdict(&right));
        println!(
            "  the same    -> {}",
            if left == right { "yes" } else { "no" }
        );
        println!();
    }

    println!("a request in a language the model does not understand:");
    println!();
    for request in [
        "validar un correo electrónico",
        "valider une adresse e-mail",
        "eine E-Mail-Adresse validieren",
        "validare un indirizzo email",
    ] {
        let outcome = model.interpret(request);
        println!("  {request:?}");
        println!("    -> {}", verdict(&outcome));
    }
    println!();

    println!("the exception: an acronym is not a language");
    println!();
    for request in ["cpf valideren", "cpf を検証する", "验证 CPF"] {
        let outcome = model.interpret(request);
        println!("  {request:?}");
        println!("    -> {}", verdict(&outcome));
    }
}

/// One line about the outcome of a request.
fn verdict(outcome: &IntentSpec) -> String {
    match outcome {
        IntentSpec::Confident(candidate) => format!("confident {}", candidate.kind_name()),
        IntentSpec::Ambiguous {
            reason,
            suggestions,
        } => {
            let names: Vec<&str> = suggestions.iter().map(|s| s.kind_name()).collect();
            format!("ambiguous {reason:?} [{names:?}]")
        }
    }
}
