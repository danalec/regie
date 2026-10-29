//! Tests for the deterministic automaton.
//!
//! The DFA is the artifact that firmware embeds, so the tests check both
//! that it agrees with the Pike VM and that the anchors survive the
//! determinization.

#![cfg(feature = "alloc")]

use regie_core::Regex;

fn dfa_of(pattern: &str) -> regie_core::Dfa {
    let re = Regex::new(pattern).expect("pattern should compile");
    re.build_dfa().expect("dfa should build")
}

fn agrees(pattern: &str, inputs: &[&[u8]]) {
    let re = Regex::new(pattern).expect("pattern should compile");
    let dfa = re.build_dfa().expect("dfa should build");
    for input in inputs {
        assert_eq!(
            re.is_match(input),
            dfa.is_match(input),
            "dfa disagrees with the pike vm for {pattern:?} on {:?}",
            String::from_utf8_lossy(input)
        );
    }
}

#[test]
fn literal_dfa_finds_the_literal_anywhere() {
    let dfa = dfa_of("admin");
    assert!(dfa.is_match(b"the admin user"));
    assert!(dfa.is_match(b"admin"));
    assert!(!dfa.is_match(b"adm in"));
    assert!(!dfa.is_match(b""));
}

#[test]
fn dfa_agrees_with_the_pike_vm() {
    agrees(
        "admin",
        &[b"", b"admin", b"xxadminxx", b"adminx", b"admi", b"xx"],
    );
    agrees("a+b", &[b"", b"aab", b"ab", b"b", b"aabab", b"xxaab"]);
    agrees(
        "[0-9]{3}-[0-9]{2}",
        &[b"123-45", b"1234-5", b"abc", b"xx123-45xx"],
    );
    agrees("colou?r", &[b"color", b"colour", b"colouur", b"colr"]);
    agrees("cat|dog", &[b"cat", b"dog", b"cow", b"a dog here"]);
}

#[test]
fn anchored_patterns_keep_their_anchors() {
    agrees("^abc", &[b"abc", b"xabc", b"abcx", b"xxabc"]);
    agrees("abc$", &[b"abc", b"abcx", b"xabc"]);
    agrees("^abc$", &[b"abc", b"xabc", b"abcx", b"ab"]);
}

#[test]
fn anchored_dfa_accepts_only_at_the_end() {
    let dfa = dfa_of("^abc$");
    assert!(dfa.accepts_at_end_only());
    assert!(dfa.is_match(b"abc"));
    assert!(!dfa.is_match(b"abcd"));
}

#[test]
fn word_boundary_dfa_is_rejected() {
    // A word boundary cannot be expressed by a deterministic automaton
    // without look-around, so the build fails explicitly instead of
    // producing a wrong automaton.
    let re = Regex::new(r"\bcat\b").expect("pattern should compile");
    match re.build_dfa() {
        Err(regie_core::RegexError::Unsupported { .. }) => {}
        other => panic!("expected an unsupported error, got {other:?}"),
    }
}

#[test]
fn dfa_from_a_specification_is_anchored() {
    use regie_core::spec::{CandidateSpec, CharsetPolicy, Strictness, ValidationKind};
    let spec = CandidateSpec::Validate {
        kind: ValidationKind::BrazilianPostalCode,
        strictness: Strictness::Standard,
        charset: CharsetPolicy::Digits,
        max_len: Some(9),
    };
    let re = Regex::from_spec(&spec).expect("spec should compile");
    let dfa = re.build_dfa().expect("dfa should build");
    assert!(dfa.is_match(b"01310-100"));
    assert!(dfa.is_match(b"01310100"));
    assert!(!dfa.is_match(b"0131-0100"));
    assert!(!dfa.is_match(b"0131010"));
    assert!(!dfa.is_match(b"abcdefg"));
}

#[test]
fn dfa_state_count_stays_small() {
    // A handful of literal characters must not explode the automaton.
    let dfa = dfa_of("admin");
    assert!(
        dfa.state_count() <= 16,
        "unexpected state count: {}",
        dfa.state_count()
    );
    assert!(
        dfa.class_count() <= 8,
        "unexpected class count: {}",
        dfa.class_count()
    );
}
