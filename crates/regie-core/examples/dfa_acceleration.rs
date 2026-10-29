//! Building the deterministic automaton, and comparing its verdict with the
//! verdict of the Pike VM on the same inputs.
//!
//! Run with:
//!
//! ```text
//! cargo run -p regie-core --example dfa_acceleration --features alloc
//! ```

#[cfg(feature = "alloc")]
fn main() {
    automaton::run();
}

#[cfg(not(feature = "alloc"))]
fn main() {
    println!("this example needs the `alloc` feature:");
    println!("cargo run -p regie-core --example dfa_acceleration --features alloc");
}

#[cfg(feature = "alloc")]
mod automaton {
    use regie_core::Regex;

    /// The inputs every pattern is tested against.
    const INPUTS: [&str; 8] = [
        "555-1234",
        "5551234",
        "x555-1234",
        "ana@corp.com",
        "aab",
        "b",
        "a cat",
        "",
    ];

    /// One pattern, with the inputs it is tested against.
    const CASES: [(&str, &[&str]); 4] = [
        (r"^[0-9]{3}-[0-9]{4}$", &INPUTS),
        (r"[a-z]+@[a-z]+\.[a-z]{2,}", &INPUTS),
        ("a+b", &INPUTS),
        (r"\bcat\b", &INPUTS),
    ];

    /// Builds the automaton of every pattern and compares the two engines.
    pub fn run() {
        for (pattern, inputs) in CASES {
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
                    println!("  the Pike VM still answers: {}", verdict(&re, inputs));
                    continue;
                }
            };
            println!(
                "  automaton: {} states, {} byte classes",
                dfa.state_count(),
                dfa.class_count()
            );
            for input in inputs {
                let vm = re.is_match(input.as_bytes());
                let automaton = dfa.is_match(input.as_bytes());
                let verdict = if vm == automaton { "agree" } else { "DIFFER" };
                println!(
                    "  {input:?} -> pike vm: {}, automaton: {} ({verdict})",
                    yes_no(vm),
                    yes_no(automaton)
                );
            }
        }
    }

    /// The verdict of the Pike VM on every input.
    fn verdict(re: &Regex, inputs: &[&str]) -> String {
        let mut out = String::new();
        for input in inputs {
            if re.is_match(input.as_bytes()) {
                out.push_str(&format!("{input:?} matches; "));
            }
        }
        if out.is_empty() {
            out.push_str("no input matches");
        }
        out.trim_end_matches("; ").to_owned()
    }

    /// A verdict as a word.
    fn yes_no(value: bool) -> &'static str {
        if value {
            "match"
        } else {
            "no match"
        }
    }
}
