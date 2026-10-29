//! The behaviour of the intent layer.

use proptest::prelude::*;
use regie_core::spec::{
    AmbiguityReason, CandidateSpec, CharsetPolicy, ExtractionKind, IntentSpec, PatternTemplate,
    Strictness, ValidationKind,
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
        "validate a CPF",
        "extract the text between brackets",
        "block the words spam and phishing",
        "build a custom pattern of 6 alphanumeric characters",
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
///
/// The accents belong to the Brazilian Portuguese half of the corpus, which is
/// where the accented spellings of a request live, so the pair is written in
/// that language.
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
    match model.interpret("validate a CPF") {
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
    match model.interpret("extract the text between brackets") {
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
    match model.interpret("build a custom pattern of 6 alphanumeric characters") {
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

/// A request that sits on a family boundary is answered with a valid
/// specification that answers a different question.
///
/// The model chooses one value out of a closed enumeration, so a wrong guess
/// is not an error: it is a well formed specification for another family.
/// This is the sharpest edge of the layer, and the behaviour is pinned here
/// so a future change to the weights cannot move it silently.
#[test]
fn a_wrong_guess_is_a_valid_specification_for_another_family() {
    let model = IntentModel::embedded();
    match model.interpret("extract a date in ISO format") {
        IntentSpec::Confident(CandidateSpec::Validate { kind, .. }) => {
            assert_eq!(kind, ValidationKind::IsoDate);
        }
        other => panic!("unexpected specification {:?}", other),
    }

    // The specification is a *validation*, so it is anchored: the date inside
    // a sentence is refused, even though the request asked to extract it.
    let spec = model.interpret("extract a date in ISO format");
    let candidate = match spec {
        IntentSpec::Confident(candidate) => candidate,
        other => panic!("unexpected specification {:?}", other),
    };
    let regex = regie_core::Regex::from_spec(&candidate).expect("should compile");
    assert!(regex.is_match(b"2026-10-29"));
    assert!(!regex.is_match(b"the date is 2026-10-29"));
}

/// A request the model cannot settle between two kinds carries the kinds it
/// weighed.
#[test]
fn a_conflicting_request_carries_its_candidates() {
    let model = IntentModel::embedded();
    match model.interpret("extract the text") {
        IntentSpec::Ambiguous {
            reason,
            suggestions,
        } => {
            assert_eq!(reason, AmbiguityReason::ConflictingKinds);
            assert_eq!(suggestions.len(), 3);
            let kinds: Vec<ExtractionKind> = suggestions
                .iter()
                .filter_map(|candidate| match candidate {
                    CandidateSpec::Extract { kind, .. } => Some(*kind),
                    _ => None,
                })
                .collect();
            assert!(kinds.contains(&ExtractionKind::Words));
            assert!(kinds.contains(&ExtractionKind::Urls));
            assert!(kinds.contains(&ExtractionKind::IpAddresses));
        }
        other => panic!("unexpected specification {:?}", other),
    }
}

/// A request the model is not confident about carries the candidates it
/// weighed.
#[test]
fn an_uncertain_request_carries_its_candidates() {
    let model = IntentModel::embedded();
    match model.interpret("validate a number") {
        IntentSpec::Ambiguous {
            reason,
            suggestions,
        } => {
            assert_eq!(reason, AmbiguityReason::LowConfidence);
            assert_eq!(suggestions.len(), 3);
            let kinds: Vec<ValidationKind> = suggestions
                .iter()
                .filter_map(|candidate| match candidate {
                    CandidateSpec::Validate { kind, .. } => Some(*kind),
                    _ => None,
                })
                .collect();
            assert!(kinds.contains(&ValidationKind::Decimal));
            assert!(kinds.contains(&ValidationKind::BrazilianPhone));
            assert!(kinds.contains(&ValidationKind::CreditCard));
        }
        other => panic!("unexpected specification {:?}", other),
    }
}

/// A request that is not a request at all comes back with no candidate.
#[test]
fn a_request_without_a_candidate_carries_nothing() {
    let model = IntentModel::embedded();
    match model.interpret("asdkjhaskjdhaskjdh") {
        IntentSpec::Ambiguous {
            reason,
            suggestions,
        } => {
            assert_eq!(reason, AmbiguityReason::LowConfidence);
            assert!(suggestions.is_empty());
        }
        other => panic!("unexpected specification {:?}", other),
    }
}

/// Every family the model can reach is a real family of the specification.
#[test]
fn the_reachable_families_are_real() {
    let model = IntentModel::embedded();
    for request in [
        "validate a CNPJ",
        "validate a CPF",
        "validate a NIF",
        "validate a brazilian phone number",
        "validate a brazilian postal code",
        "validate an email address",
        "extract the quoted text",
        "extract the numbers",
        "block messages containing the word urgent",
        "check if this is a valid email address",
    ] {
        match model.interpret(request) {
            IntentSpec::Confident(candidate) => match candidate {
                CandidateSpec::Validate { kind, .. } => {
                    assert!(
                        regie_core::spec::VALIDATION_KINDS.contains(&kind),
                        "request {:?} produced {:?}",
                        request,
                        kind
                    );
                }
                CandidateSpec::Extract { kind, .. } => {
                    assert!(
                        regie_core::spec::EXTRACTION_KINDS.contains(&kind),
                        "request {:?} produced {:?}",
                        request,
                        kind
                    );
                }
                CandidateSpec::Filter { .. } => {}
                CandidateSpec::Custom { .. } => {}
            },
            other => panic!("request {:?} produced {:?}", request, other),
        }
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
