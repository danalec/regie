//! The behaviour of the intent layer.

use proptest::prelude::*;
use regie_core::spec::{
    CandidateSpec, CharsetPolicy, ExtractionKind, IntentSpec, PatternTemplate, Strictness,
    ValidationKind,
};
use regie_intent::weights;
use regie_intent::IntentModel;

/// A request the model has no word for is refused, not guessed.
#[test]
fn nonsense_is_ambiguous() {
    let model = IntentModel::embedded();
    for request in [
        "qwertyuiop zzzz",
        "asdfghjkl qwerty",
        "",
        "   ",
        "\u{1f600}\u{1f601}",
        "12345",
    ] {
        let spec = model.interpret(request);
        assert!(
            !spec.is_confident(),
            "the request {:?} should not be classified, got {:?}",
            request,
            spec
        );
    }
}

/// An ambiguous intent carries the candidates the model considered.
#[test]
fn an_ambiguous_intent_carries_suggestions() {
    let model = IntentModel::embedded();
    match model.interpret("qwertyuiop zzzz") {
        IntentSpec::Ambiguous {
            reason,
            suggestions,
        } => {
            assert!(
                format!("{:?}", reason).contains("Confidence")
                    || format!("{:?}", reason).contains("Conflict")
            );
            assert!(suggestions.len() <= 3);
        }
        IntentSpec::Confident(_) => panic!("a nonsense request must not be classified"),
    }
}

/// The same request always yields the same answer.
#[test]
fn the_model_is_deterministic() {
    let model = IntentModel::embedded();
    for request in [
        "validar um cpf",
        "extrair o que está entre colchetes",
        "bloquear as palavras spam e phishing",
        "criar um padrão de 6 caracteres alfanuméricos",
        "qwertyuiop zzzz",
    ] {
        let first = model.interpret(request);
        let second = model.interpret(request);
        assert_eq!(
            first, second,
            "the model is not deterministic for {:?}",
            request
        );

        let candidates = model.candidates(request);
        assert_eq!(candidates, model.candidates(request));
        assert!(candidates.len() <= 4);
    }
}

/// Accented and unaccented spellings of the same request agree.
#[test]
fn accents_are_folded() {
    let model = IntentModel::embedded();
    let accented = model.interpret("valida\u{e7}\u{e3}o de cpf");
    let plain = model.interpret("validacao de cpf");
    assert_eq!(accented, plain);
}

/// A plain request reaches the expected validation family.
#[test]
fn a_plain_request_is_classified() {
    let model = IntentModel::embedded();
    match model.interpret("validar um cpf") {
        IntentSpec::Confident(CandidateSpec::Validate {
            kind,
            strictness,
            charset,
            max_len,
        }) => {
            assert_eq!(kind, ValidationKind::BrazilianCpf);
            assert_eq!(strictness, Strictness::Standard);
            assert_eq!(charset, CharsetPolicy::Digits);
            assert_eq!(max_len, Some(14));
        }
        other => panic!("unexpected specification {:?}", other),
    }
}

/// A request for a delimited part reaches the expected extraction family.
#[test]
fn a_delimited_request_is_classified() {
    let model = IntentModel::embedded();
    match model.interpret("extrair o texto entre colchetes") {
        IntentSpec::Confident(CandidateSpec::Extract { kind, .. }) => {
            assert_eq!(kind, ExtractionKind::BetweenDelimiters);
        }
        other => panic!("unexpected specification {:?}", other),
    }
}

/// A request for a custom shape reaches the custom family.
#[test]
fn a_custom_request_is_classified() {
    let model = IntentModel::embedded();
    match model.interpret("criar um padr\u{e3}o de 6 caracteres alfanumericos") {
        IntentSpec::Confident(CandidateSpec::Custom { template, params }) => {
            match template {
                PatternTemplate::Repeat { class, count } => {
                    assert_eq!(class, CharsetPolicy::Alphanumeric);
                    assert_eq!(count, 6);
                }
                other => panic!("unexpected template {:?}", other),
            }
            assert_eq!(params.min_len, Some(6));
            assert_eq!(params.max_len, Some(6));
        }
        other => panic!("unexpected specification {:?}", other),
    }
}

/// The embedded weights stay inside the documented budget.
#[test]
fn the_embedded_weights_fit_the_budget() {
    let table = weights::weight_bytes();
    assert!(
        table < 256 * 1024,
        "the weight table holds {} bytes, the budget is 256 KiB",
        table
    );
    assert_eq!(table, IntentModel::weight_bytes());
}

/// The class table matches the specification contract.
#[test]
fn the_class_table_matches_the_contract() {
    assert_eq!(weights::CLASS_COUNT, 43);
    assert_eq!(weights::CLASS_NAMES[0], "validate:Email");
    assert_eq!(weights::CLASS_NAMES[30], "validate:Json");
    assert_eq!(weights::CLASS_NAMES[31], "extract:BetweenDelimiters");
    assert_eq!(weights::CLASS_NAMES[40], "extract:BeforeKeyword");
    assert_eq!(weights::CLASS_NAMES[41], "filter");
    assert_eq!(weights::CLASS_NAMES[42], "custom");
}

/// The vocabulary is sorted, so the runtime can binary search it.
#[test]
fn the_vocabulary_is_sorted() {
    let vocabulary = &weights::VOCAB;
    assert!(!vocabulary.is_empty());
    for pair in vocabulary.windows(2) {
        assert!(
            pair[0].as_bytes() < pair[1].as_bytes(),
            "the vocabulary is not sorted: {:?} then {:?}",
            pair[0],
            pair[1]
        );
    }
}

// The model never panics, whatever the input is.
proptest! {
    #[test]
    fn arbitrary_input_never_panics(bytes in prop::collection::vec(any::<u8>(), 0..600)) {
        let model = IntentModel::embedded();
        let text = String::from_utf8_lossy(&bytes);
        let spec = model.interpret(&text);
        match spec {
            IntentSpec::Confident(candidate) => {
                match candidate {
                    CandidateSpec::Validate { kind, .. } => {
                        prop_assert!(regie_core::spec::VALIDATION_KINDS.contains(&kind));
                    }
                    CandidateSpec::Extract { kind, .. } => {
                        prop_assert!(regie_core::spec::EXTRACTION_KINDS.contains(&kind));
                    }
                    CandidateSpec::Filter { .. } => {}
                    CandidateSpec::Custom { .. } => {}
                }
            }
            IntentSpec::Ambiguous { suggestions, .. } => {
                prop_assert!(suggestions.len() <= 3);
            }
        }
        let candidates = model.candidates(&text);
        prop_assert!(candidates.len() <= 4);
    }
}
