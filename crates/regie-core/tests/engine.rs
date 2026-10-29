//! Behavioural tests for the `regie-core` engine.

use regie_core::spec::{CandidateSpec, CharsetPolicy, ExtractionKind, Strictness, ValidationKind};
use regie_core::{Captures, Match, Regex, RegexError};

fn m(pattern: &str, haystack: &[u8]) -> Option<Match> {
    let re = Regex::new(pattern).expect("pattern should compile");
    re.find(haystack)
}

fn matched(pattern: &str, haystack: &[u8]) -> bool {
    let re = Regex::new(pattern).expect("pattern should compile");
    re.is_match(haystack)
}

#[test]
fn literal_search() {
    assert!(matched("abc", b"xxabcxx"));
    assert!(!matched("abc", b"xxabxx"));
    let hit = m("abc", b"xxabcxx").expect("should match");
    assert_eq!((hit.start, hit.end), (2, 5));
}

#[test]
fn empty_pattern_matches_everything() {
    assert!(matched("", b""));
    assert!(matched("", b"abc"));
    let hit = m("", b"abc").expect("should match");
    assert_eq!((hit.start, hit.end), (0, 0));
}

#[test]
fn anchors() {
    assert!(matched("^abc", b"abc"));
    assert!(!matched("^abc", b"xabc"));
    assert!(matched("abc$", b"abc"));
    assert!(!matched("abc$", b"abcx"));
    assert!(matched("^abc$", b"abc"));
    assert!(!matched("^abc$", b"abcd"));
}

#[test]
fn character_classes() {
    assert!(matched("[a-z]+", b"abc"));
    // The class stops at the uppercase letter, but the match still exists.
    assert!(matched("[a-z]+", b"abC"));
    let hit = m("[a-z]+", b"abC").expect("should match");
    assert_eq!((hit.start, hit.end), (0, 2));
    assert!(!matched("[a-z]+", b"AB"));
    assert!(matched("[^a-z]+", b"ABC"));
    assert!(matched(r"\d+", b"12345"));
    assert!(matched(r"\d+", b"12a45"));
    assert!(matched(r"\w+", b"a_1"));
    assert!(matched(r"\s+", b"  \t"));
    assert!(!matched(r"\S+", b"  "));
    assert!(matched(r"[\d\-]+", b"12-34"));
}

#[test]
fn alternation_prefers_the_left_branch() {
    // Leftmost-first: the first alternative wins even when it is shorter.
    let hit = m("a|ab", b"ab").expect("should match");
    assert_eq!((hit.start, hit.end), (0, 1));
    // ... but a longer first alternative wins over a shorter second one.
    let hit = m("ab|a", b"ab").expect("should match");
    assert_eq!((hit.start, hit.end), (0, 2));
}

#[test]
fn alternation_prefers_the_leftmost_start() {
    // The second branch starts earlier, so it wins even though the first
    // branch matches earlier in time.
    let hit = m("c|abcd", b"xabcd").expect("should match");
    assert_eq!((hit.start, hit.end), (1, 5));
}

#[test]
fn quantifiers_are_greedy_by_default() {
    let hit = m("<.+>", b"<a><b>").expect("should match");
    assert_eq!((hit.start, hit.end), (0, 6));
    // The lazy variant stops as early as possible.
    let hit = m("<.+?>", b"<a><b>").expect("should match");
    assert_eq!((hit.start, hit.end), (0, 3));
}

#[test]
fn counted_repetition() {
    assert!(matched("a{3}", b"aaa"));
    assert!(!matched("a{3}", b"aa"));
    assert!(matched("a{2,}", b"aaaa"));
    assert!(!matched("a{2,}", b"a"));
    assert!(matched("a{2,4}", b"aaa"));
    assert!(!matched("a{2,4}", b"a"));
    assert!(matched("a{2,4}", b"aaaaa"));
}

#[test]
fn captures_report_group_spans() {
    let re = Regex::new(r"(\w+)@(\w+)\.com").expect("should compile");
    let caps: Captures = re.captures(b"mail ana@corp.com now").expect("should match");
    assert_eq!(caps.len(), 3);
    let whole = caps.get(0).expect("group 0");
    assert_eq!((whole.start, whole.end), (5, 17));
    let local = caps.get(1).expect("group 1");
    assert_eq!((local.start, local.end), (5, 8));
    let domain = caps.get(2).expect("group 2");
    assert_eq!((domain.start, domain.end), (9, 13));
}

#[test]
fn optional_group_that_does_not_participate_is_none() {
    let re = Regex::new(r"(a)?b").expect("should compile");
    let caps = re.captures(b"b").expect("should match");
    assert!(caps.get(1).is_none());
    assert_eq!(caps.get(0).map(|g| (g.start, g.end)), Some((0, 1)));
}

#[test]
fn nested_captures() {
    let re = Regex::new(r"((a)(b))c").expect("should compile");
    let caps = re.captures(b"abc").expect("should match");
    assert_eq!(caps.get(1).map(|g| (g.start, g.end)), Some((0, 2)));
    assert_eq!(caps.get(2).map(|g| (g.start, g.end)), Some((0, 1)));
    assert_eq!(caps.get(3).map(|g| (g.start, g.end)), Some((1, 2)));
}

#[test]
fn word_boundaries() {
    assert!(matched(r"\bcat\b", b"a cat here"));
    assert!(!matched(r"\bcat\b", b"concatenate"));
    assert!(matched(r"\Bcat", b"concat"));
}

#[test]
fn case_insensitive_flag() {
    let re = Regex::builder()
        .pattern("hello")
        .case_insensitive(true)
        .build()
        .expect("should compile");
    assert!(re.is_match(b"HELLO"));
    assert!(!re.is_match(b"goodbye"));

    let re = Regex::new("(?i)hello").expect("should compile");
    assert!(re.is_match(b"Hello"));
}

#[test]
fn dot_does_not_match_newline_by_default() {
    assert!(!matched("a.b", b"a\nb"));
    let re = Regex::builder()
        .pattern("a.b")
        .dot_matches_newline(true)
        .build()
        .expect("should compile");
    assert!(re.is_match(b"a\nb"));
}

#[test]
fn multi_line_flag() {
    let re = Regex::builder()
        .pattern("^b$")
        .multi_line(true)
        .build()
        .expect("should compile");
    assert!(re.is_match(b"a\nb\nc"));
}

#[test]
fn unsupported_constructs_are_rejected() {
    for pattern in [
        "(?=foo)",
        "(?!foo)",
        r"(?<=foo)",
        r"(?<!foo)",
        r"\1",
        r"(a)\1",
    ] {
        match Regex::new(pattern) {
            Err(RegexError::Unsupported { .. }) => {}
            Err(other) => panic!("{pattern} should be unsupported, got {other:?}"),
            Ok(_) => panic!("{pattern} should not compile"),
        }
    }
}

#[test]
fn malformed_patterns_are_rejected() {
    for pattern in ["[a-", "(abc", "abc)", "*abc", "a**", "[]", "[z-a]"] {
        assert!(Regex::new(pattern).is_err(), "{pattern} should not compile");
    }
}

#[test]
fn long_patterns_are_rejected() {
    let pattern = "a".repeat(regie_core::MAX_PATTERN_LEN + 1);
    match Regex::new(&pattern) {
        Err(RegexError::PatternTooLong { .. }) => {}
        other => panic!("expected PatternTooLong, got {other:?}"),
    }
}

#[test]
fn no_catastrophic_backtracking() {
    // A classic catastrophic pattern: the engine must still answer quickly.
    let re = Regex::new(r"^(a+)+$").expect("should compile");
    let haystack = vec![b'a'; 40];
    let haystack = [haystack, vec![b'b']].concat();
    assert!(!re.is_match(&haystack));
}

#[test]
fn specification_compiles_into_a_working_regex() {
    let spec = CandidateSpec::Validate {
        kind: ValidationKind::BrazilianPostalCode,
        strictness: Strictness::Standard,
        charset: CharsetPolicy::Digits,
        max_len: Some(9),
    };
    let re = Regex::from_spec(&spec).expect("spec should compile");
    assert!(re.is_match(b"01310-100"));
    assert!(re.is_match(b"01310100"));
    assert!(!re.is_match(b"0131-0100"));
    assert!(!re.is_match(b"abcdefg"));
}

#[test]
fn specification_validation_is_anchored() {
    let spec = CandidateSpec::Validate {
        kind: ValidationKind::Email,
        strictness: Strictness::Standard,
        charset: CharsetPolicy::Any,
        max_len: Some(254),
    };
    let re = Regex::from_spec(&spec).expect("spec should compile");
    assert!(re.is_match(b"ana@corp.com"));
    assert!(!re.is_match(b"mail ana@corp.com"));
}

#[test]
fn extraction_specification_captures_the_inner_text() {
    let spec = CandidateSpec::Extract {
        kind: ExtractionKind::BetweenDelimiters,
        boundary: regie_core::spec::BoundaryPolicy::Greedy,
        capture: regie_core::spec::CapturePolicy::Group,
    };
    let re = Regex::from_spec(&spec).expect("spec should compile");
    let caps = re.captures(b"value [42] tail").expect("should match");
    let inner = caps.get(1).expect("group 1");
    assert_eq!(&b"value [42] tail"[inner.start..inner.end], b"42");
}

#[test]
fn every_validation_family_compiles() {
    for kind in regie_core::spec::VALIDATION_KINDS {
        let spec = CandidateSpec::Validate {
            kind,
            strictness: Strictness::Standard,
            charset: CharsetPolicy::Any,
            max_len: None,
        };
        assert!(
            Regex::from_spec(&spec).is_ok(),
            "{kind:?} should compile into an automaton"
        );
    }
}

#[test]
fn every_extraction_family_compiles() {
    for kind in regie_core::spec::EXTRACTION_KINDS {
        let spec = CandidateSpec::Extract {
            kind,
            boundary: regie_core::spec::BoundaryPolicy::Greedy,
            capture: regie_core::spec::CapturePolicy::Group,
        };
        assert!(
            Regex::from_spec(&spec).is_ok(),
            "{kind:?} should compile into an automaton"
        );
    }
}

#[test]
fn the_full_set_of_capture_groups_is_accepted() {
    // A concatenation is compiled from the right, so the highest numbered
    // group arrives first.  The limit belongs to the group being emitted and
    // not to the number of groups emitted so far.
    let re = Regex::new("(a)(b)(c)(d)(e)(f)(g)(h)").expect("eight groups should compile");
    let caps = re.captures(b"abcdefgh").expect("should match");
    assert_eq!(caps.len(), 9);
    assert_eq!(re.group_count(), 8);
    assert_eq!(caps.get(1).map(|g| (g.start, g.end)), Some((0, 1)));
    assert_eq!(caps.get(8).map(|g| (g.start, g.end)), Some((7, 8)));
}

#[test]
fn a_ninth_capture_group_is_refused() {
    let pattern = "(a)(b)(c)(d)(e)(f)(g)(h)(i)";
    match Regex::new(pattern) {
        Err(RegexError::TooManyGroups { limit }) => {
            assert_eq!(limit, regie_core::MAX_CAPTURE_GROUPS);
        }
        other => panic!("expected TooManyGroups, got {other:?}"),
    }
}

#[test]
fn a_scoped_flag_group_does_not_leak() {
    // The group is case-insensitive; the `C` that follows it is not.
    let re = Regex::new("a(?i:b)C").expect("should compile");
    assert!(re.is_match(b"abC"));
    assert!(!re.is_match(b"aBc"));
}

#[test]
fn a_scoped_multi_line_flag_does_not_leak() {
    // The `^` inside the group is a line anchor; the one after it is a text
    // anchor, so the pattern cannot match the second line.
    let re = Regex::new("(?m:^a\n)^b").expect("should compile");
    assert!(!re.is_match(b"a\nb"));

    // The directive form applies to the rest of the group, so this one does
    // match the second line.
    let re = Regex::new("(?m)^a\n^b").expect("should compile");
    assert!(re.is_match(b"a\nb"));
}

#[test]
fn a_flag_directive_applies_to_the_rest_of_the_group() {
    let re = Regex::new("a(?i)b").expect("should compile");
    assert!(re.is_match(b"aB"));
    assert!(!re.is_match(b"Ab"));
}
