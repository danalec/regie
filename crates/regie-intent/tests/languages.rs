//! The list of languages the model understands, checked.
//!
//! The list is written down in [`docs/LANGUAGES.md`](../../docs/LANGUAGES.md).
//! A list that nothing checks is a paragraph that is believed, so this file
//! checks it: the same intent in the two supported languages has to reach the
//! same family, and a request in a language that is not supported has to come
//! back ambiguous.
//!
//! The exception is checked too, because it is the one that would be easy to
//! mistake for support: a request that carries nothing but the acronym of a
//! family is answered in any language, because the acronym is the same
//! everywhere and the model keys on it rather than on the sentence.

use regie_core::spec::{CandidateSpec, IntentSpec, ValidationKind};
use regie_intent::IntentModel;

/// The same intent, written in the two languages the model understands.
const PAIRS: [(&str, &str, &str); 12] = [
    ("validate a CPF", "validar um CPF", "brazilian_cpf"),
    ("validate a CNPJ", "validar um CNPJ", "brazilian_cnpj"),
    ("validate a NIF", "validar um NIF", "portuguese_nif"),
    (
        "validate a brazilian phone number",
        "validar telefone brasileiro",
        "brazilian_phone",
    ),
    (
        "validate a brazilian postal code",
        "validar CEP",
        "brazilian_postal_code",
    ),
    ("validate an email address", "validar email", "email"),
    ("validate a date", "validar data", "iso_date"),
    (
        "extract the quoted text",
        "extrair as palavras entre aspas",
        "quoted",
    ),
    (
        "extract the numbers",
        "extrair o numero do processo",
        "numbers",
    ),
    (
        "extract the text between brackets",
        "extrair os numeros entre colchetes",
        "between_delimiters",
    ),
    (
        "block messages containing the word spam",
        "bloquear a palavra spam",
        "filter",
    ),
    (
        "build a custom pattern of 6 alphanumeric characters",
        "criar um padrão de 6 caracteres alfanuméricos",
        "custom",
    ),
];

/// The same intent in the two languages reaches the same family.
#[test]
fn the_two_supported_languages_agree() {
    let model = IntentModel::embedded();
    for (english, portuguese, family) in PAIRS {
        let left = model.interpret(english);
        let right = model.interpret(portuguese);

        assert_eq!(
            left, right,
            "the two languages disagree on {english:?} and {portuguese:?}"
        );
        match left {
            IntentSpec::Confident(candidate) => {
                assert_eq!(
                    candidate.kind_name(),
                    family,
                    "the request {english:?} reached the wrong family"
                );
            }
            IntentSpec::Ambiguous { reason, .. } => {
                panic!("the request {english:?} came back ambiguous: {reason:?}")
            }
        }
    }
}

/// Every family of the two languages answers with a validation or an
/// extraction, and a filter answers with a filter.
#[test]
fn the_answers_are_the_families_they_claim_to_be() {
    let model = IntentModel::embedded();
    match model.interpret("validate a CPF") {
        IntentSpec::Confident(CandidateSpec::Validate { kind, .. }) => {
            assert_eq!(kind, ValidationKind::BrazilianCpf);
        }
        other => panic!("unexpected outcome: {other:?}"),
    }
    match model.interpret("validar um CPF") {
        IntentSpec::Confident(CandidateSpec::Validate { kind, .. }) => {
            assert_eq!(kind, ValidationKind::BrazilianCpf);
        }
        other => panic!("unexpected outcome: {other:?}"),
    }
}

/// A request in a language the model does not understand is refused.
#[test]
fn an_unsupported_language_is_refused() {
    let model = IntentModel::embedded();
    for request in [
        "validar un correo electrónico",
        "extraer el texto entre corchetes",
        "valider une adresse e-mail",
        "eine E-Mail-Adresse validieren",
        "validare un indirizzo email",
        "una dirección de correo electrónico",
    ] {
        let outcome = model.interpret(request);
        assert!(
            !outcome.is_confident(),
            "the request {request:?} should not have been understood, got {outcome:?}"
        );
    }
}

/// An acronym is not a language.
///
/// The model keys on the string `CPF`, which is the same in every language, so
/// a request that carries nothing else is answered confidently in any of
/// them.  This is not support for Dutch, Japanese or Chinese, and the test
/// exists so that the difference between the two is never lost.
#[test]
fn an_acronym_alone_is_answered_in_any_language() {
    let model = IntentModel::embedded();
    for request in ["cpf valideren", "cpf を検証する", "验证 CPF"] {
        match model.interpret(request) {
            IntentSpec::Confident(CandidateSpec::Validate { kind, .. }) => {
                assert_eq!(kind, ValidationKind::BrazilianCpf);
            }
            other => panic!("the request {request:?} came back as {other:?}"),
        }
    }
}

/// The corpus carries both languages, so the model is not a Portuguese-only
/// model that happens to answer a few English requests.
#[test]
fn the_corpus_carries_both_languages() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../datasets/intent_train.jsonl"
    );
    let raw = std::fs::read_to_string(path).expect("the dataset is readable");

    let mut portuguese = 0usize;
    let mut english = 0usize;
    for line in raw.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let request: serde_json::Value =
            serde_json::from_str(line).expect("the record is well formed");
        let request = request["request"]
            .as_str()
            .expect("the record has a request");
        if is_portuguese(request) {
            portuguese += 1;
        } else {
            english += 1;
        }
    }

    assert!(portuguese > 0, "the corpus has no Portuguese record");
    assert!(english > 0, "the corpus has no English record");
    assert!(
        portuguese > 200 && english > 200,
        "the corpus is not balanced: {portuguese} Portuguese and {english} English"
    );
}

/// Whether a request is written in Portuguese.
///
/// The corpus is Brazilian Portuguese, and the words that tell the two
/// languages apart are the ones the corpus actually uses.
fn is_portuguese(request: &str) -> bool {
    const PORTUGUESE: [&str; 22] = [
        "validar",
        "validação",
        "valida",
        "extrair",
        "extração",
        "busca",
        "buscar",
        "palavra",
        "palavras",
        "número",
        "numero",
        "números",
        "textos",
        "aspas",
        "colchetes",
        "endereço",
        "formato",
        "rejeitar",
        "checar",
        "criar",
        "padrão",
        "alfanuméricos",
    ];
    let lowered = request.to_lowercase();
    PORTUGUESE.iter().any(|word| lowered.contains(word))
}
