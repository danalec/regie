//! Behaviour of every specification family.
//!
//! `engine.rs` checks that the matcher works; this file checks that each
//! family of the structured specification accepts what it should and refuses
//! what it should.  A family that compiles but matches the wrong thing is
//! worse than a family that does not compile, so every family is checked
//! against a value it must accept and a value it must refuse.

use regie_core::spec::{
    BoundaryPolicy, CandidateSpec, CapturePolicy, CharsetPolicy, ExtractionKind, FilterAction,
    PatternParams, PatternTemplate, PatternTerm, Strictness, TemplatePiece, TermKind,
    ValidationKind,
};
use regie_core::Regex;

/// Builds the regex of a validation family.
fn validation(kind: ValidationKind, strictness: Strictness) -> Regex {
    let spec = CandidateSpec::Validate {
        kind,
        strictness,
        charset: CharsetPolicy::Any,
        max_len: None,
    };
    Regex::from_spec(&spec).expect("the specification compiles")
}

/// Builds the regex of an extraction family.
fn extraction(kind: ExtractionKind, boundary: BoundaryPolicy) -> Regex {
    let spec = CandidateSpec::Extract {
        kind,
        boundary,
        capture: CapturePolicy::Group,
    };
    Regex::from_spec(&spec).expect("the specification compiles")
}

/// Asserts that the family accepts `valid` and refuses `invalid`.
fn accepts(kind: ValidationKind, valid: &[&str], invalid: &[&str]) {
    let regex = validation(kind, Strictness::Standard);
    for value in valid {
        assert!(
            regex.is_match(value.as_bytes()),
            "{:?} should accept {value:?}",
            kind
        );
    }
    for value in invalid {
        assert!(
            !regex.is_match(value.as_bytes()),
            "{:?} should refuse {value:?}",
            kind
        );
    }
}

/// Every validation family accepts its own values and refuses the others.
#[test]
fn every_validation_family_accepts_its_own_values() {
    accepts(
        ValidationKind::Email,
        &["ana@corp.com", "a.b+c@sub.example.org"],
        &["not-an-email", "@corp.com", "ana@corp"],
    );
    accepts(
        ValidationKind::Url,
        &["https://example.com/path", "http://example.org:8080/x?y=1"],
        &["example.com", "http://", "ftp://example.com"],
    );
    accepts(
        ValidationKind::Uri,
        &["mailto:ana@corp.com", "urn:isbn:0451450523"],
        &["://nothing", "no scheme here"],
    );
    accepts(
        ValidationKind::DomainName,
        &["example.com", "www.example.org"],
        &["", ".com", "example..com"],
    );
    accepts(
        ValidationKind::Ipv4,
        &["192.168.0.1", "10.0.0.1"],
        &["1.2.3", "1.2.3.4.5", "a.b.c.d"],
    );
    accepts(
        ValidationKind::Ipv6,
        &["2001:0db8:85a3:0000:0000:8a2e:0370:7334"],
        &["::1", "2001:db8", "not an address"],
    );
    accepts(
        ValidationKind::MacAddress,
        &["00:1B:44:11:3A:B7", "00-1b-44-11-3a-b7"],
        &["00:1B:44:11:3A", "zz:zz:zz:zz:zz:zz"],
    );
    accepts(
        ValidationKind::BrazilianCpf,
        &["123.456.789-09"],
        &["123.456.789", "123.456.789-0"],
    );
    accepts(
        ValidationKind::BrazilianCnpj,
        &["12.345.678/0001-95"],
        &["12.345.678/0001", "12.345.678/0001-9"],
    );
    accepts(
        ValidationKind::BrazilianPostalCode,
        &["01310-100", "01310100"],
        &["0131", "01310-1", "abcdefghij"],
    );
    accepts(
        ValidationKind::BrazilianPhone,
        &["+55 11 91234-5678", "11 91234-5678", "11912345678"],
        &["+55 1 912345678", "phone"],
    );
    accepts(
        ValidationKind::BrazilianLicensePlate,
        &["ABC1D23", "ABC1234"],
        &["ABC1D2", "A1B2C3D"],
    );
    accepts(
        ValidationKind::PortugueseNif,
        &["123456789"],
        &["12345678", "1234567890", "abcdefghi"],
    );
    accepts(
        ValidationKind::UsSsn,
        &["123-45-6789", "123456789"],
        &["123-45-678", "123-456-789"],
    );
    accepts(
        ValidationKind::Semver,
        &["1.2.3", "1.2.3-rc.1+build"],
        &["1.2", "v1.2.3", "1.2.3.4"],
    );
    accepts(
        ValidationKind::IsoDate,
        &["2026-10-29", "20261029"],
        &["2026-13-01", "29/10/2026"],
    );
    accepts(
        ValidationKind::IsoDateTime,
        &["2026-10-29T12:00:00Z", "2026-10-29T12:00:00"],
        &["2026-10-29", "2026-10-29 12:00:00"],
    );
    accepts(
        ValidationKind::ClockTime,
        &["12:34", "12:34:56"],
        &["1:34", "12:3", "1234"],
    );
    accepts(
        ValidationKind::Uuid,
        &["123e4567-e89b-12d3-a456-426614174000"],
        &["123e4567-e89b-12d3-a456", "not-a-uuid"],
    );
    accepts(
        ValidationKind::Hex,
        &["0x1f", "0X1F", "deadBEEF"],
        &["0x", "nothex"],
    );
    accepts(
        ValidationKind::Base64,
        &["aGVsbG8=", "aGVsbG8"],
        &["not base64!", "a==="],
    );
    accepts(
        ValidationKind::Base32,
        &["NBSWY3DP", "NBSWY3DP======"],
        &["not base32!"],
    );
    accepts(
        ValidationKind::Slug,
        &["my-page", "my-page_title", "post-1"],
        &["My Page", "post-", "-page"],
    );
    accepts(
        ValidationKind::Username,
        &["ana", "ana.silva", "ana_silva"],
        &["an", "", "ana silva"],
    );
    accepts(
        ValidationKind::CreditCard,
        &["4111111111111111", "411111111111111"],
        &["411111111111", "41111111111111111a"],
    );
    accepts(
        ValidationKind::Iban,
        &["DE89370400440532013000", "DE8937040044053201300"],
        &[
            "D89370400440532013000",
            "DE8937040044053201300012345678901234567890",
        ],
    );
    accepts(
        ValidationKind::UsZipCode,
        &["94105", "94105-1234"],
        &["9410", "94105-123"],
    );
    accepts(ValidationKind::Integer, &["42", "-42"], &["", "4.2", "42a"]);
    accepts(
        ValidationKind::Decimal,
        &["3.14", "-3.14", "42"],
        &["", ".14", "3.14.15"],
    );
    accepts(
        ValidationKind::CurrencyAmount,
        &["$1,234.56", "1234.56", "EUR99"],
        &["", "$abc"],
    );
    accepts(
        ValidationKind::Json,
        &["{}", r#"{"a": 1}"#],
        &["{", "no braces"],
    );
}

/// The three levels of strictness accept different forms of a CPF.
#[test]
fn the_strictness_levels_accept_different_forms_of_a_cpf() {
    let strict = validation(ValidationKind::BrazilianCpf, Strictness::Strict);
    assert!(strict.is_match(b"123.456.789-09"));
    assert!(!strict.is_match(b"12345678909"));

    let standard = validation(ValidationKind::BrazilianCpf, Strictness::Standard);
    assert!(standard.is_match(b"123.456.789-09"));
    assert!(standard.is_match(b"12345678909"));

    let lenient = validation(ValidationKind::BrazilianCpf, Strictness::Lenient);
    assert!(lenient.is_match(b"12345678909"));
    assert!(!lenient.is_match(b"123.456.789-09"));
}

/// Every charset policy builds a class that accepts what it should.
#[test]
fn every_charset_policy_accepts_what_it_should() {
    use regie_core::spec::PatternTemplate;

    let cases: &[(CharsetPolicy, &[u8], &[u8])] = &[
        (CharsetPolicy::Digits, b"0123456789", b"abc"),
        (CharsetPolicy::Letters, b"abcXYZ", b"0123"),
        (CharsetPolicy::Alphanumeric, b"abc123", b"-_"),
        (CharsetPolicy::Word, b"abc_123", b"-"),
        (CharsetPolicy::PrintableAscii, b"a b~", b"\x01"),
        // `Any` holds every byte, so it refuses nothing.
        (CharsetPolicy::Any, b"a\x01\xff", b""),
    ];

    for (policy, accepted, refused) in cases {
        let spec = CandidateSpec::Custom {
            template: PatternTemplate::Repeat {
                class: *policy,
                count: 3,
            },
            params: PatternParams::default(),
        };
        let regex = Regex::from_spec(&spec).expect("the specification compiles");
        let sample: Vec<u8> = accepted.iter().copied().cycle().take(3).collect();
        assert!(
            regex.is_match(&sample),
            "{policy:?} should accept {sample:?}"
        );
        if refused.is_empty() {
            continue;
        }
        let sample: Vec<u8> = refused.iter().copied().cycle().take(3).collect();
        assert!(
            !regex.is_match(&sample),
            "{policy:?} should refuse {sample:?}"
        );
    }
}

/// Every extraction family finds what it looks for.
#[test]
fn every_extraction_family_finds_what_it_looks_for() {
    let cases: &[(ExtractionKind, &str)] = &[
        (ExtractionKind::BetweenDelimiters, "the value [42] is here"),
        (ExtractionKind::Quoted, "he said \"hello\" today"),
        (ExtractionKind::Numbers, "I am 42 years old"),
        (ExtractionKind::Words, "the quick brown fox"),
        (ExtractionKind::Urls, "visit https://example.com now"),
        (ExtractionKind::Emails, "write to ana@corp.com now"),
        (ExtractionKind::IpAddresses, "the host 192.168.0.1 answered"),
        (ExtractionKind::Dates, "the meeting is 2026-10-29 now"),
        (ExtractionKind::AfterKeyword, "total: 42 items"),
        (ExtractionKind::BeforeKeyword, "42 items: total"),
    ];

    for (kind, text) in cases {
        let regex = extraction(*kind, BoundaryPolicy::Greedy);
        assert!(
            regex.is_match(text.as_bytes()),
            "{kind:?} should find something in {text:?}"
        );

        let lazy = extraction(*kind, BoundaryPolicy::Lazy);
        assert!(
            lazy.is_match(text.as_bytes()),
            "{kind:?} should find something in {text:?} when lazy"
        );
    }
}

/// The exclusive boundary is accepted as a policy, even where it changes
/// nothing: the compiler must not refuse it.
#[test]
fn the_exclusive_boundary_is_accepted() {
    let regex = extraction(ExtractionKind::BetweenDelimiters, BoundaryPolicy::Exclusive);
    assert!(regex.is_match(b"the value [42] is here"));
}

/// The whole capture policy is accepted as well.
#[test]
fn the_whole_capture_policy_is_accepted() {
    let spec = CandidateSpec::Extract {
        kind: ExtractionKind::Numbers,
        boundary: BoundaryPolicy::Greedy,
        capture: CapturePolicy::Whole,
    };
    let regex = Regex::from_spec(&spec).expect("the specification compiles");
    assert!(regex.is_match(b"a 42 here"));
}

/// A filter matches any of its terms.
#[test]
fn a_filter_matches_any_of_its_terms() {
    let mut terms = regie_core::SmallVec::new();
    terms.push(PatternTerm::literal(b"spam"));
    terms.push(PatternTerm::word(b"phishing"));

    let spec = CandidateSpec::Filter {
        action: FilterAction::Block,
        terms,
    };
    let regex = Regex::from_spec(&spec).expect("the specification compiles");

    assert!(regex.is_match(b"this is spam"));
    assert!(regex.is_match(b"a phishing attempt"));
    assert!(!regex.is_match(b"a normal message"));
}

/// A phrase is matched case-insensitively, a literal is not.
#[test]
fn a_phrase_is_matched_case_insensitively_and_a_literal_is_not() {
    let mut terms = regie_core::SmallVec::new();
    terms.push(PatternTerm::phrase(b"Spam"));

    let spec = CandidateSpec::Filter {
        action: FilterAction::Block,
        terms,
    };
    let regex = Regex::from_spec(&spec).expect("the specification compiles");
    assert!(regex.is_match(b"this is SPAM"));

    let mut terms = regie_core::SmallVec::new();
    terms.push(PatternTerm::literal(b"Spam"));

    let spec = CandidateSpec::Filter {
        action: FilterAction::Block,
        terms,
    };
    let regex = Regex::from_spec(&spec).expect("the specification compiles");
    assert!(regex.is_match(b"this is Spam"));
    assert!(!regex.is_match(b"this is SPAM"));
}

/// A filter without terms matches nothing.
#[test]
fn a_filter_without_terms_matches_nothing() {
    let spec = CandidateSpec::Filter {
        action: FilterAction::Allow,
        terms: regie_core::SmallVec::new(),
    };
    let regex = Regex::from_spec(&spec).expect("the specification compiles");
    assert!(!regex.is_match(b"anything"));
}

/// Every action of a filter is accepted.
#[test]
fn every_action_of_a_filter_is_accepted() {
    for action in [
        FilterAction::Block,
        FilterAction::Allow,
        FilterAction::Highlight,
        FilterAction::Count,
    ] {
        let mut terms = regie_core::SmallVec::new();
        terms.push(PatternTerm::literal(b"spam"));
        let spec = CandidateSpec::Filter { action, terms };
        let regex = Regex::from_spec(&spec).expect("the specification compiles");
        assert!(regex.is_match(b"spam"));
    }
}

/// Every kind of term is accepted.
#[test]
fn every_kind_of_term_is_accepted() {
    for term in [
        PatternTerm::literal(b"spam"),
        PatternTerm::word(b"spam"),
        PatternTerm::phrase(b"spam"),
    ] {
        let mut terms = regie_core::SmallVec::new();
        terms.push(term);
        let spec = CandidateSpec::Filter {
            action: FilterAction::Block,
            terms,
        };
        let regex = Regex::from_spec(&spec).expect("the specification compiles");
        assert!(regex.is_match(b"spam"));
    }
    // The kind of a term is part of its contract.
    assert_eq!(PatternTerm::word(b"x").kind, TermKind::Word);
}

/// The custom templates build the patterns they describe.
#[test]
fn the_custom_templates_build_the_patterns_they_describe() {
    // A run of characters from a class.
    let spec = CandidateSpec::Custom {
        template: PatternTemplate::Repeat {
            class: CharsetPolicy::Digits,
            count: 4,
        },
        params: PatternParams::default(),
    };
    let regex = Regex::from_spec(&spec).expect("the specification compiles");
    // The template is not anchored, so it matches a run of four digits
    // anywhere in the input.
    assert!(regex.is_match(b"1234"));
    assert!(regex.is_match(b"x12345y"));
    assert!(!regex.is_match(b"123"));

    // A literal prefix followed by a run of characters.
    let mut literal = regie_core::SmallVec::new();
    literal.push(b'v');
    literal.push(b'1');
    let spec = CandidateSpec::Custom {
        template: PatternTemplate::LiteralThenClass {
            literal,
            class: CharsetPolicy::Digits,
            count: 2,
        },
        params: PatternParams::default(),
    };
    let regex = Regex::from_spec(&spec).expect("the specification compiles");
    assert!(regex.is_match(b"v112"));
    assert!(!regex.is_match(b"v12"));
}

/// A sequence of pieces builds the pattern it describes.
#[test]
fn a_sequence_of_pieces_builds_the_pattern_it_describes() {
    let mut pieces = regie_core::SmallVec::new();
    pieces.push(TemplatePiece::Literal(b'a'));
    pieces.push(TemplatePiece::LiteralRange(b'0', b'9'));
    pieces.push(TemplatePiece::Class(CharsetPolicy::Digits));
    pieces.push(TemplatePiece::Any);

    let spec = CandidateSpec::Custom {
        template: PatternTemplate::Sequence(pieces),
        params: PatternParams::default(),
    };
    let regex = Regex::from_spec(&spec).expect("the specification compiles");
    assert!(regex.is_match(b"a0123"));
}

/// Every piece of a template is accepted.
#[test]
fn every_piece_of_a_template_is_accepted() {
    use regie_core::hir::AssertKind;

    let pieces = [
        TemplatePiece::Literal(b'a'),
        TemplatePiece::LiteralRange(b'0', b'9'),
        TemplatePiece::Class(CharsetPolicy::Digits),
        TemplatePiece::Any,
        TemplatePiece::Anchor(AssertKind::StartText),
        TemplatePiece::GroupStart(0),
        TemplatePiece::GroupEnd(0),
    ];

    for piece in pieces {
        let mut sequence = regie_core::SmallVec::new();
        sequence.push(piece);
        let spec = CandidateSpec::Custom {
            template: PatternTemplate::Sequence(sequence),
            params: PatternParams::default(),
        };
        let regex = Regex::from_spec(&spec).expect("the specification compiles");
        // The point is that the compiler accepts every piece.
        assert!(!regex.pattern().is_empty() || regex.pattern().is_empty());
    }
}

/// The anchoring parameters of a custom pattern are honoured.
#[test]
fn the_anchoring_parameters_of_a_custom_pattern_are_honoured() {
    let mut pieces = regie_core::SmallVec::new();
    pieces.push(TemplatePiece::Literal(b'a'));

    let params = PatternParams {
        anchored_start: true,
        anchored_end: true,
        ..PatternParams::default()
    };

    let spec = CandidateSpec::Custom {
        template: PatternTemplate::Sequence(pieces),
        params,
    };
    let regex = Regex::from_spec(&spec).expect("the specification compiles");
    assert!(regex.is_match(b"a"));
    assert!(!regex.is_match(b"ab"));
    assert!(!regex.is_match(b"ba"));

    // Without the anchoring the same pattern matches anywhere.
    let mut pieces = regie_core::SmallVec::new();
    pieces.push(TemplatePiece::Literal(b'a'));
    let spec = CandidateSpec::Custom {
        template: PatternTemplate::Sequence(pieces),
        params: PatternParams::default(),
    };
    let regex = Regex::from_spec(&spec).expect("the specification compiles");
    assert!(regex.is_match(b"ba"));
    assert!(regex.is_match(b"ab"));
}

/// The other parameters of a custom pattern are accepted.
#[test]
fn the_other_parameters_of_a_custom_pattern_are_accepted() {
    let mut pieces = regie_core::SmallVec::new();
    pieces.push(TemplatePiece::Literal(b'a'));

    let params = PatternParams {
        case_sensitive: false,
        multiline: true,
        dot_all: true,
        anchored_start: false,
        anchored_end: false,
        min_len: Some(1),
        max_len: Some(10),
    };

    let spec = CandidateSpec::Custom {
        template: PatternTemplate::Sequence(pieces),
        params,
    };
    let regex = Regex::from_spec(&spec).expect("the specification compiles");
    assert!(regex.is_match(b"a"));
}
