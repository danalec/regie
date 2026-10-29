//! The two engines must agree.
//!
//! The Pike VM simulates the NFA of a pattern; the deterministic automaton
//! walks the same pattern with a table. They are two answers to one question,
//! so on every subject they have to give the same verdict and the same end
//! of the match. This program builds the automaton of four hard patterns and
//! compares the two engines on every subject: the verdict of one against the
//! verdict of the other, and the end of the match of one against the end of
//! the match of the other.
//!
//! Run with:
//!
//! ```text
//! cargo run -p regie-core --example dfa_vs_nfa --features alloc
//! ```

#[cfg(feature = "alloc")]
fn main() {
    engines::run();
}

#[cfg(not(feature = "alloc"))]
fn main() {
    println!("this example needs the `alloc` feature:");
    println!("cargo run -p regie-core --example dfa_vs_nfa --features alloc");
}

#[cfg(feature = "alloc")]
mod engines {
    use regie_core::{Dfa, Regex};

    /// The subjects of the first pattern: a match, a non-match, a prefix of a
    /// match and a longer string with the match inside it.
    const RUNS: [&str; 4] = ["aaaa", "aaab", "aa", "xx aaaa"];

    /// The subjects of the second pattern.
    const MAILS: [&str; 4] = [
        "ana@corp.io",
        "ana@corp",
        "ana@corp.i",
        "write to ana@corp.io today",
    ];

    /// The subjects of the third pattern.
    const DATES: [&str; 4] = ["2026-10-29", "2026-10", "2026-10-2", "on 2026-10-29 ok"];

    /// The subjects of the fourth pattern.
    const PHRASES: [&str; 4] = [
        "one two three",
        "one two",
        "one two thr",
        "one two three four five",
    ];

    /// One pattern, with the subjects it is tested against.
    const CASES: [(&str, &[&str]); 4] = [
        (r"(a|aa)+$", &RUNS),
        (r"[a-z]+@[a-z]+\.[a-z]{2,}", &MAILS),
        (r"(\d{4})-(\d{2})-(\d{2})", &DATES),
        (r"^(?:[a-z]+ ){2,4}[a-z]+$", &PHRASES),
    ];

    /// Builds the automaton of every pattern and compares the two engines.
    pub fn run() {
        for (pattern, subjects) in CASES {
            let re = match Regex::new(pattern) {
                Ok(re) => re,
                Err(err) => {
                    println!("pattern {pattern}: could not compile: {err}");
                    continue;
                }
            };
            println!("pattern {pattern}:");
            let dfa = match re.build_dfa() {
                Ok(dfa) => dfa,
                Err(err) => {
                    println!("  no automaton: {err}");
                    for subject in subjects {
                        println!("  {subject:?} -> pike vm: {}", pike(&re, subject));
                    }
                    continue;
                }
            };
            println!(
                "  automaton: {} states, {} byte classes",
                dfa.state_count(),
                dfa.class_count()
            );
            println!(
                "  {:<30} {:<14} {:<16} {:<8} END",
                "SUBJECT", "PIKE VM", "AUTOMATON", "VERDICT"
            );
            for subject in subjects {
                row(&re, &dfa, subject);
            }
        }
    }

    /// Prints one subject: what the two engines answer, and whether they
    /// agree on the verdict and on the end of the match.
    fn row(re: &Regex, dfa: &Dfa, subject: &str) {
        let bytes = subject.as_bytes();
        let vm_verdict = re.is_match(bytes);
        let automaton_verdict = dfa.is_match(bytes);
        let vm_end = re.find(bytes).map(|span| span.end);
        let automaton_end = dfa.find_end(bytes);
        let verdict = if vm_verdict == automaton_verdict {
            "agree"
        } else {
            "DIFFER"
        };
        let end = if vm_end == automaton_end {
            "agree"
        } else {
            "DIFFER"
        };
        println!(
            "  {:<30} {:<14} {:<16} {:<8} {end}",
            format!("{subject:?}"),
            pike(re, subject),
            automaton(dfa, subject),
            verdict
        );
    }

    /// What the Pike VM answers: the span of the match, if there is one.
    fn pike(re: &Regex, subject: &str) -> String {
        match re.find(subject.as_bytes()) {
            Some(span) => format!("match {}..{}", span.start, span.end),
            None => "no match".to_owned(),
        }
    }

    /// What the automaton answers: the end of the match, if there is one.
    fn automaton(dfa: &Dfa, subject: &str) -> String {
        match dfa.find_end(subject.as_bytes()) {
            Some(end) => format!("match end {end}"),
            None => "no match".to_owned(),
        }
    }
}
