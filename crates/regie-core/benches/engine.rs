//! Benchmarks for the `regie-core` engine.
//!
//! Run with `cargo bench -p regie-core`.

// The criterion macros generate functions of their own.
#![allow(missing_docs)]

use criterion::{criterion_group, criterion_main, Criterion};
use regie_core::Regex;

fn haystack(size: usize, filler: u8) -> Vec<u8> {
    let mut out = Vec::with_capacity(size);
    while out.len() < size {
        out.push(filler);
    }
    out
}

fn bench_literal(c: &mut Criterion) {
    let text = haystack(4096, b'a');
    let re = Regex::new("needle").expect("pattern should compile");
    c.bench_function("literal/search", |b| b.iter(|| re.is_match(&text)));
}

fn bench_literal_hit(c: &mut Criterion) {
    let mut text = haystack(2048, b'a');
    text.extend_from_slice(b"needle");
    text.extend_from_slice(&haystack(2048, b'a'));
    let re = Regex::new("needle").expect("pattern should compile");
    c.bench_function("literal/hit", |b| b.iter(|| re.is_match(&text)));
}

fn bench_alternation(c: &mut Criterion) {
    let text = haystack(4096, b'a');
    let re = Regex::new("(alpha|beta|gamma|delta)+").expect("pattern should compile");
    c.bench_function("alternation", |b| b.iter(|| re.is_match(&text)));
}

fn bench_character_class(c: &mut Criterion) {
    let text = haystack(4096, b'a');
    let re = Regex::new("[a-zA-Z0-9_]+").expect("pattern should compile");
    c.bench_function("character_class", |b| b.iter(|| re.is_match(&text)));
}

fn bench_repetition(c: &mut Criterion) {
    let text = haystack(4096, b'a');
    let re = Regex::new("a{2,8}b").expect("pattern should compile");
    c.bench_function("repetition", |b| b.iter(|| re.is_match(&text)));
}

fn bench_captures(c: &mut Criterion) {
    let text = b"mail: ana@corp.com, bob@corp.com".to_vec();
    let re = Regex::new("(\\w+)@(\\w+)\\.com").expect("pattern should compile");
    c.bench_function("captures", |b| {
        b.iter(|| re.captures(&text).and_then(|c| c.get(1)))
    });
}

fn bench_find(c: &mut Criterion) {
    let text = haystack(4096, b'a');
    let re = Regex::new("needle").expect("pattern should compile");
    c.bench_function("find", |b| b.iter(|| re.find(&text)));
}

#[cfg(feature = "alloc")]
fn bench_precompiled_dfa(c: &mut Criterion) {
    let re = Regex::new("needle").expect("pattern should compile");
    let dfa = re.build_dfa().expect("dfa should build");
    let text = haystack(4096, b'a');
    c.bench_function("dfa/precompiled", |b| b.iter(|| dfa.is_match(&text)));
}

#[cfg(feature = "alloc")]
fn bench_spec_compile(c: &mut Criterion) {
    use regie_core::spec::{CandidateSpec, CharsetPolicy, Strictness, ValidationKind};
    let spec = CandidateSpec::Validate {
        kind: ValidationKind::BrazilianPostalCode,
        strictness: Strictness::Standard,
        charset: CharsetPolicy::Digits,
        max_len: Some(9),
    };
    c.bench_function("compile/specification", |b| {
        b.iter(|| Regex::from_spec(&spec))
    });
}

fn bench_compile(c: &mut Criterion) {
    c.bench_function("compile/pattern", |b| {
        b.iter(|| Regex::new("^[a-z0-9._%+-]+@[a-z0-9.-]+\\.[a-z]{2,}$"))
    });
}

/// The same patterns, measured against `regex-automata`.
///
/// The two engines do the same work on the same input, so the numbers are
/// comparable.  `regex-automata` is the reference implementation of this
/// family of engines, and it is the one `regie` is measured against.
fn bench_comparison(c: &mut Criterion) {
    use regex_automata::meta::Regex as MetaRegex;

    let cases: &[(&str, &str)] = &[
        ("literal", "needle"),
        ("email", "^[a-z0-9._%+-]+@[a-z0-9.-]+\\.[a-z]{2,}$"),
        ("alternation", "(alpha|beta|gamma|delta)+"),
        ("class", "[a-zA-Z0-9_]+"),
        ("repetition", "a{2,8}b"),
    ];

    for (name, pattern) in cases {
        let text = haystack(4096, b'a');

        let ours = Regex::new(pattern).expect("pattern should compile");
        c.bench_function(&format!("comparison/{name}/regie"), |b| {
            b.iter(|| ours.is_match(&text))
        });

        let theirs = MetaRegex::new(pattern).expect("pattern should compile");
        c.bench_function(&format!("comparison/{name}/regex-automata"), |b| {
            b.iter(|| theirs.is_match(&text))
        });
    }
}

criterion_group!(
    benches,
    bench_literal,
    bench_literal_hit,
    bench_alternation,
    bench_character_class,
    bench_repetition,
    bench_captures,
    bench_find,
    bench_compile,
    bench_comparison,
);
// The benchmarks that need an allocator are only there when the `alloc`
// feature is on, so the suite runs either way.
#[cfg(not(feature = "alloc"))]
fn bench_precompiled_dfa(_c: &mut Criterion) {}

#[cfg(not(feature = "alloc"))]
fn bench_spec_compile(_c: &mut Criterion) {}

criterion_group!(benches_alloc, bench_precompiled_dfa, bench_spec_compile);

criterion_main!(benches, benches_alloc);
