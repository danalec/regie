# regie-core

The engine of [`regie`](../../README.md): a safe regex dialect, a Thompson
NFA, a Pike VM and a compact deterministic automaton.

```toml
[dependencies]
regie-core = "0.1"
```

## What it guarantees

* **No backtracking.** The matcher simulates an automaton; a pattern of `m`
  instructions over an input of `n` bytes takes `O(m · n)` time, always.
* **No allocator**, unless the `alloc` feature is on. A compiled pattern
  lives in a fixed-size value.
* **No panics on user input.** Every failure is a variant of `RegexError`.
* **No `unsafe`.** The crate is `#![forbid(unsafe_code)]`.

## The API

```rust
use regie_core::Regex;

let re = Regex::new(r"^\d{3}-\d{4}$").expect("the pattern compiles");

assert!(re.is_match(b"555-1234"));
assert!(!re.is_match(b"5551234"));

let re = Regex::new(r"(\w+)@(\w+)").expect("the pattern compiles");
let caps = re.captures(b"mail: ana@corp").expect("the pattern matches");
assert_eq!(caps.get(1).expect("group 1 took part").start, 6);
```

A pattern can also be compiled from a structured specification, which is the
path the intent layer takes:

```rust
use regie_core::spec::{CandidateSpec, CharsetPolicy, Strictness, ValidationKind};
use regie_core::Regex;

let spec = CandidateSpec::Validate {
    kind: ValidationKind::BrazilianCpf,
    strictness: Strictness::Standard,
    charset: CharsetPolicy::Digits,
    max_len: Some(14),
};

let re = Regex::from_spec(&spec).expect("the specification compiles");
assert!(re.is_match(b"123.456.789-09"));
```

## More examples

The gallery of [`EXAMPLES.md`](EXAMPLES.md) has one runnable program per
subject: the dialect, the capture groups, the structured specifications, the
refusals, the budgets, the deterministic automaton and the error type.

Matching a pattern:

```rust
use regie_core::Regex;

let re = Regex::new(r"^\d{3}-\d{4}$").expect("the pattern compiles");
for input in ["555-1234", "5551234"] {
    println!("{input}: {}", re.is_match(input.as_bytes()));
}
```

```console
555-1234: true
5551234: false
```

Compiling a specification:

```rust
use regie_core::spec::{CandidateSpec, CharsetPolicy, Strictness, ValidationKind};
use regie_core::Regex;

let spec = CandidateSpec::Validate {
    kind: ValidationKind::Email,
    strictness: Strictness::Standard,
    charset: CharsetPolicy::Any,
    max_len: None,
};

let re = Regex::from_spec(&spec).expect("the specification compiles");
println!("ana@corp.com: {}", re.is_match(b"ana@corp.com"));
println!("not-an-email:  {}", re.is_match(b"not-an-email"));
```

```console
ana@corp.com: true
not-an-email:  false
```

## The features

| feature | effect |
|---|---|
| *(none)* | `no_std`, no allocator: the parser, the compiler and the Pike VM |
| `alloc` | the deterministic automaton builder |
| `std` | `std::error::Error` for `RegexError` |
| `serde` | `Serialize` and `Deserialize` for the specification types |
| `unicode` | reserved for the UTF-8 aware classes |

## The deterministic automaton

With the `alloc` feature, a pattern can be determinized into a table that
is indexed by byte class:

```rust
let dfa = re.build_dfa().expect("the automaton builds");
assert!(dfa.is_match(b"123.456.789-09"));
```

The automaton is the form that is meant to be linked into a firmware image;
[`regie-codec`](../regie-codec) serializes it.

## The dialect

The dialect is small on purpose, and it refuses the constructs that make
matching unpredictable. The full grammar and the complete list of
refusals are in [docs/SYNTAX.md](../../docs/SYNTAX.md).

## The limits

Every value of the engine has a size that is known at compile time: a
pattern is at most 512 bytes, a pattern has at most eight capture groups,
the compiled program has at most 512 instructions, a match uses at most
256 threads and the automaton has at most 256 states. The table is in
[docs/SYNTAX.md](../../docs/SYNTAX.md).

## The tests

```console
cargo test -p regie-core --all-features
```

* `tests/engine.rs` - the behaviour of the matcher.
* `tests/api.rs` - the whole public surface: every error renders a message,
  the inline vector behaves like a vector, every family has a name.
* `tests/specs.rs` - every validation and extraction family accepts what it
  should and refuses what it should.
* `tests/dfa.rs` - the deterministic automaton.
* `tests/property.rs` - the invariants that hold for every input.

The differential test against Python's `re` lives in the workspace and
covers 21,980 cases.
