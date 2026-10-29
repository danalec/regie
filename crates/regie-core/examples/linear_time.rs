//! The worst case is linear.
//!
//! The patterns of this program are the ones that make a backtracking engine
//! take exponential time on a subject of a few hundred bytes. This engine
//! simulates a Thompson NFA with a Pike VM instead of backtracking, so the
//! work is `O(m * n)` for a program of `m` instructions over a subject of `n`
//! bytes. Every case below ends, and the last two end on a subject of one
//! mebibyte.
//!
//! The program prints no timings on purpose. The point is not that the run is
//! fast, but that it finishes at all.
//!
//! Run with:
//!
//! ```text
//! cargo run -p regie-core --example linear_time
//! ```

use regie_core::Regex;

/// The patterns that make a backtracking engine explode.
const PATTERNS: [&str; 5] = ["(a+)+$", "(a|aa)+$", "(a|a)*$", "(a*)*b", "((a)|(a))*b"];

/// The lengths of the growing subjects.
const LENGTHS: [usize; 4] = [64, 256, 1024, 4096];

/// The size of the large subjects: one mebibyte of `a`.
const LARGE: usize = 1024 * 1024;

fn main() {
    growing_subjects();
    large_subjects();
}

/// Runs every pattern against every growing subject, twice: once with a
/// subject of `a` alone, once with a `b` at the end that the pattern has to
/// refuse.
fn growing_subjects() {
    println!("the patterns that break a backtracking engine:");
    header();
    for pattern in PATTERNS {
        let re = compile(pattern);
        for length in LENGTHS {
            let mut subject = vec![b'a'; length];
            row(&re, &subject, "all a");
            subject.push(b'b');
            row(&re, &subject, "all a + b");
        }
    }
}

/// Runs the same engine on a subject of one mebibyte.
fn large_subjects() {
    println!();
    println!("the same engine on a subject of one mebibyte:");
    header();
    let re = compile("a+$");
    let mut subject = vec![b'a'; LARGE];
    row(&re, &subject, "all a");
    subject.push(b'b');
    row(&re, &subject, "all a + b");
}

/// Prints the header of a table.
fn header() {
    println!(
        "{:<12} {:>9} {:<12} {:<10} SPAN",
        "PATTERN", "LENGTH", "SUBJECT", "VERDICT"
    );
}

/// Prints one row: the pattern, the length of the subject, the verdict and
/// the span of the match.
fn row(re: &Regex, subject: &[u8], shape: &str) {
    let (verdict, span) = match re.find(subject) {
        Some(span) => ("match", format!("{}..{}", span.start, span.end)),
        None => ("no match", "-".to_owned()),
    };
    println!(
        "{:<12} {:>9} {:<12} {:<10} {span}",
        re.pattern(),
        subject.len(),
        shape,
        verdict
    );
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
