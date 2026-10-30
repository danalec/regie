//! Integration tests for the `regie` binary.
//!
//! The tests run the compiled binary, so they check what a script actually
//! sees: the output on standard output, the error on standard error and the
//! exit code.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};
use std::time::Duration;

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

/// A port that is free at the moment of the call.
///
/// The port is taken from the operating system and released right away, so
/// there is a small window in which another process could take it.  The test
/// that uses it fails loudly when that happens, instead of hanging.
fn free_port() -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("no port could be taken");
    let port = listener
        .local_addr()
        .expect("the port of the listener is unknown")
        .port();
    drop(listener);
    port
}

/// Sends a request to the server and returns what it answered.
fn ask(port: u16, request: &str) -> Option<String> {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).ok()?;
    stream.write_all(request.as_bytes()).ok()?;
    let mut answer = String::new();
    stream.read_to_string(&mut answer).ok()?;
    Some(answer)
}

/// The request that asks the server for its page.
const PAGE: &str = "GET / HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n";

/// Starts the server and returns the port it listens on.
///
/// The port is taken from the operating system and released right away, so
/// there is a small window in which another test, running in parallel, could
/// take it.  The server that loses the port stops at once, and the test takes
/// another one.
fn start_server() -> (u16, std::process::Child) {
    for _ in 0..8 {
        let port = free_port();
        let mut child = Command::new(BIN)
            .arg("serve")
            .arg("--port")
            .arg(port.to_string())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("the server could not be started");

        for _ in 0..50 {
            if let Some(answer) = ask(port, PAGE) {
                if answer.contains("200 OK") {
                    return (port, child);
                }
            }
            // The server that could not take the port is already gone.
            if matches!(child.try_wait(), Ok(Some(_))) {
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        let _ = child.kill();
        let _ = child.wait();
    }
    panic!("the server did not answer in time");
}

/// The body of an HTTP answer.
fn body(answer: &str) -> &str {
    answer
        .split("\r\n\r\n")
        .nth(1)
        .expect("the answer has no body")
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
    let output = run(&["intent", "validate a CPF"]);

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
        "validate a CPF",
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
    let output = run(&["search", "validate a CPF"]);

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
    for subcommand in [
        "match", "test", "intent", "search", "show", "serve", "dfa", "decode",
    ] {
        assert!(
            text.contains(subcommand),
            "`{subcommand}` is missing from the help"
        );
    }
    assert!(text.contains("EXIT CODES"), "{text}");
}

#[test]
fn show_underlines_the_match_and_lists_the_capture_groups() {
    let output = run(&[
        "show",
        "extract the email",
        "--input",
        "mail ana@corp.com now",
    ]);

    assert_eq!(code(&output), Some(0));
    let text = stdout(&output);
    assert!(text.contains("haystack: mail ana@corp.com now"), "{text}");
    assert!(text.contains("matched 5..17"), "{text}");
    assert!(text.contains("0: 5..17 \"ana@corp.com\""), "{text}");
    assert!(text.contains("1: 5..17 \"ana@corp.com\""), "{text}");
}

#[test]
fn show_explains_that_a_validation_is_anchored() {
    let output = run(&[
        "show",
        "validate a CPF",
        "--input",
        "the cpf of ana is 123.456.789-09",
    ]);

    assert_eq!(code(&output), Some(1));
    let text = stdout(&output);
    assert!(text.contains("no match"), "{text}");
    assert!(
        text.contains("a validation is anchored - the whole haystack has to be the value"),
        "{text}"
    );
}

#[test]
fn show_without_a_haystack_is_a_usage_error() {
    let output = run(&["show", "validate a CPF"]);

    assert_eq!(code(&output), Some(2));
    assert!(
        stderr(&output).starts_with("error: "),
        "{}",
        stderr(&output)
    );
}

#[test]
fn show_writes_plain_text_into_a_pipe() {
    let output = run(&[
        "show",
        "extract the email",
        "--input",
        "mail ana@corp.com now",
    ]);

    let text = stdout(&output);
    assert!(
        !text.contains('\u{1b}'),
        "the output carries escape codes: {text:?}"
    );
}

#[test]
fn serve_answers_the_page_and_the_analysis() {
    let (port, mut child) = start_server();

    let page = ask(port, PAGE);
    let analysis = page.as_ref().and_then(|_| {
        ask(
            port,
            "GET /analyse?request=extract%20the%20email&haystack=mail%20ana@corp.com%20now HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n",
        )
    });
    let validation = analysis.as_ref().and_then(|_| {
        ask(
            port,
            "GET /analyse?request=validate%20a%20CPF&haystack=123.456.789-09 HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n",
        )
    });

    let _ = child.kill();
    let _ = child.wait();

    let page = page.expect("the server did not answer in time");
    assert!(page.contains("regie - the workbench"), "{page}");

    let answer = analysis.expect("the server did not answer the analysis");
    let value: serde_json::Value =
        serde_json::from_str(body(&answer)).expect("the answer is not valid JSON");
    assert_eq!(value["status"], "confident");
    assert_eq!(value["matched"], true);
    assert_eq!(value["start"], 5);
    assert_eq!(value["end"], 17);
    assert_eq!(value["anchored"], false);

    let answer = validation.expect("the server did not answer the second analysis");
    let value: serde_json::Value =
        serde_json::from_str(body(&answer)).expect("the answer is not valid JSON");
    assert_eq!(value["status"], "confident");
    assert_eq!(value["matched"], true);
    assert_eq!(value["anchored"], true);
}

#[test]
fn the_version_is_reported() {
    let output = run(&["--version"]);

    assert_eq!(code(&output), Some(0));
    assert!(stdout(&output).starts_with("regie "), "{}", stdout(&output));
}

#[test]
fn the_server_answers_a_raw_pattern() {
    let (port, mut child) = start_server();

    // The pattern is `\w+@[a-z]+\.[\w]+`: a word, an at sign, a lower-case
    // run, a dot and a word.  It lands on `ana@corp.com`, the bytes 5 to 17.
    let answer = ask(
        port,
        "GET /pattern?pattern=%5Cw%2B%40%5Ba-z%5D%2B%5C.%5B%5Cw%5D%2B&haystack=mail%20ana%40corp.com%20now HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n",
    );
    let _ = child.kill();
    let _ = child.wait();

    let answer = answer.expect("the server did not answer");
    let value: serde_json::Value =
        serde_json::from_str(body(&answer)).expect("the answer is not valid JSON");
    assert_eq!(value["status"], "compiled");
    assert_eq!(value["matched"], true);
    assert_eq!(value["start"], 5);
    assert_eq!(value["end"], 17);
    assert_eq!(
        value["groups"]
            .as_array()
            .expect("the groups are a list")
            .len(),
        1
    );
}

#[test]
fn the_server_refuses_a_pattern_the_engine_refuses() {
    let (port, mut child) = start_server();

    let answer = ask(
        port,
        "GET /pattern?pattern=%28%3F%3Cname%3Ea%29&haystack=a HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n",
    );
    let _ = child.kill();
    let _ = child.wait();

    let answer = answer.expect("the server did not answer");
    let value: serde_json::Value =
        serde_json::from_str(body(&answer)).expect("the answer is not valid JSON");
    assert_eq!(value["status"], "error");
    assert_eq!(value["error_kind"], "Unsupported");
    assert!(value["error_position"].is_number(), "{value}");
}

#[test]
fn the_server_reports_the_automaton_of_a_pattern() {
    let (port, mut child) = start_server();

    let answer = ask(
        port,
        "GET /automaton?pattern=%5Cd%2B HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n",
    );
    let _ = child.kill();
    let _ = child.wait();

    let answer = answer.expect("the server did not answer");
    let value: serde_json::Value =
        serde_json::from_str(body(&answer)).expect("the answer is not valid JSON");
    assert_eq!(value["status"], "ok");
    assert!(value["states"].as_u64().expect("the states are a number") >= 2);
    assert!(value["classes"].as_u64().expect("the classes are a number") >= 1);
    assert!(!value["transitions"]
        .as_array()
        .expect("the transitions are a list")
        .is_empty());
}

#[test]
fn the_server_reports_the_automaton_of_a_request() {
    let (port, mut child) = start_server();

    let answer = ask(
        port,
        "GET /automaton?request=validate%20a%20CPF HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n",
    );
    let _ = child.kill();
    let _ = child.wait();

    let answer = answer.expect("the server did not answer");
    let value: serde_json::Value =
        serde_json::from_str(body(&answer)).expect("the answer is not valid JSON");
    assert_eq!(value["status"], "ok");
    assert_eq!(value["kind"], "validate brazilian_cpf");
    assert!(value["states"].as_u64().expect("the states are a number") >= 2);
}

#[test]
fn the_server_serves_the_script_of_the_page() {
    let (port, mut child) = start_server();

    let answer = ask(
        port,
        "GET /app.js HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n",
    );
    let _ = child.kill();
    let _ = child.wait();

    let answer = answer.expect("the server did not answer");
    assert!(answer.contains("200 OK"), "{answer}");
    assert!(answer.contains("RegieMarkdown"), "{answer}");
    assert!(answer.contains("RegieEditor"), "{answer}");
}
