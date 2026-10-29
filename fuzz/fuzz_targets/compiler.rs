//! Fuzzes the compiler and the matcher.
//!
//! The first half of the input is the pattern, the second half is the
//! haystack. The engine must answer every request without panicking and
//! without hanging.

#![no_main]

use libfuzzer_sys::fuzz_target;
use regie_core::Regex;

fuzz_target!(|data: &[u8]| {
    if data.is_empty() {
        return;
    }
    let split = data.len() / 2;
    let (pattern_bytes, haystack) = data.split_at(split);
    let Ok(pattern) = std::str::from_utf8(pattern_bytes) else {
        return;
    };
    let Ok(re) = Regex::new(pattern) else {
        return;
    };
    let matched = re.is_match(haystack);
    let found = re.find(haystack);
    assert_eq!(matched, found.is_some(), "is_match and find disagree");
    if let Some(hit) = found {
        assert!(hit.start <= hit.end);
        assert!(hit.end <= haystack.len());
    }
    let _ = re.captures(haystack);
});
