//! Integration tests for the `regie` binary.
//!
//! The tests run the compiled binary, so they check what a script actually
//! sees: the output on standard output, the error on standard error and the
//! exit code.

use std::path::PathBuf;
use std::process::{Command, Output};

/// The binary under test.
const BIN: &str = env!("CARGO_BIN_EXE_regie");

/// Runs the binary with `args`.
fn run(args: &[&str]) -> Output {
    Command::new(BIN)
        .args(args)
        .output()
        .expect("the regie binary could not be started")
}

/// The standard output of a run, as text.
fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// The standard error of a run, as text.
fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

/// The exit code of a run.
fn code(output: &Output) -> Option<i32> {
    output.status.code()
}

/// A path in the temporary directory, unique to this test run.
fn temp_path(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("regie-cli-{}-{name}", std::process::id()))
}

#[test]
fn match_reports_the_span_and_the_capture_groups() {
    let output = run(&["match", r"(\w+)@(\w+\.\w+)", "mail: ana@corp.com"]);

    assert_eq!(code(&output), Some(0));
    let text = stdout(&output);
    assert!(text.contains("match: 6..18 \"ana@corp.com\""), "{text}");
    assert!(text.contains("group 1: 6..9 \"ana\""), "{text}");
    assert!(text.contains("group 2: 10..18 \"corp.com\""), "{text}");
    assert!(stderr(&output).is_empty(), "{}", stderr(&output));
}

#[test]
fn match_without_a_match_exits_with_one() {
    let output = run(&["match", r"\d+", "abc"]);

    assert_eq!(code(&output), Some(1));
    assert!(stdout(&output).contains("no match"), "{}", stdout(&output));
    assert!(stderr(&output).is_empty(), "{}", stderr(&output));
}

#[test]
fn test_exits_with_zero_when_the_pattern_matches() {
    let output = run(&["test", r"^\d{3}-\d{2}$", "123-45"]);

    assert_eq!(code(&output), Some(0));
    assert!(stdout(&output).contains("match"), "{}", stdout(&output));
}

#[test]
fn test_exits_with_one_when_the_pattern_does_not_match() {
    let output = run(&["test", r"^\d{3}-\d{2}$", "123-456"]);

    assert_eq!(code(&output), Some(1));
    assert!(stdout(&output).contains("no match"), "{}", stdout(&output));
}

#[test]
fn intent_prints_a_summary_and_a_parseable_json_line() {
    let output = run(&["intent", "validar CPF"]);

    assert_eq!(code(&output), Some(0));
    let text = stdout(&output);
    let mut lines = text.lines();
    let summary = lines.next().expect("the summary line is missing");
    assert_eq!(
        summary,
        "summary: confident validate brazilian_cpf strictness=standard charset=digits max_len=14"
    );

    let json = lines.next().expect("the JSON line is missing");
    assert!(lines.next().is_none(), "the output has more than two lines");
    let value: serde_json::Value =
        serde_json::from_str(json).expect("the second line is not valid JSON");
    assert_eq!(value["status"], "confident");
    assert_eq!(value["candidate"]["type"], "validate");
    assert_eq!(value["candidate"]["kind"], "brazilian_cpf");
    assert_eq!(value["candidate"]["strictness"], "standard");
    assert_eq!(value["candidate"]["charset"], "digits");
    assert_eq!(value["candidate"]["max_len"], 14);
}

#[test]
fn search_prints_the_confident_candidate_and_the_scores() {
    let output = run(&[
        "search",
        "validar CPF",
        "--positive",
        "123.456.789-09",
        "--negative",
        "123.456.789",
    ]);

    assert_eq!(code(&output), Some(0));
    let text = stdout(&output);
    assert!(text.contains("search: confident"), "{text}");
    assert!(
        text.contains(
            "candidate: validate brazilian_cpf strictness=standard charset=digits max_len=14"
        ),
        "{text}"
    );
    assert!(
        text.contains("rank 1: validate brazilian_cpf score="),
        "{text}"
    );
    assert!(
        text.contains("positives=1/1 negatives_rejected=1/1"),
        "{text}"
    );
}

#[test]
fn search_without_a_positive_example_is_a_usage_error() {
    let output = run(&["search", "validar CPF"]);

    assert_eq!(code(&output), Some(2));
    assert!(
        stderr(&output).starts_with("error: "),
        "{}",
        stderr(&output)
    );
}

#[test]
fn dfa_writes_an_artifact_that_decode_matches_against() {
    let path = temp_path("round-trip.bin");
    let artifact = path.to_string_lossy().into_owned();

    let output = run(&["dfa", r"^ana@corp\.com$", &artifact]);
    assert_eq!(code(&output), Some(0), "{}", stderr(&output));
    assert!(stdout(&output).contains("wrote "), "{}", stdout(&output));
    assert!(path.exists(), "the artifact was not written");

    let output = run(&["decode", &artifact, "ana@corp.com"]);
    assert_eq!(code(&output), Some(0), "{}", stdout(&output));
    assert!(stdout(&output).contains("match"), "{}", stdout(&output));

    let output = run(&["decode", &artifact, "ana@other.com"]);
    assert_eq!(code(&output), Some(1));
    assert!(stdout(&output).contains("no match"), "{}", stdout(&output));

    let _ = std::fs::remove_file(&path);
}

#[test]
fn an_invalid_pattern_is_reported_on_stderr() {
    let output = run(&["match", "a(", "x"]);

    assert_eq!(code(&output), Some(2));
    assert!(
        stderr(&output).starts_with("error: "),
        "{}",
        stderr(&output)
    );
    assert!(
        !stderr(&output).contains("panicked"),
        "the tool must not panic: {}",
        stderr(&output)
    );
    assert!(stdout(&output).is_empty(), "{}", stdout(&output));
}

#[test]
fn an_unsupported_construct_is_reported_on_stderr() {
    let output = run(&["match", "(?=x)", "x"]);

    assert_eq!(code(&output), Some(2));
    assert!(
        stderr(&output).starts_with("error: "),
        "{}",
        stderr(&output)
    );
}

#[test]
fn an_unreadable_artifact_is_reported_on_stderr() {
    let output = run(&["decode", "definitely-not-a-file.bin", "x"]);

    assert_eq!(code(&output), Some(2));
    assert!(
        stderr(&output).starts_with("error: "),
        "{}",
        stderr(&output)
    );
    assert!(
        !stderr(&output).contains("panicked"),
        "the tool must not panic: {}",
        stderr(&output)
    );
}

#[test]
fn an_unknown_flag_is_a_usage_error() {
    let output = run(&["match", "--nope", "x"]);

    assert_eq!(code(&output), Some(2));
    assert!(
        stderr(&output).starts_with("error: "),
        "{}",
        stderr(&output)
    );
}

#[test]
fn an_unknown_subcommand_is_a_usage_error() {
    let output = run(&["bogus"]);

    assert_eq!(code(&output), Some(2));
    assert!(
        stderr(&output).starts_with("error: "),
        "{}",
        stderr(&output)
    );
}

#[test]
fn help_lists_every_subcommand() {
    let output = run(&["--help"]);

    assert_eq!(code(&output), Some(0));
    let text = stdout(&output);
    for subcommand in ["match", "test", "intent", "search", "dfa", "decode"] {
        assert!(
            text.contains(subcommand),
            "`{subcommand}` is missing from the help"
        );
    }
    assert!(text.contains("EXIT CODES"), "{text}");
}

#[test]
fn the_version_is_reported() {
    let output = run(&["--version"]);

    assert_eq!(code(&output), Some(0));
    assert!(stdout(&output).starts_with("regie "), "{}", stdout(&output));
}
