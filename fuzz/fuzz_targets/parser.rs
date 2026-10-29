//! Fuzzes the pattern parser.
//!
//! The parser is the boundary that decides what the engine is willing to
//! accept, so it has to reject everything it does not understand without
//! panicking.

#![no_main]

use libfuzzer_sys::fuzz_target;
use regie_core::Regex;

fuzz_target!(|data: &[u8]| {
    let Ok(pattern) = std::str::from_utf8(data) else {
        return;
    };
    // The result is either a compiled pattern or an error; both are fine.
    let _ = Regex::new(pattern);
});
