//! Output helpers for the `regie` tool.
//!
//! The specification types of `regie-core` do implement
//! `serde::Serialize`, but the tool deliberately does not dump them raw. The
//! `serde` form is the externally tagged one the training dataset uses
//! (`{"Validate":{"kind":"Email",...}}`), which is convenient for a machine
//! but noisy for a command line.
//!
//! The tool therefore renders a **stable, flat projection** of the
//! specification:
//!
//! * The JSON form is a single line of plain JSON, built from
//!   [`serde_json::Value`]. The family of the specification is the `type`
//!   field (`validate`, `extract`, `filter`, `custom`), every enumeration is
//!   spelled out as a stable snake-case name (`brazilian_cpf`, `strict`,
//!   `printable_ascii`), and every optional field is `null` when it is
//!   absent.
//! * The human form is one line of `key=value` pairs
//!   (`validate brazilian_cpf strictness=standard charset=digits max_len=14`).
//!
//! Both forms are stable: the same specification always renders to the same
//! bytes, on every platform, which is what a script can rely on.
//!
//! The projection cannot silently fall behind the types it mirrors: every
//! variant of [`CandidateSpec`] is matched with its fields listed one by one,
//! without a `..` rest pattern, so adding a field to a specification is a
//! compile error in this file until the projection is updated.

use regie_core::hir::AssertKind;
use regie_core::spec::{
    extraction_kind_name, validation_kind_name, AmbiguityReason, BoundaryPolicy, CandidateSpec,
    CapturePolicy, CharsetPolicy, FilterAction, IntentSpec, PatternParams, PatternTemplate,
    Strictness, TemplatePiece, TermKind,
};
use regie_core::{Captures, Regex};
use serde_json::{json, Value};

/// Quotes and escapes a byte string for display.
///
/// Printable ASCII is kept as it is, the usual escapes are used for quotes,
/// backslashes and control characters, and any byte that is not valid UTF-8
/// is written as `\xNN`.
pub fn quote_bytes(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() + 2);
    out.push('"');
    out.push_str(&escape_bytes(bytes));
    out.push('"');
    out
}

/// Escapes a byte string into printable text.
///
/// The result is always valid UTF-8: byte sequences that are not valid UTF-8
/// are written as `\xNN`.
pub fn escape_bytes(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len());
    let mut rest = bytes;

    while !rest.is_empty() {
        match std::str::from_utf8(rest) {
            Ok(text) => {
                push_escaped_text(&mut out, text);
                break;
            }
            Err(error) => {
                let valid = error.valid_up_to();
                // `valid_up_to` guarantees that the prefix is valid UTF-8.
                let text = std::str::from_utf8(&rest[..valid]).unwrap_or("");
                push_escaped_text(&mut out, text);
                out.push_str(&format!("\\x{:02x}", rest[valid]));
                rest = &rest[valid + 1..];
            }
        }
    }

    out
}

/// Appends the escaped form of a piece of text.
fn push_escaped_text(out: &mut String, text: &str) {
    for ch in text.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            ch if (ch as u32) < 0x20 => out.push_str(&format!("\\x{:02x}", ch as u32)),
            ch => out.push(ch),
        }
    }
}

/// Prints the match and its capture groups.
///
/// The spans are byte offsets into the input, the same offsets the engine
/// reports.
pub fn print_match(regex: &Regex, captures: &Captures, haystack: &[u8]) {
    match captures.get(0) {
        Some(span) => println!(
            "match: {}..{} {}",
            span.start,
            span.end,
            quote_bytes(&haystack[span.start..span.end])
        ),
        None => println!("match: <none>"),
    }

    for index in 1..=regex.group_count() {
        match captures.get(index) {
            Some(span) => println!(
                "group {index}: {}..{} {}",
                span.start,
                span.end,
                quote_bytes(&haystack[span.start..span.end])
            ),
            None => println!("group {index}: <none>"),
        }
    }
}

/// Renders an intent specification as a JSON value.
///
/// The shape is stable:
///
/// ```text
/// {"status":"confident","candidate":{...}}
/// {"status":"ambiguous","reason":"conflicting_kinds","suggestions":[{...}]}
/// ```
pub fn intent_json(spec: &IntentSpec) -> Value {
    match spec {
        IntentSpec::Confident(candidate) => json!({
            "status": "confident",
            "candidate": candidate_json(candidate),
        }),
        IntentSpec::Ambiguous {
            reason,
            suggestions,
        } => {
            let mut list = Vec::new();
            for suggestion in suggestions.iter() {
                list.push(candidate_json(suggestion));
            }
            json!({
                "status": "ambiguous",
                "reason": ambiguity_reason_name(*reason),
                "suggestions": list,
            })
        }
    }
}

/// Renders a candidate specification as a JSON value.
pub fn candidate_json(candidate: &CandidateSpec) -> Value {
    match candidate {
        CandidateSpec::Validate {
            kind,
            strictness,
            charset,
            max_len,
        } => json!({
            "type": "validate",
            "kind": validation_kind_name(*kind),
            "strictness": strictness_name(*strictness),
            "charset": charset_name(*charset),
            "max_len": max_len,
        }),
        CandidateSpec::Extract {
            kind,
            boundary,
            capture,
        } => json!({
            "type": "extract",
            "kind": extraction_kind_name(*kind),
            "boundary": boundary_name(*boundary),
            "capture": capture_name(*capture),
        }),
        CandidateSpec::Filter { action, terms } => {
            let mut list = Vec::new();
            for term in terms.iter() {
                list.push(json!({
                    "kind": term_kind_name(term.kind),
                    "text": escape_bytes(term.bytes.as_slice()),
                }));
            }
            json!({
                "type": "filter",
                "action": filter_action_name(*action),
                "terms": list,
            })
        }
        CandidateSpec::Custom { template, params } => json!({
            "type": "custom",
            "template": template_json(template),
            "params": params_json(params),
        }),
    }
}

/// Renders a pattern template as a JSON value.
fn template_json(template: &PatternTemplate) -> Value {
    match template {
        PatternTemplate::Sequence(pieces) => {
            let mut list = Vec::new();
            for piece in pieces.iter() {
                list.push(piece_json(piece));
            }
            json!({ "shape": "sequence", "pieces": list })
        }
        PatternTemplate::Repeat { class, count } => json!({
            "shape": "repeat",
            "class": charset_name(*class),
            "count": count,
        }),
        PatternTemplate::LiteralThenClass {
            literal,
            class,
            count,
        } => json!({
            "shape": "literal_then_class",
            "literal": escape_bytes(literal.as_slice()),
            "class": charset_name(*class),
            "count": count,
        }),
    }
}

/// Renders one piece of a pattern template as a JSON value.
fn piece_json(piece: &TemplatePiece) -> Value {
    match piece {
        TemplatePiece::Literal(byte) => json!({ "piece": "literal", "byte": byte }),
        TemplatePiece::LiteralRange(start, end) => {
            json!({ "piece": "literal_range", "from": start, "to": end })
        }
        TemplatePiece::Class(class) => json!({ "piece": "class", "class": charset_name(*class) }),
        TemplatePiece::Any => json!({ "piece": "any" }),
        TemplatePiece::Anchor(kind) => {
            json!({ "piece": "anchor", "kind": assert_kind_name(*kind) })
        }
        TemplatePiece::GroupStart(index) => json!({ "piece": "group_start", "index": index }),
        TemplatePiece::GroupEnd(index) => json!({ "piece": "group_end", "index": index }),
    }
}

/// Renders the parameters of a custom specification as a JSON value.
fn params_json(params: &PatternParams) -> Value {
    json!({
        "case_sensitive": params.case_sensitive,
        "multiline": params.multiline,
        "dot_all": params.dot_all,
        "anchored_start": params.anchored_start,
        "anchored_end": params.anchored_end,
        "min_len": params.min_len,
        "max_len": params.max_len,
    })
}

/// A one-line, human readable summary of an intent specification.
pub fn intent_summary(spec: &IntentSpec) -> String {
    match spec {
        IntentSpec::Confident(candidate) => format!("confident {}", candidate_summary(candidate)),
        IntentSpec::Ambiguous {
            reason,
            suggestions,
        } => format!(
            "ambiguous {} ({} suggestion(s))",
            ambiguity_reason_name(*reason),
            suggestions.len()
        ),
    }
}

/// A short name of a candidate: its family and its kind.
///
/// The short name is what a ranked list uses, where the full summary of
/// every candidate would make the lines unreadable.
pub fn candidate_short_name(candidate: &CandidateSpec) -> String {
    match candidate {
        CandidateSpec::Validate { kind, .. } => {
            format!("validate {}", validation_kind_name(*kind))
        }
        CandidateSpec::Extract { kind, .. } => {
            format!("extract {}", extraction_kind_name(*kind))
        }
        CandidateSpec::Filter { .. } => "filter".to_string(),
        CandidateSpec::Custom { .. } => "custom".to_string(),
    }
}

/// A one-line, human readable summary of a candidate specification.
pub fn candidate_summary(candidate: &CandidateSpec) -> String {
    match candidate {
        CandidateSpec::Validate {
            kind,
            strictness,
            charset,
            max_len,
        } => format!(
            "validate {} strictness={} charset={} max_len={}",
            validation_kind_name(*kind),
            strictness_name(*strictness),
            charset_name(*charset),
            optional_number(max_len.map(u32::from))
        ),
        CandidateSpec::Extract {
            kind,
            boundary,
            capture,
        } => format!(
            "extract {} boundary={} capture={}",
            extraction_kind_name(*kind),
            boundary_name(*boundary),
            capture_name(*capture)
        ),
        CandidateSpec::Filter { action, terms } => {
            let mut names = Vec::new();
            for term in terms.iter() {
                names.push(escape_bytes(term.bytes.as_slice()));
            }
            format!(
                "filter action={} terms={}",
                filter_action_name(*action),
                names.join(",")
            )
        }
        CandidateSpec::Custom { template, params } => format!(
            "custom shape={} count={} min_len={} max_len={}",
            template_shape_name(template),
            template_count(template),
            optional_number(params.min_len.map(u32::from)),
            optional_number(params.max_len.map(u32::from))
        ),
    }
}

/// The name of the shape of a pattern template.
fn template_shape_name(template: &PatternTemplate) -> &'static str {
    match template {
        PatternTemplate::Sequence(_) => "sequence",
        PatternTemplate::Repeat { .. } => "repeat",
        PatternTemplate::LiteralThenClass { .. } => "literal_then_class",
    }
}

/// The number of characters a pattern template asks for.
fn template_count(template: &PatternTemplate) -> u32 {
    match template {
        PatternTemplate::Sequence(pieces) => pieces.len() as u32,
        PatternTemplate::Repeat { count, .. } => u32::from(*count),
        PatternTemplate::LiteralThenClass { literal, count, .. } => {
            literal.len() as u32 + u32::from(*count)
        }
    }
}

/// Renders an optional number as text.
fn optional_number(value: Option<u32>) -> String {
    match value {
        Some(number) => number.to_string(),
        None => "none".to_string(),
    }
}

/// The stable name of a strictness level.
pub fn strictness_name(strictness: Strictness) -> &'static str {
    match strictness {
        Strictness::Strict => "strict",
        Strictness::Standard => "standard",
        Strictness::Lenient => "lenient",
    }
}

/// The stable name of a character repertoire.
pub fn charset_name(charset: CharsetPolicy) -> &'static str {
    match charset {
        CharsetPolicy::Digits => "digits",
        CharsetPolicy::Letters => "letters",
        CharsetPolicy::Alphanumeric => "alphanumeric",
        CharsetPolicy::Word => "word",
        CharsetPolicy::PrintableAscii => "printable_ascii",
        CharsetPolicy::Any => "any",
    }
}

/// The stable name of a boundary policy.
pub fn boundary_name(boundary: BoundaryPolicy) -> &'static str {
    match boundary {
        BoundaryPolicy::Greedy => "greedy",
        BoundaryPolicy::Lazy => "lazy",
        BoundaryPolicy::Exclusive => "exclusive",
    }
}

/// The stable name of a capture policy.
pub fn capture_name(capture: CapturePolicy) -> &'static str {
    match capture {
        CapturePolicy::Whole => "whole",
        CapturePolicy::Group => "group",
    }
}

/// The stable name of a filter action.
pub fn filter_action_name(action: FilterAction) -> &'static str {
    match action {
        FilterAction::Block => "block",
        FilterAction::Allow => "allow",
        FilterAction::Highlight => "highlight",
        FilterAction::Count => "count",
    }
}

/// The stable name of a filter term kind.
pub fn term_kind_name(kind: TermKind) -> &'static str {
    match kind {
        TermKind::Literal => "literal",
        TermKind::Word => "word",
        TermKind::Phrase => "phrase",
    }
}

/// The stable name of an ambiguity reason.
pub fn ambiguity_reason_name(reason: AmbiguityReason) -> &'static str {
    match reason {
        AmbiguityReason::LowConfidence => "low_confidence",
        AmbiguityReason::ConflictingKinds => "conflicting_kinds",
        AmbiguityReason::MissingDetail => "missing_detail",
        AmbiguityReason::InconsistentExamples => "inconsistent_examples",
    }
}

/// The stable name of a zero-width assertion.
pub fn assert_kind_name(kind: AssertKind) -> &'static str {
    match kind {
        AssertKind::StartText => "start_text",
        AssertKind::EndText => "end_text",
        AssertKind::StartLine => "start_line",
        AssertKind::EndLine => "end_line",
        AssertKind::WordBoundary => "word_boundary",
        AssertKind::NotWordBoundary => "not_word_boundary",
    }
}
