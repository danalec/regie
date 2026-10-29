//! The behaviour of the verified search.

use proptest::prelude::*;
use regie_core::spec::{
    AmbiguityReason, BoundaryPolicy, CandidateSpec, CapturePolicy, CharsetPolicy, ExtractionKind,
    FilterAction, IntentSpec, PatternParams, PatternTemplate, PatternTerm, Strictness,
    ValidationKind,
};
use regie_intent::IntentModel;
use regie_search::{
    search, search_flat, search_with_candidates, search_with_candidates_flat, Examples, SmallVec,
};

/// The candidate of a dotted CPF, the way the intent model builds it.
fn cpf(strictness: Strictness) -> CandidateSpec {
    CandidateSpec::Validate {
        kind: ValidationKind::BrazilianCpf,
        strictness,
        charset: CharsetPolicy::Digits,
        max_len: Some(14),
    }
}

/// A request with unambiguous examples returns the right family.
#[test]
fn a_dotted_cpf_is_confident() {
    let positives = ["123.456.789-09"];
    let negatives = ["1234567890a"];
    let report = search(
        "validate a CPF",
        Examples {
            positive: &positives,
            negative: &negatives,
        },
    );

    match report.outcome {
        IntentSpec::Confident(CandidateSpec::Validate { kind, .. }) => {
            assert_eq!(kind, ValidationKind::BrazilianCpf);
        }
        other => panic!("unexpected outcome: {:?}", other),
    }
    assert!(report.is_confident());
    assert!(report.explored > 0);
}

/// The strictness of the survivor is the one the model proposed.
#[test]
fn the_survivor_keeps_the_strictness_of_the_model() {
    let positives = ["123.456.789-09"];
    let negatives = ["1234567890a"];
    let report = search(
        "validate a CPF",
        Examples {
            positive: &positives,
            negative: &negatives,
        },
    );

    match report.outcome {
        IntentSpec::Confident(CandidateSpec::Validate {
            strictness,
            charset,
            ..
        }) => {
            assert_eq!(strictness, Strictness::Standard);
            assert_eq!(charset, CharsetPolicy::Digits);
        }
        other => panic!("unexpected outcome: {:?}", other),
    }
}

/// Examples that contradict each other are refused, not guessed.
#[test]
fn contradictory_examples_are_ambiguous() {
    let positives = ["123.456.789-09"];
    let negatives = ["123.456.789-09"];
    let report = search(
        "validate a CPF",
        Examples {
            positive: &positives,
            negative: &negatives,
        },
    );

    match report.outcome {
        IntentSpec::Ambiguous {
            reason,
            suggestions,
        } => {
            assert_eq!(reason, AmbiguityReason::InconsistentExamples);
            assert!(suggestions.is_empty(), "a refused search suggests nothing");
        }
        other => panic!("unexpected outcome: {:?}", other),
    }
}

/// Two positives no candidate can match at once are refused as well.
#[test]
fn unsatisfiable_examples_are_ambiguous() {
    let positives = ["123.456.789-09", "not a cpf at all"];
    let negatives = ["1234567890a"];
    let report = search(
        "validate a CPF",
        Examples {
            positive: &positives,
            negative: &negatives,
        },
    );

    match report.outcome {
        IntentSpec::Ambiguous { reason, .. } => {
            assert_eq!(reason, AmbiguityReason::InconsistentExamples);
        }
        other => panic!("unexpected outcome: {:?}", other),
    }
}

/// Two families that fit the examples are reported as a conflict.
#[test]
fn two_families_are_a_conflict() {
    // A dotted CPF is also a semantic version, so both families survive.
    let positives = ["123.456.789-09"];
    let negatives = ["1234567890a"];
    let report = search(
        "validate the CPF",
        Examples {
            positive: &positives,
            negative: &negatives,
        },
    );

    match report.outcome {
        IntentSpec::Ambiguous {
            reason,
            suggestions,
        } => {
            assert_eq!(reason, AmbiguityReason::ConflictingKinds);
            assert!(!suggestions.is_empty());
            assert!(suggestions.len() <= 3);
            // The most promising family comes first.
            match suggestions.get(0) {
                Some(CandidateSpec::Validate { kind, .. }) => {
                    assert_eq!(*kind, ValidationKind::BrazilianCpf);
                }
                other => panic!("unexpected suggestion: {:?}", other),
            }
        }
        other => panic!("unexpected outcome: {:?}", other),
    }
}

/// Without examples the search falls back to the ranking of the model.
#[test]
fn no_examples_returns_the_model_ranking() {
    let report = search(
        "validate a CPF",
        Examples {
            positive: &[],
            negative: &[],
        },
    );
    let model = IntentModel::embedded();

    assert!(report.is_confident());
    assert_eq!(report.outcome, model.interpret("validate a CPF"));
    assert_eq!(
        report.ranked.len(),
        model.candidates("validate a CPF").len()
    );
}

/// A request the model does not understand stays ambiguous.
#[test]
fn no_examples_keeps_the_ambiguity_of_the_model() {
    let report = search(
        "qwertyuiop zzzz",
        Examples {
            positive: &[],
            negative: &[],
        },
    );
    assert!(!report.is_confident());
}

/// The same search always yields the same report.
#[test]
fn the_search_is_deterministic() {
    let positives = ["123.456.789-09"];
    let negatives = ["1234567890a", "abc"];
    let examples = Examples {
        positive: &positives,
        negative: &negatives,
    };

    let first = search("validate a CPF", examples);
    let second = search("validate a CPF", examples);
    assert_eq!(first, second);
    assert_eq!(first.outcome, second.outcome);
    assert_eq!(first.ranked, second.ranked);
    assert_eq!(first.explored, second.explored);
    assert_eq!(first.rejected, second.rejected);
}

/// A candidate that does not compile is rejected, never a panic.
#[test]
fn a_candidate_that_does_not_compile_is_rejected() {
    // Six hundred mandatory characters do not fit in the instruction budget.
    let broken = CandidateSpec::Custom {
        template: PatternTemplate::Repeat {
            class: CharsetPolicy::Digits,
            count: 600,
        },
        params: PatternParams::default(),
    };
    let positives = ["123.456.789-09"];
    let negatives = ["1234567890a"];
    let examples = Examples {
        positive: &positives,
        negative: &negatives,
    };

    let report = search_with_candidates(&[broken], examples);
    match report.outcome {
        IntentSpec::Ambiguous { reason, .. } => {
            assert_eq!(reason, AmbiguityReason::InconsistentExamples);
        }
        other => panic!("unexpected outcome: {:?}", other),
    }
    assert_eq!(report.rejected, report.explored);
}

/// A broken candidate does not keep a good one from surviving.
#[test]
fn a_good_candidate_survives_a_broken_one() {
    let broken = CandidateSpec::Custom {
        template: PatternTemplate::Repeat {
            class: CharsetPolicy::Digits,
            count: 600,
        },
        params: PatternParams::default(),
    };
    let positives = ["123.456.789-09"];
    let negatives = ["1234567890a"];
    let examples = Examples {
        positive: &positives,
        negative: &negatives,
    };

    let report = search_with_candidates(&[broken, cpf(Strictness::Standard)], examples);
    match report.outcome {
        IntentSpec::Confident(CandidateSpec::Validate { kind, .. }) => {
            assert_eq!(kind, ValidationKind::BrazilianCpf);
        }
        other => panic!("unexpected outcome: {:?}", other),
    }
}

/// An explicit list of candidates is explored like the ranking of the model.
#[test]
fn an_explicit_list_is_explored() {
    let positives = ["123.456.789-09"];
    let negatives = ["1234567890a"];
    let examples = Examples {
        positive: &positives,
        negative: &negatives,
    };

    let report = search_with_candidates(&[cpf(Strictness::Standard)], examples);
    assert!(report.is_confident());
    assert_eq!(report.ranked.len(), 4);
    assert_eq!(report.ranked.get(0).map(|entry| entry.score), Some(1100));
}

/// The tree search and the flat enumeration reach the same outcome.
#[test]
fn the_tree_search_agrees_with_the_flat_search() {
    let cases: [(&str, &[&str], &[&str]); 7] = [
        ("validate a CPF", &["123.456.789-09"], &["1234567890a"]),
        ("validate the CPF", &["123.456.789-09"], &["1234567890a"]),
        ("extract the text between brackets", &["[abc]"], &["abc"]),
        (
            "block the words spam and phishing",
            &["please drop the spam"],
            &["hello world"],
        ),
        (
            "build a custom pattern of 6 alphanumeric characters",
            &["abc123"],
            &["abcdef"],
        ),
        (
            "validate an email address",
            &["ana@corp.com"],
            &["ana@corp"],
        ),
        ("qwertyuiop zzzz", &["123"], &["456"]),
    ];

    for (request, positives, negatives) in cases {
        let examples = Examples {
            positive: positives,
            negative: negatives,
        };
        let tree = search(request, examples);
        let flat = search_flat(request, examples);
        assert_eq!(
            tree.outcome, flat.outcome,
            "the paths disagree on {:?}",
            request
        );
        assert_eq!(
            tree.ranked, flat.ranked,
            "the paths disagree on {:?}",
            request
        );
        assert_eq!(tree.explored, flat.explored);
        assert_eq!(tree.rejected, flat.rejected);
    }
}

/// Two families in an explicit list are reported as a conflict as well.
#[test]
fn two_families_in_an_explicit_list_are_a_conflict() {
    let mut terms = SmallVec::new();
    terms.push(PatternTerm::word(b"spam"));
    let filter = CandidateSpec::Filter {
        action: FilterAction::Block,
        terms,
    };
    let words = CandidateSpec::Extract {
        kind: ExtractionKind::Words,
        boundary: BoundaryPolicy::Greedy,
        capture: CapturePolicy::Whole,
    };

    // The filter matches "spam" and the word extraction matches it too, and
    // neither of them matches "123".  The extraction comes first, so the
    // filter is the second family to survive.
    let positives = ["spam"];
    let negatives = ["123"];
    let report = search_with_candidates(
        &[words, filter],
        Examples {
            positive: &positives,
            negative: &negatives,
        },
    );

    match report.outcome {
        IntentSpec::Ambiguous {
            reason,
            suggestions,
        } => {
            assert_eq!(reason, AmbiguityReason::ConflictingKinds);
            assert_eq!(suggestions.len(), 2);
        }
        other => panic!("unexpected outcome: {:?}", other),
    }
}

/// The flat search over an explicit list agrees with itself.
#[test]
fn the_flat_search_over_an_explicit_list_is_deterministic() {
    let positives = ["123.456.789-09"];
    let negatives = ["1234567890a"];
    let examples = Examples {
        positive: &positives,
        negative: &negatives,
    };
    let candidates = [cpf(Strictness::Standard)];

    let first = search_with_candidates_flat(&candidates, examples);
    let second = search_with_candidates_flat(&candidates, examples);
    assert_eq!(first, second);
}

/// The budget of the search is respected.
#[test]
fn the_search_stays_inside_its_budget() {
    let positives = ["123.456.789-09"];
    let negatives = ["1234567890a"];
    let report = search(
        "validate a CPF",
        Examples {
            positive: &positives,
            negative: &negatives,
        },
    );

    assert!(report.explored as usize <= regie_search::MAX_EXPLORED);
    assert!(report.rejected <= report.explored);
    assert_eq!(
        report.ranked.len().min(report.ranked.capacity()),
        report.ranked.len()
    );
}

/// An empty candidate list without examples is refused.
#[test]
fn an_empty_list_is_refused() {
    let report = search_with_candidates(
        &[],
        Examples {
            positive: &[],
            negative: &[],
        },
    );
    match report.outcome {
        IntentSpec::Ambiguous {
            reason,
            suggestions,
        } => {
            assert_eq!(reason, AmbiguityReason::LowConfidence);
            assert!(suggestions.is_empty());
        }
        other => panic!("unexpected outcome: {:?}", other),
    }
}

// The search never panics, whatever the examples are.
proptest! {
    #[test]
    fn arbitrary_examples_never_panic(
        request in "[a-zA-Z ]{0,24}",
        positives in prop::collection::vec("[a-zA-Z0-9@. ]{0,16}", 0..6),
        negatives in prop::collection::vec("[a-zA-Z0-9@. ]{0,16}", 0..6),
    ) {
        let positives: Vec<&str> = positives.iter().map(|text| text.as_str()).collect();
        let negatives: Vec<&str> = negatives.iter().map(|text| text.as_str()).collect();
        let examples = Examples { positive: &positives, negative: &negatives };

        let report = search(&request, examples);
        match report.outcome {
            IntentSpec::Confident(spec) => {
                prop_assert!(!spec.kind_name().is_empty());
            }
            IntentSpec::Ambiguous { suggestions, .. } => {
                prop_assert!(suggestions.len() <= 3);
            }
        }
        prop_assert!(report.explored as usize <= regie_search::MAX_EXPLORED);
    }

    #[test]
    fn a_confident_winner_matches_every_positive(
        request in "[a-zA-Z ]{0,16}",
        positives in prop::collection::vec("[a-zA-Z0-9@. ]{1,8}", 1..4),
        negatives in prop::collection::vec("[a-zA-Z0-9@. ]{1,8}", 0..4),
    ) {
        // The search is *verified*: a candidate only survives when it matches
        // every positive example and refuses every negative one.  The winner
        // therefore has to behave that way when it is compiled again.
        let positives: Vec<&str> = positives.iter().map(|text| text.as_str()).collect();
        let negatives: Vec<&str> = negatives.iter().map(|text| text.as_str()).collect();
        let examples = Examples { positive: &positives, negative: &negatives };

        let report = search(&request, examples);
        let candidate = match report.outcome {
            IntentSpec::Confident(candidate) => candidate,
            IntentSpec::Ambiguous { .. } => return Ok(()),
        };

        let regex = match regie_core::Regex::from_spec(&candidate) {
            Ok(regex) => regex,
            Err(_) => return Ok(()),
        };
        for positive in &positives {
            prop_assert!(
                regex.is_match(positive.as_bytes()),
                "the winner {:?} refused the positive {:?}",
                candidate.kind_name(),
                positive
            );
        }
        for negative in &negatives {
            prop_assert!(
                !regex.is_match(negative.as_bytes()),
                "the winner {:?} matched the negative {:?}",
                candidate.kind_name(),
                negative
            );
        }
    }
}

/// The examples decide the family, not the model.
///
/// The same request with two different sets of examples gives two different
/// answers: this is the property that makes the search a search and not a
/// second opinion from the model.
#[test]
fn the_examples_decide_the_family() {
    let iso_positives = ["2026-10-29", "1999-01-31"];
    let iso_negatives = ["29/10/2026", "31/01/1999"];
    let report = search(
        "validate a date",
        Examples {
            positive: &iso_positives,
            negative: &iso_negatives,
        },
    );
    match report.outcome {
        IntentSpec::Confident(CandidateSpec::Validate { kind, .. }) => {
            assert_eq!(kind, ValidationKind::IsoDate);
        }
        other => panic!("unexpected outcome: {:?}", other),
    }

    // The same request with the Brazilian shape of a date has no family at
    // all, so the search refuses to settle instead of guessing.
    let br_positives = ["29/10/2026", "31/01/1999"];
    let br_negatives = ["2026-10-29", "1999-01-31"];
    let report = search(
        "validate a date",
        Examples {
            positive: &br_positives,
            negative: &br_negatives,
        },
    );
    match report.outcome {
        IntentSpec::Ambiguous {
            reason,
            suggestions,
        } => {
            assert_eq!(reason, AmbiguityReason::InconsistentExamples);
            assert!(suggestions.is_empty(), "a refused search suggests nothing");
        }
        other => panic!("unexpected outcome: {:?}", other),
    }
    assert_eq!(report.explored, report.rejected);
}

/// A shape no family accepts is refused with nothing to show for it.
#[test]
fn a_shape_no_family_accepts_is_refused() {
    let positives = ["29/10/2026", "31/01/1999"];
    let negatives = ["2026-10-29", "1999-01-31"];
    let report = search(
        "validate a date",
        Examples {
            positive: &positives,
            negative: &negatives,
        },
    );

    assert!(!report.is_confident());
    assert_eq!(report.explored, report.rejected);
    // The best candidate still matches none of the positives.
    let best = report.ranked.get(0).expect("the ranking is not empty");
    assert_eq!(best.positives_matched, 0);
    assert_eq!(best.negatives_rejected, 2);
}

/// Two values of the same family, one demanded to match and the other to be
/// refused, leave the search with no answer.
///
/// The family that accepts the first value accepts the second one as well, so
/// no candidate can satisfy both demands at once.
#[test]
fn two_values_of_one_family_cannot_be_split() {
    let positives = ["123.456.789-09"];
    let negatives = ["987.654.321-00"];
    let report = search(
        "validate a CPF",
        Examples {
            positive: &positives,
            negative: &negatives,
        },
    );

    match report.outcome {
        IntentSpec::Ambiguous {
            reason,
            suggestions,
        } => {
            assert_eq!(reason, AmbiguityReason::InconsistentExamples);
            assert!(suggestions.is_empty());
        }
        other => panic!("unexpected outcome: {:?}", other),
    }
    // The family is the right one, and it is the examples that rule it out.
    let best = report.ranked.get(0).expect("the ranking is not empty");
    assert_eq!(best.positives_matched, 1);
    assert_eq!(best.negatives_rejected, 0);
}

/// The quoting of the examples is enough to tell two extraction families
/// apart.
#[test]
fn the_quoting_of_the_examples_picks_the_family() {
    let positives = ["\"value\"", "\"outro\""];
    let negatives = ["[value]", "[outro]"];
    let report = search(
        "extract the bracketed text",
        Examples {
            positive: &positives,
            negative: &negatives,
        },
    );

    match report.outcome {
        IntentSpec::Confident(CandidateSpec::Extract { kind, .. }) => {
            assert_eq!(kind, ExtractionKind::Quoted);
        }
        other => panic!("unexpected outcome: {:?}", other),
    }
}
