//! The `show` subcommand.
//!
//! The subcommand does what the other subcommands do and adds the view they
//! leave out: it interprets the request, compiles the specification and
//! prints the haystack with the part that matched underlined.
//!
//! With `--watch` it re-reads a file and prints the view again every time
//! the file changes, which is what a log file or a template needs.

use crate::render;
use crate::{CliError, EXIT_NO_MATCH, EXIT_OK};
use regie_core::spec::{CandidateSpec, IntentSpec};
use regie_core::{Captures, Match, Regex};
use regie_intent::IntentModel;
use regie_search::{search, Examples};
use std::time::Duration;

/// The arguments of the `show` subcommand.
pub struct ShowOptions<'a> {
    /// The request, in natural language.
    pub request: &'a str,
    /// The haystack, when it is given on the command line.
    pub input: Option<&'a str>,
    /// The values the specification has to accept.
    pub positive: &'a [String],
    /// The values the specification has to reject.
    pub negative: &'a [String],
    /// Whether the output may carry colour.
    pub color: bool,
    /// The file to watch, when the haystack comes from one.
    pub watch: Option<&'a str>,
}

/// Runs the `show` subcommand.
///
/// Returns [`EXIT_OK`] when the haystack matched, and [`EXIT_NO_MATCH`] when
/// it did not, when the request stayed ambiguous, or when the file is being
/// watched.
pub fn run_show(options: &ShowOptions<'_>) -> Result<u8, CliError> {
    match options.watch {
        Some(path) => watch(path, options),
        None => {
            let haystack = match options.input {
                Some(input) => input.to_owned(),
                None => {
                    return Err(CliError::Usage(
                        "missing <haystack>: `show` needs `--input` or `--watch`".to_owned(),
                    ))
                }
            };
            let matched = render(options, &haystack)?;
            Ok(if matched { EXIT_OK } else { EXIT_NO_MATCH })
        }
    }
}

/// Prints the view of one haystack and reports whether it matched.
fn render(options: &ShowOptions<'_>, haystack: &str) -> Result<bool, CliError> {
    let outcome = interpret(options);

    println!("{} {}", label("request:", options.color), options.request);

    match &outcome {
        IntentSpec::Confident(spec) => {
            println!(
                "{} {}",
                label("intent:", options.color),
                render::intent_summary(&outcome)
            );

            let regex = Regex::from_spec(spec).map_err(CliError::Spec)?;
            let found = regex.find(haystack.as_bytes());
            print_haystack(haystack, found, options.color);

            match found {
                Some(_) => {
                    if let Some(captures) = regex.captures(haystack.as_bytes()) {
                        print_groups(&captures, haystack, options.color);
                    }
                    Ok(true)
                }
                None => {
                    // A validation is anchored, so a value inside a sentence
                    // is not a match.  Say so, instead of leaving the reader
                    // with a bare "no match".
                    if matches!(spec, CandidateSpec::Validate { .. }) {
                        println!(
                            "{}",
                            paint(
                                "note: a validation is anchored - the whole haystack has to be the value",
                                render::DIM,
                                options.color
                            )
                        );
                    }
                    Ok(false)
                }
            }
        }
        IntentSpec::Ambiguous {
            reason,
            suggestions,
        } => {
            println!(
                "{} {}",
                label("intent:", options.color),
                render::intent_summary(&outcome)
            );
            println!(
                "{} the model did not settle on one specification ({reason:?})",
                label("note:", options.color)
            );
            for (index, suggestion) in suggestions.iter().enumerate() {
                println!("  {}. {}", index + 1, suggestion.kind_name());
            }
            Ok(false)
        }
    }
}

/// Interprets the request, with the search when there are examples.
fn interpret(options: &ShowOptions<'_>) -> IntentSpec {
    if options.positive.is_empty() && options.negative.is_empty() {
        return IntentModel::embedded().interpret(options.request);
    }

    let positives: Vec<&str> = options
        .positive
        .iter()
        .map(|value| value.as_str())
        .collect();
    let negatives: Vec<&str> = options
        .negative
        .iter()
        .map(|value| value.as_str())
        .collect();
    search(options.request, Examples::new(&positives, &negatives)).outcome
}

/// Prints the haystack with the part that matched underlined.
fn print_haystack(haystack: &str, found: Option<Match>, color: bool) {
    let bytes = haystack.as_bytes();
    let (start, end) = match found {
        Some(span) => (span.start.min(bytes.len()), span.end.min(bytes.len())),
        None => (0, 0),
    };

    let before = String::from_utf8_lossy(&bytes[..start]);
    let inside = String::from_utf8_lossy(&bytes[start..end]);
    let after = String::from_utf8_lossy(&bytes[end..]);

    println!(
        "{} {}{}{}",
        label("haystack:", color),
        before,
        paint(&inside, render::GREEN, color),
        after
    );

    match found {
        Some(span) => {
            // The underline is only meaningful when the match stays on one
            // line; otherwise the span says enough.
            let column = before.rsplit('\n').next().map(count_chars).unwrap_or(0);
            let width = count_chars(&inside);
            if !inside.contains('\n') && width > 0 {
                println!(
                    "{}{} {}",
                    " ".repeat(column),
                    paint(&"^".repeat(width), render::GREEN, color),
                    paint(
                        &format!("matched {}..{}", span.start, span.end),
                        render::DIM,
                        color
                    )
                );
            } else {
                println!(
                    "{}",
                    paint(
                        &format!("matched {}..{}", span.start, span.end),
                        render::DIM,
                        color
                    )
                );
            }
        }
        None => {
            println!("{}", paint("no match", render::RED, color));
        }
    }
}

/// Prints the capture groups of a match.
fn print_groups(captures: &Captures, haystack: &str, color: bool) {
    let bytes = haystack.as_bytes();
    println!("{}", label("groups:", color));
    for index in 0..captures.len() {
        match captures.get(index) {
            Some(span) => {
                let text = String::from_utf8_lossy(&bytes[span.start..span.end]);
                println!(
                    "  {index}: {}..{} {}",
                    span.start,
                    span.end,
                    render::quote_bytes(text.as_bytes())
                );
            }
            None => println!("  {index}: <no match>"),
        }
    }
}

/// The number of characters of a string.
fn count_chars(text: &str) -> usize {
    text.chars().count()
}

/// Paints a label in dim letters.
fn label(text: &str, color: bool) -> String {
    paint(text, render::DIM, color)
}

/// Wraps a text in an escape code, when the output may carry colour.
fn paint(text: &str, code: &str, color: bool) -> String {
    if color {
        format!("{code}{text}{}", render::RESET)
    } else {
        text.to_owned()
    }
}

/// Watches a file and prints the view again every time it changes.
fn watch(path: &str, options: &ShowOptions<'_>) -> Result<u8, CliError> {
    let mut stamp: Option<(u64, u64)> = None;
    let mut first = true;

    loop {
        let metadata = std::fs::metadata(path).map_err(|error| CliError::Io {
            path: path.to_owned(),
            source: error,
        })?;
        let current = (
            metadata.len(),
            metadata
                .modified()
                .map(|time| {
                    time.duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_nanos() as u64)
                        .unwrap_or(0)
                })
                .unwrap_or(0),
        );

        if stamp != Some(current) {
            if !first {
                let separator = format!("--- {path} ---");
                println!("{}", paint(&separator, render::DIM, options.color));
            }
            stamp = Some(current);
            first = false;

            let content = std::fs::read_to_string(path).map_err(|error| CliError::Io {
                path: path.to_owned(),
                source: error,
            })?;
            render(options, &content)?;
        }

        std::thread::sleep(Duration::from_millis(250));
    }
}
