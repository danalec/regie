//! The `serve` subcommand.
//!
//! The subcommand starts a small HTTP server on the loopback interface and
//! serves a page that interprets a request and shows what it matches.  The
//! page asks the server for an analysis every time the request or the
//! haystack changes, so the view follows the typing.
//!
//! The server never listens on anything but `127.0.0.1`: it is a view of the
//! engine, not a service.

use crate::render;
use crate::{CliError, EXIT_OK};
use regie_core::spec::{CandidateSpec, IntentSpec};
use regie_core::{Dfa, Regex, RegexBuilder, RegexError};
use regie_intent::IntentModel;
use serde_json::json;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::time::Duration;

/// The page the server serves.
const PAGE: &str = include_str!("../../web/index.html");

/// The script the page loads.
///
/// The three parts are one file on the wire, so the page needs a single
/// request; each part is a module that publishes itself on the window.
const APP: &str = concat!(
    include_str!("../../web/markdown.js"),
    include_str!("../../web/editor.js"),
    include_str!("../../web/workbench.js"),
);

/// The largest request the server reads.
const MAX_REQUEST: usize = 64 * 1024;

/// The time the server waits for the rest of a request.
const READ_TIMEOUT: Duration = Duration::from_secs(5);

/// The time the server gives the socket to deliver the answer.
const DRAIN_TIMEOUT: Duration = Duration::from_millis(20);

/// Runs the `serve` subcommand.
pub fn run_serve(port: u16) -> Result<u8, CliError> {
    let address = format!("127.0.0.1:{port}");
    let listener = TcpListener::bind(&address).map_err(|error| CliError::Io {
        path: address.clone(),
        source: error,
    })?;

    println!("regie is serving the live view on http://{address}");
    println!("press ctrl-c to stop it");

    for stream in listener.incoming() {
        let mut stream = match stream {
            Ok(stream) => stream,
            Err(error) => {
                eprintln!("could not accept a connection: {error}");
                continue;
            }
        };
        // Every connection gets its own thread, so a client that stops
        // reading cannot hold up the others.
        std::thread::spawn(move || {
            if let Err(error) = handle(&mut stream) {
                eprintln!("could not answer the request: {error}");
            }
        });
    }

    Ok(EXIT_OK)
}

/// Answers one request.
fn handle(stream: &mut TcpStream) -> std::io::Result<()> {
    let request = read_request(stream)?;
    let target = request
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .unwrap_or("/")
        .to_owned();

    let (path, query) = match target.split_once('?') {
        Some((path, query)) => (path.to_owned(), query.to_owned()),
        None => (target, String::new()),
    };

    let (content_type, body) = match path.as_str() {
        "/analyse" => ("application/json", analyse(&query)),
        "/pattern" => ("application/json", pattern(&query)),
        "/automaton" => ("application/json", automaton(&query)),
        "/app.js" => ("application/javascript; charset=utf-8", APP.to_owned()),
        _ => ("text/html; charset=utf-8", PAGE.to_owned()),
    };

    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(response.as_bytes())?;
    stream.flush()?;
    drain(stream);
    Ok(())
}

/// Empties the input of the connection before it is closed.
///
/// The socket is closed right after the answer, and a close that follows a
/// write too quickly can reach the client as a reset instead of the answer:
/// the client then loses both.  Reading what is left of the input gives the
/// stack the moment it needs, and leaves the receive buffer empty, so the
/// client gets the answer and a clean end of stream.
fn drain(stream: &mut TcpStream) {
    let _ = stream.set_read_timeout(Some(DRAIN_TIMEOUT));
    let mut scratch = [0u8; 256];
    while let Ok(read) = stream.read(&mut scratch) {
        if read == 0 {
            break;
        }
    }
}

/// Reads the header block of a request.
///
/// The request may arrive in more than one segment, so the read continues
/// until the empty line that ends the headers is found.
fn read_request(stream: &mut TcpStream) -> std::io::Result<String> {
    let _ = stream.set_read_timeout(Some(READ_TIMEOUT));

    let mut buffer = vec![0u8; MAX_REQUEST];
    let mut filled = 0usize;

    while filled < buffer.len() {
        let read = stream.read(&mut buffer[filled..])?;
        if read == 0 {
            break;
        }
        filled += read;
        if buffer[..filled]
            .windows(4)
            .any(|window| window == b"\r\n\r\n")
        {
            break;
        }
    }

    Ok(String::from_utf8_lossy(&buffer[..filled]).into_owned())
}

/// Answers an analysis request with a JSON document.
fn analyse(query: &str) -> String {
    let request = parameter(query, "request").unwrap_or_default();
    let haystack = parameter(query, "haystack").unwrap_or_default();

    let outcome = IntentModel::embedded().interpret(&request);
    let mut answer = json!({
        "request": request,
        "haystack": haystack,
    });

    match outcome {
        IntentSpec::Confident(spec) => {
            answer["status"] = json!("confident");
            answer["spec"] = json!(render::intent_summary(&IntentSpec::Confident(spec)));
            answer["kind"] = json!(render::candidate_short_name(&spec));
            answer["anchored"] = json!(matches!(spec, CandidateSpec::Validate { .. }));

            match Regex::from_spec(&spec) {
                Ok(regex) => {
                    let bytes = haystack.as_bytes();
                    match regex.find(bytes) {
                        Some(span) => {
                            answer["matched"] = json!(true);
                            answer["start"] = json!(span.start);
                            answer["end"] = json!(span.end);
                            let mut groups = Vec::new();
                            if let Some(captures) = regex.captures(bytes) {
                                groups = groups_of(&captures, bytes);
                            }
                            answer["groups"] = json!(groups);
                        }
                        None => {
                            answer["matched"] = json!(false);
                        }
                    }
                }
                Err(error) => {
                    answer["status"] = json!("error");
                    answer["error"] = json!(error.to_string());
                }
            }
        }
        IntentSpec::Ambiguous {
            reason,
            suggestions,
        } => {
            answer["status"] = json!("ambiguous");
            answer["reason"] = json!(format!("{reason:?}"));
            answer["kind"] = json!("ambiguous");
            answer["spec"] = json!(render::intent_summary(&outcome));
            let names: Vec<String> = suggestions
                .iter()
                .map(|suggestion| suggestion.kind_name().to_owned())
                .collect();
            answer["suggestions"] = json!(names);
        }
    }

    answer.to_string()
}

/// Answers a request that carries a raw pattern.
///
/// This is the path an advanced user takes: the pattern goes straight to the
/// compiler, and the answer says what the engine made of it - the match, the
/// capture groups, or the reason it refused the pattern.
fn pattern(query: &str) -> String {
    let source = parameter(query, "pattern").unwrap_or_default();
    let haystack = parameter(query, "haystack").unwrap_or_default();
    let flags = parameter(query, "flags").unwrap_or_default();

    let mut answer = json!({
        "pattern": source,
        "haystack": haystack,
        "flags": flags,
    });

    match builder_with(&source, &flags).build() {
        Ok(regex) => {
            answer["status"] = json!("compiled");
            let bytes = haystack.as_bytes();
            match regex.captures(bytes) {
                Some(captures) => {
                    answer["matched"] = json!(true);
                    answer["start"] = json!(captures.get(0).map(|span| span.start));
                    answer["end"] = json!(captures.get(0).map(|span| span.end));
                    answer["groups"] = json!(groups_of(&captures, bytes));
                }
                None => {
                    answer["matched"] = json!(false);
                    answer["groups"] = json!([]);
                }
            }
        }
        Err(error) => {
            answer["status"] = json!("error");
            answer["error"] = json!(error.to_string());
            answer["error_kind"] = json!(error_kind(&error));
            answer["error_position"] = json!(error_position(&error));
        }
    }

    answer.to_string()
}

/// Answers a request about the automaton a pattern compiles into.
///
/// The automaton is what the engine runs on an embedded target: a table of
/// states over the byte equivalence classes of the pattern.  The answer
/// carries its shape, so an advanced user can see what the pattern costs.
fn automaton(query: &str) -> String {
    let mut answer = json!({});

    let built = if let Some(source) = parameter(query, "pattern") {
        answer["pattern"] = json!(source.clone());
        let flags = parameter(query, "flags").unwrap_or_default();
        answer["flags"] = json!(flags);
        builder_with(&source, &flags).build()
    } else {
        let request = parameter(query, "request").unwrap_or_default();
        answer["request"] = json!(request.clone());
        match IntentModel::embedded().interpret(&request) {
            IntentSpec::Confident(spec) => {
                answer["kind"] = json!(render::candidate_short_name(&spec));
                answer["spec"] = json!(render::intent_summary(&IntentSpec::Confident(spec)));
                Regex::from_spec(&spec)
            }
            IntentSpec::Ambiguous {
                reason,
                suggestions,
            } => {
                answer["status"] = json!("ambiguous");
                answer["reason"] = json!(format!("{reason:?}"));
                answer["suggestions"] = json!(suggestions
                    .iter()
                    .map(|suggestion| suggestion.kind_name())
                    .collect::<Vec<_>>());
                return answer.to_string();
            }
        }
    };

    match built {
        Ok(regex) => match regex.build_dfa() {
            Ok(dfa) => {
                answer["status"] = json!("ok");
                answer["states"] = json!(dfa.state_count());
                answer["classes"] = json!(dfa.class_count());
                answer["accept_at_end"] = json!(dfa.accepts_at_end_only());
                answer["transitions"] = json!(transitions_of(&dfa));
                answer["accepting"] = json!(accepting_states(&dfa));
            }
            Err(error) => {
                answer["status"] = json!("error");
                answer["error"] = json!(error.to_string());
                answer["error_kind"] = json!(error_kind(&error));
                answer["error_position"] = json!(error_position(&error));
            }
        },
        Err(error) => {
            answer["status"] = json!("error");
            answer["error"] = json!(error.to_string());
            answer["error_kind"] = json!(error_kind(&error));
            answer["error_position"] = json!(error_position(&error));
        }
    }

    answer.to_string()
}

/// A builder for a pattern, with the flags of the engine.
///
/// The flags are the ones the page shows: `i` makes the match case
/// insensitive, `m` makes the anchors match at the end of a line, and `s`
/// makes the dot match a newline.  The same builder serves the analysis and
/// the automaton, so the two always agree on what a pattern means.
fn builder_with(source: &str, flags: &str) -> RegexBuilder {
    let mut builder = Regex::builder().pattern(source);
    if flags.contains('i') {
        builder = builder.case_insensitive(true);
    }
    if flags.contains('m') {
        builder = builder.multi_line(true);
    }
    if flags.contains('s') {
        builder = builder.dot_matches_newline(true);
    }
    builder
}

/// The capture groups of a match, as JSON.
fn groups_of(captures: &regie_core::Captures, bytes: &[u8]) -> Vec<serde_json::Value> {
    let mut groups = Vec::new();
    for index in 0..captures.len() {
        if let Some(group) = captures.get(index) {
            groups.push(json!({
                "index": index,
                "start": group.start,
                "end": group.end,
                "text": String::from_utf8_lossy(&bytes[group.start..group.end]),
            }));
        }
    }
    groups
}

/// The transitions of an automaton that lead somewhere.
///
/// The dead state is left out: it carries no information, and a table of
/// 8 192 entries would drown the ones that matter.
fn transitions_of(dfa: &Dfa) -> Vec<serde_json::Value> {
    let mut out = Vec::new();
    for state in 0..dfa.state_count() {
        for class in 0..dfa.class_count() {
            let target = dfa.transition(state as u16, class as u8);
            if target != regie_core::dfa::DEAD_STATE {
                out.push(json!({
                    "from": state,
                    "class": class,
                    "to": target,
                }));
            }
        }
    }
    out
}

/// The states of an automaton that accept a match.
fn accepting_states(dfa: &Dfa) -> Vec<u16> {
    let mut out = Vec::new();
    for state in 0..dfa.state_count() {
        if dfa.is_accept(state as u16) {
            out.push(state as u16);
        }
    }
    out
}

/// The name of the kind of a compilation failure.
///
/// The name is stable, so the page can tell the user what went wrong without
/// parsing the message.
fn error_kind(error: &RegexError) -> &'static str {
    match error {
        RegexError::PatternTooLong { .. } => "PatternTooLong",
        RegexError::Syntax { .. } => "Syntax",
        RegexError::Unsupported { .. } => "Unsupported",
        RegexError::ProgramTooLarge { .. } => "ProgramTooLarge",
        RegexError::TooManyGroups { .. } => "TooManyGroups",
        RegexError::RepetitionTooLarge { .. } => "RepetitionTooLarge",
        RegexError::NestingTooDeep { .. } => "NestingTooDeep",
        RegexError::InvalidClass { .. } => "InvalidClass",
        RegexError::DfaTooLarge { .. } => "DfaTooLarge",
        RegexError::TooManyClasses { .. } => "TooManyClasses",
        RegexError::ThreadLimitExceeded { .. } => "ThreadLimitExceeded",
        _ => "Unknown",
    }
}

/// The byte offset a compilation failure points at, when it points at one.
fn error_position(error: &RegexError) -> Option<usize> {
    match error {
        RegexError::Syntax { position, .. } => Some(*position),
        RegexError::Unsupported { position, .. } => Some(*position),
        RegexError::InvalidClass { position, .. } => Some(*position),
        _ => None,
    }
}

/// Reads one parameter out of a query string.
fn parameter(query: &str, name: &str) -> Option<String> {
    for pair in query.split('&') {
        let Some((key, value)) = pair.split_once('=') else {
            continue;
        };
        if decode(key) == name {
            return Some(decode(value));
        }
    }
    None
}

/// Decodes the percent escapes of a query string value.
///
/// A `+` is a space, and every `%XX` is the byte it names.  The bytes are
/// then read as UTF-8, so a multi-byte character survives the round trip.
fn decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut index = 0;

    while index < bytes.len() {
        match bytes[index] {
            b'+' => {
                out.push(b' ');
                index += 1;
            }
            b'%' if index + 2 < bytes.len() => {
                let high = (bytes[index + 1] as char).to_digit(16);
                let low = (bytes[index + 2] as char).to_digit(16);
                match (high, low) {
                    (Some(high), Some(low)) => {
                        out.push((high * 16 + low) as u8);
                        index += 3;
                    }
                    _ => {
                        out.push(bytes[index]);
                        index += 1;
                    }
                }
            }
            byte => {
                out.push(byte);
                index += 1;
            }
        }
    }

    String::from_utf8_lossy(&out).into_owned()
}
