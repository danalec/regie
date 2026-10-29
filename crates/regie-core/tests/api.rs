//! Coverage of the public surface of `regie-core`.
//!
//! The engine tests in `engine.rs` cover the behaviour of the matcher; this
//! file covers the *surface*: every error variant renders a message, the
//! inline vector behaves like a vector, the specification helpers name every
//! family, and the builder, the automaton and the arena expose the whole of
//! their documented contract.
//!
//! Every public item is exercised here at least once, including the paths
//! that only a caller can reach: the error branches of the automaton
//! constructor, the overflow of the arena and the inline vector.

use regie_core::dfa::{Dfa, MAX_CLASSES, MAX_DFA_STATES};
use regie_core::error::RegexError;
use regie_core::hir::{AssertKind, HirArena, HirNode, MAX_HIR_NODES};
use regie_core::spec::{
    extraction_kind_name, validation_kind_name, AmbiguityReason, BoundaryPolicy, CandidateSpec,
    CapturePolicy, CharsetPolicy, ExtractionKind, FilterAction, IntentSpec, PatternParams,
    PatternTemplate, PatternTerm, Strictness, TemplatePiece, TermKind, ValidationKind,
    EXTRACTION_KINDS, VALIDATION_KINDS,
};
use regie_core::{Captures, Match, Regex, RegexBuilder, RegexError as PublicError, SmallVec};

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// Every variant renders a message that names the problem.
#[test]
fn every_error_renders_a_message() {
    let cases: Vec<(RegexError, &str)> = vec![
        (
            RegexError::PatternTooLong {
                limit: 512,
                actual: 600,
            },
            "pattern is too long: 600 bytes, limit is 512",
        ),
        (
            RegexError::Syntax {
                position: 3,
                message: "unclosed group",
            },
            "syntax error at byte 3: unclosed group",
        ),
        (
            RegexError::Unsupported {
                position: 2,
                feature: "look-ahead",
            },
            "unsupported construct `look-ahead` at byte 2",
        ),
        (
            RegexError::ProgramTooLarge { limit: 512 },
            "compiled program exceeds 512 instructions",
        ),
        (
            RegexError::TooManyGroups { limit: 8 },
            "pattern uses more than 8 capture groups",
        ),
        (
            RegexError::RepetitionTooLarge { limit: 255 },
            "repetition bound exceeds 255",
        ),
        (
            RegexError::NestingTooDeep { limit: 32 },
            "pattern nests groups deeper than 32",
        ),
        (
            RegexError::InvalidClass {
                position: 1,
                message: "empty character class",
            },
            "invalid character class at byte 1: empty character class",
        ),
        (
            RegexError::DfaTooLarge { limit: 256 },
            "determinized automaton exceeds 256 states",
        ),
        (
            RegexError::TooManyClasses { limit: 32 },
            "pattern needs more than 32 byte equivalence classes",
        ),
        (
            RegexError::ThreadLimitExceeded { limit: 256 },
            "matcher exceeded its budget of 256 threads",
        ),
    ];

    for (error, expected) in cases {
        assert_eq!(error.to_string(), expected);
        // The error is `Copy` and `Debug`, so a caller can log it and keep it.
        let copied = error;
        assert_eq!(copied, error);
        assert!(!format!("{error:?}").is_empty());
    }
}

/// The error type is re-exported at the crate root under the same name.
#[test]
fn the_error_type_is_re_exported() {
    let error: PublicError = Regex::new("(").unwrap_err();
    assert!(matches!(error, RegexError::Syntax { .. }));
}

/// With the `std` feature the error is a `std::error::Error`.
#[cfg(feature = "std")]
#[test]
fn the_error_implements_the_standard_error_trait() {
    let error = Regex::new("(").unwrap_err();
    let boxed: Box<dyn std::error::Error> = Box::new(error);
    assert!(boxed.to_string().contains("syntax error"));
}

// ---------------------------------------------------------------------------
// The inline vector
// ---------------------------------------------------------------------------

/// The inline vector behaves like a bounded vector.
#[test]
fn the_inline_vector_behaves_like_a_vector() {
    let mut items: SmallVec<u8, 4> = SmallVec::new();
    assert!(items.is_empty());
    assert_eq!(items.len(), 0);
    assert_eq!(items.capacity(), 4);

    assert!(items.push(b'a'));
    assert!(items.push(b'b'));
    assert_eq!(items.len(), 2);
    assert!(!items.is_empty());
    assert_eq!(items.as_slice(), b"ab");

    assert_eq!(items.get(0), Some(&b'a'));
    assert_eq!(items.get(1), Some(&b'b'));
    assert_eq!(items.get(2), None);
    assert_eq!(items.get(99), None);

    assert!(items.contains(&b'a'));
    assert!(!items.contains(&b'z'));

    let collected: Vec<u8> = items.iter().copied().collect();
    assert_eq!(collected, vec![b'a', b'b']);

    items.as_mut_slice()[0] = b'z';
    assert_eq!(items.as_slice(), b"zb");

    items.clear();
    assert!(items.is_empty());
    assert_eq!(items.len(), 0);
}

/// The inline vector refuses to grow past its capacity.
#[test]
fn the_inline_vector_refuses_to_grow() {
    let mut items: SmallVec<u8, 2> = SmallVec::new();
    assert!(items.push(1));
    assert!(items.push(2));
    assert!(!items.push(3));
    assert_eq!(items.len(), 2);
    assert_eq!(items.as_slice(), &[1, 2]);
}

/// The inline vector compares, prints and defaults.
#[test]
fn the_inline_vector_compares_and_prints() {
    let left: SmallVec<u8, 4> = SmallVec::from_iter([1u8, 2, 3]);
    let right: SmallVec<u8, 4> = SmallVec::from_iter([1u8, 2, 3]);
    let other: SmallVec<u8, 4> = SmallVec::from_iter([1u8, 2]);

    assert_eq!(left, right);
    assert_ne!(left, other);
    assert_eq!(format!("{left:?}"), "[1, 2, 3]");

    let empty: SmallVec<u8, 4> = SmallVec::default();
    assert!(empty.is_empty());
}

/// Collecting into an inline vector stops at the capacity.
#[test]
fn collecting_into_the_inline_vector_stops_at_the_capacity() {
    let items: SmallVec<u8, 3> = SmallVec::from_iter([1u8, 2, 3, 4, 5]);
    assert_eq!(items.len(), 3);
    assert_eq!(items.as_slice(), &[1, 2, 3]);
}

/// The inline vector round trips through JSON.
#[cfg(feature = "serde")]
#[test]
fn the_inline_vector_round_trips_through_json() {
    let items: SmallVec<u8, 4> = SmallVec::from_iter([1u8, 2, 3]);
    let text = serde_json::to_string(&items).expect("the vector serializes");
    assert_eq!(text, "[1,2,3]");

    let restored: SmallVec<u8, 4> = serde_json::from_str(&text).expect("the vector deserializes");
    assert_eq!(restored, items);
}

/// A sequence that is too long is refused, not truncated.
#[cfg(feature = "serde")]
#[test]
fn an_over_long_sequence_is_refused() {
    let result: Result<SmallVec<u8, 2>, _> = serde_json::from_str("[1,2,3]");
    assert!(result.is_err());
}

// ---------------------------------------------------------------------------
// The specification helpers
// ---------------------------------------------------------------------------

/// Every validation family has a name, and the names are unique.
#[test]
fn every_validation_family_has_a_unique_name() {
    assert_eq!(VALIDATION_KINDS.len(), 31);
    let mut names: Vec<&str> = Vec::new();
    for kind in VALIDATION_KINDS {
        let name = validation_kind_name(kind);
        assert!(!name.is_empty());
        assert!(!names.contains(&name), "the name {name} is used twice");
        names.push(name);
    }
}

/// Every extraction family has a name, and the names are unique.
#[test]
fn every_extraction_family_has_a_unique_name() {
    assert_eq!(EXTRACTION_KINDS.len(), 10);
    let mut names: Vec<&str> = Vec::new();
    for kind in EXTRACTION_KINDS {
        let name = extraction_kind_name(kind);
        assert!(!name.is_empty());
        assert!(!names.contains(&name), "the name {name} is used twice");
        names.push(name);
    }
}

/// The name of a candidate names its family.
#[test]
fn a_candidate_names_its_family() {
    let validation = CandidateSpec::Validate {
        kind: ValidationKind::BrazilianCpf,
        strictness: Strictness::Standard,
        charset: CharsetPolicy::Digits,
        max_len: Some(14),
    };
    assert_eq!(validation.kind_name(), "brazilian_cpf");

    let extraction = CandidateSpec::Extract {
        kind: ExtractionKind::BetweenDelimiters,
        boundary: BoundaryPolicy::Greedy,
        capture: CapturePolicy::Group,
    };
    assert_eq!(extraction.kind_name(), "between_delimiters");

    let filter = CandidateSpec::Filter {
        action: FilterAction::Block,
        terms: SmallVec::new(),
    };
    assert_eq!(filter.kind_name(), "filter");

    let custom = CandidateSpec::Custom {
        template: PatternTemplate::default(),
        params: PatternParams::default(),
    };
    assert_eq!(custom.kind_name(), "custom");
}

/// The default candidate is the canonical e-mail validation.
#[test]
fn the_default_candidate_is_the_canonical_email_validation() {
    let default = CandidateSpec::default();
    match default {
        CandidateSpec::Validate {
            kind, strictness, ..
        } => {
            assert_eq!(kind, ValidationKind::Email);
            assert_eq!(strictness, Strictness::Standard);
        }
        other => panic!("unexpected default: {other:?}"),
    }
}

/// A term keeps its bytes and reports them as text when they are valid UTF-8.
#[test]
fn a_term_keeps_its_bytes() {
    let literal = PatternTerm::literal(b"spam");
    assert_eq!(literal.kind, TermKind::Literal);
    assert_eq!(literal.as_str(), Some("spam"));

    let word = PatternTerm::word(b"spam");
    assert_eq!(word.kind, TermKind::Word);
    assert_eq!(word.as_str(), Some("spam"));

    // A phrase is matched case-insensitively, so it is stored lower-cased.
    let phrase = PatternTerm::phrase(b"Spam");
    assert_eq!(phrase.kind, TermKind::Phrase);
    assert_eq!(phrase.as_str(), Some("spam"));

    // Bytes that are not valid UTF-8 have no text form.
    let invalid = PatternTerm::literal(&[0xff, 0xfe]);
    assert_eq!(invalid.as_str(), None);
}

/// The default template pieces are neutral.
#[test]
fn the_default_template_pieces_are_neutral() {
    let piece = TemplatePiece::default();
    assert_eq!(piece, TemplatePiece::Literal(0));

    let template = PatternTemplate::default();
    match template {
        PatternTemplate::Sequence(pieces) => assert!(pieces.is_empty()),
        other => panic!("unexpected default: {other:?}"),
    }
}

/// An intent reports whether it settled on one specification.
#[test]
fn an_intent_reports_whether_it_is_confident() {
    let candidate = CandidateSpec::default();
    let confident = IntentSpec::Confident(candidate);
    assert!(confident.is_confident());
    assert!(confident.candidate().is_some());

    let ambiguous = IntentSpec::Ambiguous {
        reason: AmbiguityReason::LowConfidence,
        suggestions: SmallVec::new(),
    };
    assert!(!ambiguous.is_confident());
    assert!(ambiguous.candidate().is_none());
}

// ---------------------------------------------------------------------------
// The builder and the compiled regex
// ---------------------------------------------------------------------------

/// The builder carries the flags into the compiled regex.
#[test]
fn the_builder_carries_the_flags() {
    let regex = RegexBuilder::new("^a.b$")
        .case_insensitive(true)
        .multi_line(true)
        .dot_matches_newline(true)
        .build()
        .expect("the pattern compiles");

    let flags = regex.flags();
    assert!(flags.case_insensitive);
    assert!(flags.multi_line);
    assert!(flags.dot_matches_newline);
    assert_eq!(regex.pattern(), "^a.b$");
    assert!(regex.has_asserts());
    assert_eq!(regex.group_count(), 0);
}

/// The builder can be reached from the type itself.
#[test]
fn the_builder_can_be_reached_from_the_type() {
    let regex = Regex::builder()
        .pattern("abc")
        .case_insensitive(true)
        .build()
        .expect("the pattern compiles");
    assert!(regex.is_match(b"ABC"));

    // The pattern can be replaced, not only set once.
    let replaced = Regex::builder().pattern("abc").pattern("def").build();
    assert_eq!(replaced.expect("the pattern compiles").pattern(), "def");
}

/// A pattern that is too long is refused, and the limit is reported.
#[test]
fn an_over_long_pattern_is_refused() {
    let pattern = "a".repeat(1024);
    match Regex::new(&pattern) {
        Err(RegexError::PatternTooLong { limit, actual }) => {
            assert_eq!(limit, regie_core::MAX_PATTERN_LEN);
            assert!(actual > limit);
        }
        other => panic!("expected a length error, got {other:?}"),
    }
}

/// A regex compiled from a specification has no pattern text.
#[test]
fn a_regex_compiled_from_a_specification_has_no_pattern_text() {
    let spec = CandidateSpec::Validate {
        kind: ValidationKind::Email,
        strictness: Strictness::Standard,
        charset: CharsetPolicy::Any,
        max_len: None,
    };
    let regex = Regex::from_spec(&spec).expect("the specification compiles");
    assert_eq!(regex.pattern(), "");
    assert_eq!(regex.group_count(), 0);
}

/// The compiled regex prints its pattern.
#[test]
fn the_compiled_regex_prints_its_pattern() {
    let regex = Regex::new("a+b").expect("the pattern compiles");
    let text = format!("{regex:?}");
    assert!(text.contains("a+b"), "{text}");
    assert!(text.contains("groups"), "{text}");
}

/// The match span reports its length.
#[test]
fn the_match_span_reports_its_length() {
    let empty = Match { start: 4, end: 4 };
    assert!(empty.is_empty());
    assert_eq!(empty.len(), 0);

    let full = Match { start: 4, end: 9 };
    assert!(!full.is_empty());
    assert_eq!(full.len(), 5);
}

/// The captures report their groups, and the ones that did not take part.
#[test]
fn the_captures_report_their_groups() {
    let regex = Regex::new(r"(a)(b)?").expect("the pattern compiles");
    let captures: Captures = regex.captures(b"a").expect("the pattern matches");

    assert_eq!(captures.len(), 3);
    assert!(!captures.is_empty());
    assert!(captures.get(0).is_some());
    assert!(captures.get(1).is_some());
    // The optional group did not take part.
    assert_eq!(captures.get(2), None);
    // A group beyond the ones the pattern declares is not reported.
    assert_eq!(captures.get(3), None);
    assert_eq!(captures.iter().count(), 3);
}

// ---------------------------------------------------------------------------
// The parser rejects
// ---------------------------------------------------------------------------

/// Malformed patterns are refused with a message that names the problem.
#[test]
fn malformed_patterns_are_refused() {
    let cases: &[(&str, &str)] = &[
        ("a)", "unbalanced ')'"),
        ("(", "unclosed group"),
        ("[abc", "unclosed character class"),
        ("[]", "empty character class"),
        ("[z-a]", "character range is reversed"),
        ("*a", "nothing to repeat"),
        ("a**", "repeated quantifier"),
        (
            "a{3,2}",
            "repetition upper bound is smaller than the lower bound",
        ),
        ("\\", "trailing backslash"),
        ("\\q", "unknown escape sequence"),
        ("\\x", "truncated hexadecimal escape"),
        ("\\xZZ", "invalid hexadecimal escape"),
        ("(?z)", "unrecognized group syntax"),
    ];

    for (pattern, message) in cases {
        match Regex::new(pattern) {
            Err(RegexError::Syntax { message: found, .. }) => {
                assert_eq!(found, *message, "for the pattern {pattern}");
            }
            Err(RegexError::InvalidClass { message: found, .. }) => {
                assert_eq!(found, *message, "for the pattern {pattern}");
            }
            other => panic!("expected a rejection of {pattern}, got {other:?}"),
        }
    }
}

/// The constructs that make matching unpredictable are refused by name.
#[test]
fn the_unsupported_constructs_are_refused_by_name() {
    let cases: &[(&str, &str)] = &[
        ("(?=x)", "look-ahead"),
        ("(?!x)", "look-ahead"),
        ("(?<=x)", "look-behind"),
        ("(?<!x)", "look-behind"),
        ("(?#a comment)", "comment group"),
        ("(?P<name>x)", "named group"),
        ("(?U)", "flag"),
        ("(?x)", "flag"),
        ("\\1", "backreference"),
        ("\\u0041", "\\u escape"),
        ("\\p{L}", "Unicode property class"),
    ];

    for (pattern, feature) in cases {
        match Regex::new(pattern) {
            Err(RegexError::Unsupported { feature: found, .. }) => {
                assert_eq!(found, *feature, "for the pattern {pattern}");
            }
            other => panic!("expected a rejection of {pattern}, got {other:?}"),
        }
    }
}

/// The budget of the build is enforced, and reported.
#[test]
fn the_budget_of_the_build_is_enforced() {
    // More capture groups than the build supports.
    let many_groups = "(a)(b)(c)(d)(e)(f)(g)(h)(i)";
    assert!(matches!(
        Regex::new(many_groups),
        Err(RegexError::TooManyGroups { .. })
    ));

    // More nesting than the build supports.  The groups have to be
    // non-capturing, otherwise the capture budget is reached first.
    let deep = format!("{}x{}", "(?:".repeat(64), ")".repeat(64));
    assert!(matches!(
        Regex::new(&deep),
        Err(RegexError::NestingTooDeep { .. })
    ));

    // A repetition bound larger than the build supports.
    assert!(matches!(
        Regex::new("a{1000}"),
        Err(RegexError::RepetitionTooLarge { .. })
    ));
    assert!(matches!(
        Regex::new("a{1,1000}"),
        Err(RegexError::RepetitionTooLarge { .. })
    ));
}

// ---------------------------------------------------------------------------
// The lowered syntax tree
// ---------------------------------------------------------------------------

/// The arena stores nodes and refuses to grow past its capacity.
#[test]
fn the_arena_stores_nodes_and_refuses_to_grow() {
    let mut arena = HirArena::new();
    assert!(arena.is_empty());
    assert_eq!(arena.len(), 0);

    let index = arena
        .push(HirNode::LiteralByte(b'a'))
        .expect("the node fits");
    assert_eq!(index, 0);
    assert_eq!(arena.len(), 1);
    assert!(!arena.is_empty());
    assert_eq!(arena.get(index), HirNode::LiteralByte(b'a'));

    for _ in 0..MAX_HIR_NODES - 1 {
        arena.push(HirNode::Empty).expect("the node fits");
    }
    assert_eq!(arena.len(), MAX_HIR_NODES);
    assert!(matches!(
        arena.push(HirNode::Empty),
        Err(RegexError::ProgramTooLarge { .. })
    ));
}

/// The arena defaults to an empty one.
#[test]
fn the_arena_defaults_to_an_empty_one() {
    let arena = HirArena::default();
    assert!(arena.is_empty());
}

/// The assertions are the ones the parser can produce.
#[test]
fn the_assertions_are_the_ones_the_parser_produces() {
    let kinds = [
        AssertKind::StartText,
        AssertKind::EndText,
        AssertKind::StartLine,
        AssertKind::EndLine,
        AssertKind::WordBoundary,
        AssertKind::NotWordBoundary,
    ];
    for kind in kinds {
        let mut arena = HirArena::new();
        let index = arena.push(HirNode::Assert(kind)).expect("the node fits");
        assert_eq!(arena.get(index), HirNode::Assert(kind));
    }
}

// ---------------------------------------------------------------------------
// The automaton
// ---------------------------------------------------------------------------

/// A fresh automaton matches nothing.
#[test]
fn a_fresh_automaton_matches_nothing() {
    let dfa = Dfa::new();
    assert_eq!(dfa.state_count(), 2);
    assert_eq!(dfa.class_count(), 1);
    assert!(!dfa.is_match(b"anything"));
    assert_eq!(dfa.find_end(b"anything"), None);
    assert!(!dfa.accepts_at_end_only());

    let default = Dfa::default();
    assert_eq!(default.state_count(), 2);
}

/// The automaton is rebuilt from its raw parts.
#[test]
fn the_automaton_is_rebuilt_from_its_raw_parts() {
    let mut byte_class = [0u8; 256];
    byte_class[b'a' as usize] = 1;
    byte_class[b'b' as usize] = 1;

    let dfa = Dfa::from_parts(2, 3, &byte_class, &[0b0000_0100], &[0, 0, 0, 2, 0, 0])
        .expect("the parts fit");

    assert_eq!(dfa.state_count(), 3);
    assert_eq!(dfa.class_count(), 2);
    assert_eq!(dfa.class_of(b'a'), 1);
    assert_eq!(dfa.class_of(b'c'), 0);
    assert!(dfa.is_accept(2));
    assert!(!dfa.is_accept(1));
    // A state beyond the automaton is not accepting.
    assert!(!dfa.is_accept(99));
    assert_eq!(dfa.transition(1, 1), 2);
    // A state or a class beyond the automaton leads to the dead state.
    assert_eq!(dfa.transition(99, 1), 0);
    assert_eq!(dfa.transition(1, 99), 0);
    assert_eq!(dfa.transitions().len(), 3 * 2);
    assert_eq!(dfa.accept_bitmap().len(), 1);
    assert_eq!(dfa.byte_classes().len(), 256);
    assert!(dfa.is_match(b"a"));
    assert_eq!(dfa.find_end(b"a"), Some(1));
}

/// The end anchor is part of the automaton.
#[test]
fn the_end_anchor_is_part_of_the_automaton() {
    let mut byte_class = [0u8; 256];
    byte_class[b'a' as usize] = 1;

    let mut dfa = Dfa::from_parts(2, 4, &byte_class, &[0b0000_0100], &[0, 0, 0, 2, 3, 3, 3, 3])
        .expect("the parts fit");
    assert!(!dfa.accepts_at_end_only());

    dfa.set_accept_at_end(true);
    assert!(dfa.accepts_at_end_only());
    assert!(dfa.is_match(b"a"));
    assert!(!dfa.is_match(b"a "));
}

/// The constructor refuses the parts that do not fit.
#[test]
fn the_constructor_refuses_the_parts_that_do_not_fit() {
    let byte_class = [0u8; 256];

    // Too many states.
    assert!(matches!(
        Dfa::from_parts(1, MAX_DFA_STATES as u16 + 1, &byte_class, &[], &[]),
        Err(RegexError::DfaTooLarge { .. })
    ));

    // Too many classes.
    assert!(matches!(
        Dfa::from_parts(MAX_CLASSES as u16 + 1, 2, &byte_class, &[], &[]),
        Err(RegexError::TooManyClasses { .. })
    ));

    // A truncated accept bitmap.
    assert!(matches!(
        Dfa::from_parts(1, 16, &byte_class, &[0], &[]),
        Err(RegexError::Syntax { .. })
    ));

    // A truncated transition table.
    assert!(matches!(
        Dfa::from_parts(2, 4, &byte_class, &[0], &[0, 0, 0, 0]),
        Err(RegexError::Syntax { .. })
    ));
}

/// The automaton prints its shape.
#[test]
fn the_automaton_prints_its_shape() {
    let dfa = Dfa::new();
    let text = format!("{dfa:?}");
    assert!(text.contains("states"), "{text}");
    assert!(text.contains("classes"), "{text}");
}
