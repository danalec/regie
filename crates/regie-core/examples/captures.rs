//! Reading the groups of a match: the whole match, the numbered groups, a
//! group that did not take part, nested groups, and the span of a group inside
//! the subject.
//!
//! Run with:
//!
//! ```text
//! cargo run -p regie-core --example captures
//! ```

use regie_core::Regex;

fn main() {
    whole_match_and_groups();
    optional_group();
    nested_groups();
    span_inside_the_subject();
}

/// Group `0` is the whole match; the groups above it are numbered in the
/// order they appear in the pattern.
fn whole_match_and_groups() {
    let re = compile(r"(\w+)@(\w+)\.com");
    let subject = b"write to ana@corp.com today";
    println!("pattern: {}", re.pattern());
    println!("subject: {}", show(subject));
    show_groups(&re, subject);
}

/// A group that is not taken part in has no span at all.
fn optional_group() {
    let re = compile(r"(\d{4})-(\d{2})(?:-(\d{2}))?");
    for subject in [b"2026-10".as_slice(), b"2026-10-29".as_slice()] {
        println!("subject: {}", show(subject));
        show_groups(&re, subject);
    }
}

/// A nested group is numbered by its opening parenthesis.
fn nested_groups() {
    let re = compile(r"((ab)(cd))");
    let subject = b"abcd";
    println!("pattern: {}", re.pattern());
    println!("subject: {}", show(subject));
    show_groups(&re, subject);
}

/// The span of a group is a pair of byte offsets into the subject.
fn span_inside_the_subject() {
    let re = compile(r"(\w+)@(\w+)\.com");
    let subject = b"write to ana@corp.com today";
    match re.captures(subject) {
        Some(caps) => match caps.get(1) {
            Some(span) => {
                let text = &subject[span.start..span.end];
                println!("group 1 is {}..{}", span.start, span.end);
                println!("its text is {}", show(text));
            }
            None => println!("group 1 did not take part"),
        },
        None => println!("no match"),
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

/// Prints every group of the match, in order.
fn show_groups(re: &Regex, subject: &[u8]) {
    match re.captures(subject) {
        Some(caps) => {
            for index in 0..=re.group_count() {
                match caps.get(index) {
                    Some(span) => {
                        let text = &subject[span.start..span.end];
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

/// The text of a byte slice, quoted so that the control bytes stay readable.
fn show(bytes: &[u8]) -> String {
    format!("{:?}", String::from_utf8_lossy(bytes))
}
