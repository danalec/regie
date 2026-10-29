//! The hard parts of the dialect.
//!
//! Nested groups and the way they are numbered, the limit on the number of
//! groups, greedy against lazy, the precedence of the alternation, the
//! anchors with and without the multi-line flag, the scoped flag, the empty
//! match, the word boundary at the edges of the subject, the bounds of a
//! repetition, and the class the engine refuses.
//!
//! Every case is one row: the pattern, the subject, the verdict and the span
//! of the match, or the exact error the engine returns when it refuses the
//! pattern.
//!
//! Run with:
//!
//! ```text
//! cargo run -p regie-core --example hard_patterns
//! ```

use regie_core::Regex;

/// One case: what it is called, the pattern and the subject it runs on.
const CASES: [(&str, &str, &str); 19] = [
    ("nested groups", "((a)(b(c)))", "abc"),
    ("group limit, eight", "(a)(b)(c)(d)(e)(f)(g)(h)", "abcdefgh"),
    (
        "group limit, nine",
        "(a)(b)(c)(d)(e)(f)(g)(h)(i)",
        "abcdefghi",
    ),
    ("greedy", "<(.+)>", "<a><b>"),
    ("lazy", "<(.+?)>", "<a><b>"),
    ("alternation", "cat|dog", "cats"),
    ("grouped alternation", "(cat|dog)s?", "cats"),
    ("text anchor", "^b$", "a\nb"),
    ("line anchor", "(?m)^b$", "a\nb"),
    ("scoped flag", "(?i:abc)d", "ABCd"),
    ("scoped flag, other case", "(?i:abc)d", "ABCD"),
    ("global flag", "(?i)abc", "ABC"),
    ("empty match", "a*", "bbb"),
    ("word boundary", r"\bcat\b", "cat"),
    ("word boundary, in a word", r"\bcat\b", "concat"),
    ("zero repetitions", "a{0}", "aa"),
    ("bounded repetitions", "a{2,3}", "aa"),
    ("bounded repetitions, greedy", "a{2,3}", "aaaa"),
    ("reversed range", "[z-a]", "abc"),
];

fn main() {
    println!(
        "{:<28} {:<28} {:<12} {:<9} SPAN OR ERROR",
        "CONSTRUCT", "PATTERN", "SUBJECT", "VERDICT"
    );
    for (construct, pattern, subject) in CASES {
        row(construct, pattern, subject);
    }
    groups();
}

/// Prints one case: the verdict of the matcher and the span of the match, or
/// the error the engine returns for a pattern it refuses.
fn row(construct: &str, pattern: &str, subject: &str) {
    let quoted = format!("{subject:?}");
    match Regex::new(pattern) {
        Ok(re) => {
            let (verdict, span) = match re.find(subject.as_bytes()) {
                Some(span) => ("match", format!("{}..{}", span.start, span.end)),
                None => ("no match", "-".to_owned()),
            };
            println!("{construct:<28} {pattern:<28} {quoted:<12} {verdict:<9} {span}");
        }
        Err(err) => {
            println!(
                "{construct:<28} {pattern:<28} {quoted:<12} {:<9} {err}",
                "refused"
            );
        }
    }
}

/// Prints the groups of the nested pattern, one line each.
///
/// The group is numbered by its opening parenthesis, so the outermost group
/// is the first one and the innermost is the last.
fn groups() {
    println!();
    let re = compile("((a)(b(c)))");
    let subject = "abc";
    println!("the groups of {} on {subject:?}:", re.pattern());
    match re.captures(subject.as_bytes()) {
        Some(caps) => {
            for index in 0..=re.group_count() {
                match caps.get(index) {
                    Some(span) => {
                        let text = &subject.as_bytes()[span.start..span.end];
                        println!(
                            "  group {index}: {}..{} {}",
                            span.start,
                            span.end,
                            show(text)
                        );
                    }
                    None => println!("  group {index}: did not take part"),
                }
            }
        }
        None => println!("  no match"),
    }
}

/// Compiles a pattern, reporting the failure instead of panicking.
fn compile(pattern: &str) -> Regex {
    match Regex::new(pattern) {
        Ok(re) => re,
        Err(err) => {
            eprintln!("could not compile {pattern}: {err}");
            std::process::exit(1);
        }
    }
}

/// The text of a byte slice, quoted so that the control bytes stay readable.
fn show(bytes: &[u8]) -> String {
    format!("{:?}", String::from_utf8_lossy(bytes))
}
