//! # regie
//!
//! The command line front-end of the `regie` regular expression engine.
//!
//! The tool exposes the four layers of the project behind one binary:
//!
//! * `match` and `test` run a pattern against an input;
//! * `intent` turns a natural-language request into a structured
//!   specification;
//! * `search` verifies a request against examples;
//! * `dfa` and `decode` compile a pattern into a `regie-codec` artifact and
//!   match an input against such an artifact.
//!
//! ## Exit codes
//!
//! The tool is meant to sit in a pipeline, so the exit code carries the
//! verdict:
//!
//! | Code | Meaning |
//! |---|---|
//! | `0` | the command did what was asked |
//! | `1` | no match, or no answer the tool could settle on |
//! | `2` | usage error, I/O error, or an input the engine rejected |
//!
//! Every failure is reported as `error: <message>` on standard error; the
//! tool never panics on user input.

mod commands;
mod render;

use std::fmt;
use std::process::ExitCode;

/// The exit code of a command that did what was asked.
pub const EXIT_OK: u8 = 0;

/// The exit code of a command that found no match, or that could not settle
/// on a single answer.
pub const EXIT_NO_MATCH: u8 = 1;

/// The exit code of a command that failed on a usage, I/O or input error.
pub const EXIT_ERROR: u8 = 2;

/// The subcommands the tool understands.
const SUBCOMMANDS: [&str; 6] = ["match", "test", "intent", "search", "dfa", "decode"];

/// A failure reported by the tool.
///
/// The variants mirror the three ways a command can fail: the command line
/// could not be understood, an input could not be read, or the engine
/// rejected what it was given.
#[derive(Debug)]
pub enum CliError {
    /// The command line could not be understood.
    Usage(String),
    /// A pattern could not be compiled.
    Pattern {
        /// The pattern that was rejected.
        pattern: String,
        /// The error reported by `regie-core`.
        source: regie_core::RegexError,
    },
    /// A structured specification could not be compiled.
    Spec(regie_core::RegexError),
    /// A file could not be read or written.
    Io {
        /// The path involved.
        path: String,
        /// The underlying error.
        source: std::io::Error,
    },
    /// An artifact could not be encoded or decoded.
    Codec(regie_codec::CodecError),
}

impl fmt::Display for CliError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CliError::Usage(message) => f.write_str(message),
            CliError::Pattern { pattern, source } => {
                write!(f, "invalid pattern `{pattern}`: {source}")
            }
            CliError::Spec(source) => write!(f, "invalid specification: {source}"),
            CliError::Io { path, source } => write!(f, "{path}: {source}"),
            CliError::Codec(source) => write!(f, "{source}"),
        }
    }
}

/// The command the user asked for.
enum Command {
    /// Match a pattern against an input.
    Match {
        /// The pattern to compile.
        pattern: String,
        /// The text to match against.
        input: String,
    },
    /// Exit successfully when a pattern matches an input.
    Test {
        /// The pattern to compile.
        pattern: String,
        /// The text to match against.
        input: String,
    },
    /// Interpret a natural-language request.
    Intent {
        /// The request to interpret.
        request: String,
    },
    /// Verify a request against examples.
    Search {
        /// The request to interpret.
        request: String,
        /// The examples the outcome has to accept.
        positive: Vec<String>,
        /// The examples the outcome has to reject.
        negative: Vec<String>,
    },
    /// Compile a pattern into a DFA artifact.
    Dfa {
        /// The pattern to compile.
        pattern: String,
        /// Where the artifact is written.
        output: String,
    },
    /// Match an input against a DFA artifact.
    Decode {
        /// The artifact to read.
        artifact: String,
        /// The text to match against.
        input: String,
    },
    /// Print the usage block, optionally for one subcommand.
    Help(Option<String>),
    /// Print the version.
    Version,
}

fn main() -> ExitCode {
    // `args_os` never panics, unlike `args`, which rejects a non-UTF-8
    // argument.
    let args: Vec<String> = std::env::args_os()
        .skip(1)
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect();

    match run(&args) {
        Ok(code) => ExitCode::from(code),
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::from(EXIT_ERROR)
        }
    }
}

/// Runs the command described by `args` and returns the exit code.
fn run(args: &[String]) -> Result<u8, CliError> {
    match parse(args)? {
        Command::Help(topic) => {
            print_help(topic.as_deref());
            Ok(EXIT_OK)
        }
        Command::Version => {
            println!("regie {}", env!("CARGO_PKG_VERSION"));
            Ok(EXIT_OK)
        }
        Command::Match { pattern, input } => commands::matching::run_match(&pattern, &input),
        Command::Test { pattern, input } => commands::matching::run_test(&pattern, &input),
        Command::Intent { request } => commands::intent::run_intent(&request),
        Command::Search {
            request,
            positive,
            negative,
        } => commands::search::run_search(&request, &positive, &negative),
        Command::Dfa { pattern, output } => commands::artifact::run_dfa(&pattern, &output),
        Command::Decode { artifact, input } => commands::artifact::run_decode(&artifact, &input),
    }
}

/// Turns the command line into a [`Command`].
fn parse(args: &[String]) -> Result<Command, CliError> {
    let Some(first) = args.first() else {
        return Err(CliError::Usage(format!(
            "missing subcommand (expected one of: {})",
            SUBCOMMANDS.join(", ")
        )));
    };

    match first.as_str() {
        "-h" | "--help" => return Ok(Command::Help(None)),
        "-V" | "--version" => return Ok(Command::Version),
        _ => {}
    }

    let rest = &args[1..];
    match first.as_str() {
        "match" | "test" => {
            let positional = positionals(rest, first)?;
            if positional.help {
                return Ok(Command::Help(Some(first.clone())));
            }
            let (pattern, input) = exactly_two(&positional.values, first, "pattern", "input")?;
            if first == "match" {
                Ok(Command::Match { pattern, input })
            } else {
                Ok(Command::Test { pattern, input })
            }
        }
        "intent" => {
            let positional = positionals(rest, first)?;
            if positional.help {
                return Ok(Command::Help(Some(first.clone())));
            }
            let request = join_request(&positional.values, first)?;
            Ok(Command::Intent { request })
        }
        "search" => parse_search(rest),
        "dfa" => {
            let positional = positionals(rest, first)?;
            if positional.help {
                return Ok(Command::Help(Some(first.clone())));
            }
            let (pattern, output) = exactly_two(&positional.values, first, "pattern", "out.bin")?;
            Ok(Command::Dfa { pattern, output })
        }
        "decode" => {
            let positional = positionals(rest, first)?;
            if positional.help {
                return Ok(Command::Help(Some(first.clone())));
            }
            let (artifact, input) =
                exactly_two(&positional.values, first, "artifact.bin", "input")?;
            Ok(Command::Decode { artifact, input })
        }
        other => Err(CliError::Usage(format!(
            "unknown subcommand `{other}` (expected one of: {})",
            SUBCOMMANDS.join(", ")
        ))),
    }
}

/// The positional arguments of a subcommand.
struct Positional {
    /// The arguments that are not options.
    values: Vec<String>,
    /// Whether the user asked for the help of the subcommand.
    help: bool,
}

/// Collects the positional arguments of a subcommand.
///
/// A `--` ends the options: everything after it is an argument, even when it
/// starts with a dash. Every other option is rejected, so a typo never turns
/// into a silent argument.
fn positionals(args: &[String], topic: &str) -> Result<Positional, CliError> {
    let mut values = Vec::new();
    let mut literal = false;

    for arg in args {
        if literal {
            values.push(arg.clone());
            continue;
        }
        match arg.as_str() {
            "--" => literal = true,
            "-h" | "--help" => return Ok(Positional { values, help: true }),
            option if option.starts_with('-') && option.len() > 1 => {
                return Err(unknown_flag(option, topic));
            }
            other => values.push(other.to_string()),
        }
    }

    Ok(Positional {
        values,
        help: false,
    })
}

/// Parses the arguments of the `search` subcommand.
fn parse_search(args: &[String]) -> Result<Command, CliError> {
    let mut request: Vec<String> = Vec::new();
    let mut positive: Vec<String> = Vec::new();
    let mut negative: Vec<String> = Vec::new();
    let mut literal = false;
    let mut index = 0usize;

    while index < args.len() {
        let arg = args[index].as_str();
        index += 1;

        if literal {
            request.push(args[index - 1].clone());
            continue;
        }

        match arg {
            "--" => literal = true,
            "-h" | "--help" => return Ok(Command::Help(Some("search".to_string()))),
            "--positive" | "--negative" => {
                let Some(value) = args.get(index) else {
                    return Err(CliError::Usage(format!("`{arg}` needs a value")));
                };
                index += 1;
                push_example(arg, value, &mut positive, &mut negative)?;
            }
            other => {
                if let Some(value) = other.strip_prefix("--positive=") {
                    push_example("--positive", value, &mut positive, &mut negative)?;
                } else if let Some(value) = other.strip_prefix("--negative=") {
                    push_example("--negative", value, &mut positive, &mut negative)?;
                } else if other.starts_with('-') && other.len() > 1 {
                    return Err(unknown_flag(other, "search"));
                } else {
                    request.push(other.to_string());
                }
            }
        }
    }

    let request = join_request(&request, "search")?;
    if positive.is_empty() {
        return Err(CliError::Usage(
            "missing examples: `search` needs at least one `--positive` example".to_string(),
        ));
    }

    Ok(Command::Search {
        request,
        positive,
        negative,
    })
}

/// Stores one example, rejecting an empty value.
fn push_example(
    flag: &str,
    value: &str,
    positive: &mut Vec<String>,
    negative: &mut Vec<String>,
) -> Result<(), CliError> {
    if value.is_empty() {
        return Err(CliError::Usage(format!("`{flag}` needs a non-empty value")));
    }
    if flag == "--negative" {
        negative.push(value.to_string());
    } else {
        positive.push(value.to_string());
    }
    Ok(())
}

/// Joins the words of a request into one line.
fn join_request(words: &[String], topic: &str) -> Result<String, CliError> {
    if words.is_empty() {
        return Err(CliError::Usage(format!("missing <request> for `{topic}`")));
    }
    Ok(words.join(" "))
}

/// Splits exactly two positional arguments.
fn exactly_two(
    values: &[String],
    topic: &str,
    first_name: &str,
    second_name: &str,
) -> Result<(String, String), CliError> {
    match values {
        [first, second] => Ok((first.clone(), second.clone())),
        [] => Err(CliError::Usage(format!(
            "missing <{first_name}> and <{second_name}> for `{topic}`"
        ))),
        [only] => Err(CliError::Usage(format!(
            "missing <{second_name}> for `{topic}` (got <{first_name}>=`{only}`)"
        ))),
        _ => Err(CliError::Usage(format!(
            "unexpected argument `{}` for `{topic}`",
            values[2]
        ))),
    }
}

/// The error of an option the tool does not know.
fn unknown_flag(flag: &str, topic: &str) -> CliError {
    CliError::Usage(format!(
        "unknown flag `{flag}` for `{topic}` (run `regie {topic} --help`)"
    ))
}

/// Prints the usage block of the tool, or of one subcommand.
fn print_help(topic: Option<&str>) {
    match topic {
        None => print_top_help(),
        Some("match") => print_command_help(
            "match - match a pattern against an input",
            "regie match <pattern> <input>",
            &[
                ("<pattern>", "The regular expression to compile"),
                ("<input>", "The text to match against"),
            ],
            &[
                "0  the pattern matched",
                "1  the pattern did not match",
                "2  usage error, or a pattern the engine rejected",
            ],
        ),
        Some("test") => print_command_help(
            "test - exit successfully when a pattern matches an input",
            "regie test <pattern> <input>",
            &[
                ("<pattern>", "The regular expression to compile"),
                ("<input>", "The text to match against"),
            ],
            &[
                "0  the pattern matched",
                "1  the pattern did not match",
                "2  usage error, or a pattern the engine rejected",
            ],
        ),
        Some("intent") => print_command_help(
            "intent - interpret a natural-language request",
            "regie intent <request>",
            &[("<request>", "The request to interpret")],
            &[
                "0  the request was understood with confidence",
                "1  the request is ambiguous",
                "2  usage error",
            ],
        ),
        Some("search") => print_command_help(
            "search - verify a request against examples",
            "regie search <request> --positive <example> [--negative <example>]...",
            &[
                ("<request>", "The request to interpret"),
                (
                    "--positive <example>",
                    "An example the outcome has to accept",
                ),
                (
                    "--negative <example>",
                    "An example the outcome has to reject",
                ),
            ],
            &[
                "0  exactly one candidate satisfies the examples",
                "1  no candidate, or more than one, satisfies the examples",
                "2  usage error",
            ],
        ),
        Some("dfa") => print_command_help(
            "dfa - compile a pattern into a DFA artifact",
            "regie dfa <pattern> <out.bin>",
            &[
                ("<pattern>", "The regular expression to compile"),
                ("<out.bin>", "Where the artifact is written"),
            ],
            &[
                "0  the artifact was written",
                "2  usage error, or a pattern the engine rejected",
            ],
        ),
        Some("decode") => print_command_help(
            "decode - match an input against a DFA artifact",
            "regie decode <artifact.bin> <input>",
            &[
                ("<artifact.bin>", "The artifact to read"),
                ("<input>", "The text to match against"),
            ],
            &[
                "0  the artifact matched the input",
                "1  the artifact did not match the input",
                "2  usage error, I/O error, or an artifact the codec rejected",
            ],
        ),
        Some(other) => {
            println!("regie: no help for `{other}`");
            print_top_help();
        }
    }
}

/// Prints the usage block of the tool.
fn print_top_help() {
    println!(
        "regie {version} - deterministic regular expressions with an intent layer

USAGE:
    regie <COMMAND> [ARGS]

COMMANDS:
    match <pattern> <input>          Match a pattern against an input
    test  <pattern> <input>          Exit 0 when the pattern matches, 1 otherwise
    intent <request>                 Interpret a natural-language request
    search <request> --positive <v>  Verify a request against examples
             [--negative <v>...]
    dfa   <pattern> <out.bin>        Compile a pattern into a DFA artifact
    decode <artifact.bin> <input>    Match an input against a DFA artifact
    help                             Print this help
    version                          Print the version

OPTIONS:
    -h, --help                       Print this help
    -V, --version                    Print the version

NOTES:
    Every subcommand accepts `--help`, and `--` ends the options.
    `intent` and `search` take the request as one argument or as several
    words, which are joined with a single space.

EXIT CODES:
    0  success
    1  no match, or no answer the tool could settle on
    2  usage error, I/O error, or an input the engine rejected",
        version = env!("CARGO_PKG_VERSION")
    );
}

/// Prints the usage block of one subcommand.
fn print_command_help(title: &str, usage: &str, arguments: &[(&str, &str)], exit_codes: &[&str]) {
    println!("{title}\n");
    println!("USAGE:\n    {usage}\n");
    println!("ARGUMENTS:");
    for (name, description) in arguments {
        println!("    {name:<24}{description}");
    }
    println!("\nEXIT CODES:");
    for code in exit_codes {
        println!("    {code}");
    }
}
