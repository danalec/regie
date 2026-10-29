//! Compiling the four families of structured specification, with no pattern
//! text anywhere.
//!
//! Run with:
//!
//! ```text
//! cargo run -p regie-core --example specifications
//! ```

use regie_core::spec::{
    BoundaryPolicy, CandidateSpec, CapturePolicy, CharsetPolicy, ExtractionKind, FilterAction,
    PatternParams, PatternTemplate, PatternTerm, Strictness, ValidationKind,
};
use regie_core::{Regex, SmallVec};

/// One specification, with an input it accepts and an input it refuses.
type Case = (&'static str, CandidateSpec, &'static str, &'static str);

fn main() {
    let cases: [Case; 4] = [
        (
            "validate",
            CandidateSpec::Validate {
                kind: ValidationKind::Email,
                strictness: Strictness::Standard,
                charset: CharsetPolicy::Any,
                max_len: None,
            },
            "ana@corp.com",
            "not-an-email",
        ),
        (
            "extract",
            CandidateSpec::Extract {
                kind: ExtractionKind::Quoted,
                boundary: BoundaryPolicy::Greedy,
                capture: CapturePolicy::Group,
            },
            "he said \"hello there\"",
            "no quotes here",
        ),
        (
            "filter",
            CandidateSpec::Filter {
                action: FilterAction::Block,
                terms: terms(&[b"spam"]),
            },
            "buy spam now",
            "buy soap now",
        ),
        (
            "custom",
            CandidateSpec::Custom {
                template: PatternTemplate::LiteralThenClass {
                    literal: bytes(b"ID-"),
                    class: CharsetPolicy::Digits,
                    count: 4,
                },
                params: PatternParams {
                    anchored_start: true,
                    anchored_end: true,
                    ..PatternParams::default()
                },
            },
            "ID-1234",
            "ID-12",
        ),
    ];

    println!("{:<10} {:<14} {:<30} REFUSES", "FAMILY", "KIND", "ACCEPTS");
    for (family, spec, accepts, refuses) in &cases {
        let re = match Regex::from_spec(spec) {
            Ok(re) => re,
            Err(err) => {
                eprintln!("could not compile the {family} specification: {err}");
                std::process::exit(1);
            }
        };
        let accepts = verdict(&re, accepts);
        let refuses = verdict(&re, refuses);
        println!(
            "{:<10} {:<14} {:<30} {}",
            family,
            spec.kind_name(),
            accepts,
            refuses
        );
    }

    extracted_group(&cases[1].1);
}

/// The extraction family puts the extracted part in the first group.
///
/// The action of a filter and the capture policy of an extraction do not
/// change the automaton: the automaton only says whether the input matches,
/// and the caller decides what to do with the answer.
fn extracted_group(spec: &CandidateSpec) {
    let re = match Regex::from_spec(spec) {
        Ok(re) => re,
        Err(err) => {
            eprintln!("could not compile the specification: {err}");
            std::process::exit(1);
        }
    };
    let subject = b"he said \"hello there\" and left";
    println!();
    println!("the extracted part:");
    let group = re.captures(subject).and_then(|caps| caps.get(1));
    match group {
        Some(span) => {
            let text = String::from_utf8_lossy(&subject[span.start..span.end]);
            println!("  group 1: {}..{} {:?}", span.start, span.end, text);
        }
        None => println!("  group 1 did not take part"),
    }
}

/// The verdict of the matcher on one input.
fn verdict(re: &Regex, input: &str) -> String {
    let result = if re.is_match(input.as_bytes()) {
        "match"
    } else {
        "no match"
    };
    format!("{input:?} -> {result}")
}

/// The bytes of a literal prefix.
fn bytes(text: &[u8]) -> SmallVec<u8, 12> {
    let mut out = SmallVec::new();
    for byte in text {
        if !out.push(*byte) {
            break;
        }
    }
    out
}

/// The terms of a filter.
fn terms(items: &[&[u8]]) -> SmallVec<PatternTerm, 4> {
    let mut out = SmallVec::new();
    for item in items {
        if !out.push(PatternTerm::literal(item)) {
            break;
        }
    }
    out
}
