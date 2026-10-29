//! Property based tests.
//!
//! These do not check a specific answer; they check the invariants that must
//! hold for every pattern and every input.

use proptest::prelude::*;
use regie_core::Regex;

fn pattern_strategy() -> impl Strategy<Value = String> {
    let atom = prop::sample::select(vec!["a", "b", "[a-c]", "[^a]", "\\d", "\\w", ".", "^", "$"]);
    let quantifier = prop::sample::select(vec!["", "", "", "*", "+", "?", "{1,3}", "{2}"]);
    let piece =
        (atom.clone(), quantifier).prop_map(|(atom, quantifier)| format!("{atom}{quantifier}"));
    let group = (atom.clone(), atom.clone()).prop_map(|(left, right)| format!("({left}|{right})"));
    proptest::collection::vec(prop_oneof![piece, group], 0..6).prop_map(|parts| parts.join(""))
}

fn input_strategy() -> impl Strategy<Value = Vec<u8>> {
    proptest::collection::vec(
        prop_oneof![
            Just(b'a'),
            Just(b'b'),
            Just(b'c'),
            Just(b'0'),
            Just(b' '),
            Just(b'\n')
        ],
        0..24,
    )
}

proptest! {
    #[test]
    fn compiling_never_panics(pattern in pattern_strategy()) {
        // The only requirement is that the call returns.
        let _ = Regex::new(&pattern);
    }

    #[test]
    fn matching_never_panics(pattern in pattern_strategy(), input in input_strategy()) {
        if let Ok(re) = Regex::new(&pattern) {
            let _ = re.is_match(&input);
            let _ = re.find(&input);
            let _ = re.captures(&input);
        }
    }

    #[test]
    fn find_agrees_with_is_match(pattern in pattern_strategy(), input in input_strategy()) {
        let re = Regex::new(&pattern).unwrap();
        let found = re.find(&input);
        prop_assert_eq!(found.is_some(), re.is_match(&input));
        if let Some(hit) = found {
            prop_assert!(hit.start <= hit.end);
            prop_assert!(hit.end <= input.len());
        }
    }

    #[test]
    fn captures_agree_with_find(pattern in pattern_strategy(), input in input_strategy()) {
        let re = Regex::new(&pattern).unwrap();
        let caps = re.captures(&input);
        match (caps, re.find(&input)) {
            (Some(caps), Some(hit)) => {
                let whole = caps.get(0).unwrap();
                prop_assert_eq!((whole.start, whole.end), (hit.start, hit.end));
                for index in 1..caps.len() {
                    if let Some(group) = caps.get(index) {
                        prop_assert!(group.start <= group.end);
                        prop_assert!(group.end <= input.len());
                    }
                }
            }
            (None, None) => {}
            _ => prop_assert!(false, "captures and find disagree"),
        }
    }

    #[test]
    fn a_match_is_always_reproducible(pattern in pattern_strategy(), input in input_strategy()) {
        let re = Regex::new(&pattern).unwrap();
        let first = re.find(&input);
        let second = re.find(&input);
        prop_assert_eq!(first, second);
    }
}
