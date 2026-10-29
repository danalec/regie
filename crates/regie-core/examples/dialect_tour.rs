//! Every construct the dialect accepts, one line each, with an input it
//! matches and an input it refuses.
//!
//! Run with:
//!
//! ```text
//! cargo run -p regie-core --example dialect_tour
//! ```

use regie_core::Regex;

/// One construct of the dialect, with the pattern that spells it, an input it
/// matches and an input it does not.
const ROWS: [(&str, &str, &str, &str); 36] = [
    ("literal", "abc", "abc", "abd"),
    ("class", "[a-z]", "m", "M"),
    ("range", "[0-9]", "7", "x"),
    ("negated class", "[^0-9]", "a", "5"),
    ("digit class", r"\d", "42", "a"),
    ("not a digit", r"\D", "ab", "7"),
    ("word class", r"\w", "a_1", "-"),
    ("not a word", r"\W", "-", "a"),
    ("space class", r"\s", " ", "x"),
    ("not a space", r"\S", "x", " "),
    ("start anchor", "^abc", "abcdef", "xabc"),
    ("end anchor", "abc$", "xxabc", "abcx"),
    ("word boundary", r"\bcat\b", "a cat here", "concatenate"),
    ("not a boundary", r"\Bcat", "concat", " cat"),
    ("capturing group", "(ab)+", "abab", "ba"),
    ("plain group", "(?:ab)+", "abab", "ba"),
    ("alternation", "cat|dog", "dog", "cow"),
    ("zero or more", "ab*", "a", "c"),
    ("one or more", "ab+", "ab", "a"),
    ("optional", "ab?", "a", "b"),
    ("exactly n", "a{3}", "aaa", "aa"),
    ("at least n", "a{2,}", "aaa", "a"),
    ("between n and m", "a{2,3}", "aa", "a"),
    ("greedy", "a+", "aaa", "b"),
    ("lazy", "a+?", "aaa", "b"),
    ("flag i", "(?i)abc", "ABC", "abd"),
    ("flag i scoped", "(?i:abc)d", "ABCd", "ABCD"),
    ("flag m", "(?m)^b$", "a\nb", "a\nbc"),
    ("flag m scoped", "(?m:^b$)", "a\nb", "a\nbb"),
    ("dot", "a.c", "abc", "a\nc"),
    ("flag s", "(?s)a.c", "a\nc", "ac"),
    ("flag s scoped", "(?s:a.c).", "a\ncX", "a\nc\n"),
    ("escape n", r"a\nb", "a\nb", "ab"),
    ("escape r", r"a\rb", "a\rb", "ab"),
    ("escape t", r"a\tb", "a\tb", "ab"),
    ("escape xHH", r"\x41", "A", "a"),
];

fn main() {
    tour();
    greedy_and_lazy();
}

/// Prints one line per construct.
fn tour() {
    println!(
        "{:<18} {:<12} {:<24} REFUSES",
        "CONSTRUCT", "PATTERN", "ACCEPTS"
    );
    for (construct, pattern, accepts_input, refuses_input) in ROWS {
        let re = match Regex::new(pattern) {
            Ok(re) => re,
            Err(err) => {
                println!("{construct:<18} {pattern:<12} refused to compile: {err}");
                continue;
            }
        };
        let accepts = verdict(&re, accepts_input);
        let refuses = verdict(&re, refuses_input);
        println!("{construct:<18} {pattern:<12} {accepts:<24} {refuses}");
    }
}

/// The verdict of the matcher on one input, quoted so that the control bytes
/// of the escape examples stay readable.
fn verdict(re: &Regex, input: &str) -> String {
    let result = if re.is_match(input.as_bytes()) {
        "match"
    } else {
        "no match"
    };
    format!("{input:?} -> {result}")
}

/// The greedy and the lazy quantifier take different spans of the same input.
fn greedy_and_lazy() {
    println!();
    println!("greedy and lazy take different spans:");
    let subject = b"<b><i>";
    for pattern in [r"<.+>", r"<.+?>"] {
        let re = match Regex::new(pattern) {
            Ok(re) => re,
            Err(err) => {
                println!("{pattern:<8} could not compile: {err}");
                continue;
            }
        };
        let span = match re.find(subject) {
            Some(span) => span,
            None => {
                println!("{pattern:<8} no match");
                continue;
            }
        };
        let text = String::from_utf8_lossy(&subject[span.start..span.end]);
        println!("{pattern:<8} matches {text:?}");
    }
}
