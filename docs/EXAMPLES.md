# Examples

This page is the way in. It shows the whole project in one place, then points
at the per-crate galleries where every example lives in full.

The short version: **`regie` matches text with a pattern, or with a
specification it derives from what you asked for.** Everything else is
detail.

## Start here

Three commands, from the repository root:

```console
$ cargo run -p regie-cli --bin regie -- match '(\w+)@(\w+\.\w+)' 'mail: ana@corp.com'
match: 6..18 "ana@corp.com"
group 1: 6..9 "ana"
group 2: 10..18 "corp.com"

$ cargo run -p regie-cli --bin regie -- intent 'validar CPF'
summary: confident validate brazilian_cpf strictness=standard charset=digits max_len=14
{"candidate":{"charset":"digits","kind":"brazilian_cpf","max_len":14,"strictness":"standard","type":"validate"},"status":"confident"}

$ cargo run -p regie-cli --bin regie -- search 'validar CPF' --positive '123.456.789-09' --negative '123.456.789'
search: confident
candidate: validate brazilian_cpf strictness=standard charset=digits max_len=14
rank 1: validate brazilian_cpf score=1100 positives=1/1 negatives_rejected=1/1
rank 2: validate brazilian_cpf score=1100 positives=1/1 negatives_rejected=1/1
rank 3: validate brazilian_cpf score=1100 positives=1/1 negatives_rejected=1/1
rank 4: validate brazilian_cpf score=1100 positives=1/1 negatives_rejected=1/1
explored: 64 candidates, 52 rejected
```

The first command matches a pattern. The second turns a sentence into a
specification. The third narrows a sentence down with examples you supply.

## Pick your entry point

| What you want | What you use | Where |
|---|---|---|
| Match a pattern you already know | `regie-core` | [core gallery](crates/regie-core/EXAMPLES.md) |
| Turn "check whether this is a CPF" into a specification | `regie-intent` | [intent gallery](crates/regie-intent/EXAMPLES.md) |
| Find the specification that fits my examples | `regie-search` | [search gallery](crates/regie-search/EXAMPLES.md) |
| Ship a matcher inside firmware | `regie-codec` | [codec gallery](crates/regie-codec/EXAMPLES.md) |
| Try any of the above from a shell | `regie-cli` | [cli gallery](crates/regie-cli/EXAMPLES.md) |

If you are unsure, start with `regie-core`: it is the only crate with no
dependencies, and everything else is built on top of it.

## The four layers, end to end

One request, through every layer:

```text
"validar CPF"
     │
     ▼
regie-intent  ──► Confident(Validate { kind: BrazilianCpf, strictness: Standard,
                  charset: Digits, max_len: 14 })
     │
     ▼
regie-search  ──► the same specification, checked against your examples
     │
     ▼
regie-core    ──► a Thompson program and a Pike VM, or a compact DFA
     │
     ▼
regie-codec   ──► an `RGDF` blob that is linked into the firmware image
```

The model never produces pattern text. It selects one value out of a closed
enumeration, and the deterministic compiler turns that value into an
automaton. A model that guesses wrong therefore produces a *wrong but valid*
specification, never a broken or dangerous pattern.

## From the shell

The `regie` binary has six subcommands. The exit code carries the verdict:
`0` for success, `1` for "no match" or "could not settle", `2` for an error.

### `match` — find a pattern in a text

```console
$ cargo run -p regie-cli --bin regie -- match '(\w+)@(\w+\.\w+)' 'mail: ana@corp.com'
match: 6..18 "ana@corp.com"
group 1: 6..9 "ana"
group 2: 10..18 "corp.com"
```

When nothing matches, the tool says so and exits with `1`:

```console
$ cargo run -p regie-cli --bin regie -- match '^\d{3}-\d{2}$' '123-456'
no match
```

### `test` — use a pattern as a predicate

`test` prints one word and nothing else, which makes it useful in a pipeline:

```console
$ cargo run -p regie-cli --bin regie -- test '^\d{3}-\d{2}$' '123-45'
match
```

### `intent` — turn a sentence into a specification

```console
$ cargo run -p regie-cli --bin regie -- intent 'validar CPF'
summary: confident validate brazilian_cpf strictness=standard charset=digits max_len=14
{"candidate":{"charset":"digits","kind":"brazilian_cpf","max_len":14,"strictness":"standard","type":"validate"},"status":"confident"}
```

When the model is not sure, it says so and offers alternatives. That is a
normal outcome, not a failure:

```console
$ cargo run -p regie-cli --bin regie -- intent 'qwertyuiop zzzz'
summary: ambiguous low_confidence (0 suggestion(s))
{"reason":"low_confidence","status":"ambiguous","suggestions":[]}
```

### `search` — narrow a request down with examples

```console
$ cargo run -p regie-cli --bin regie -- search 'validar CPF' --positive '123.456.789-09' --negative '123.456.789'
search: confident
candidate: validate brazilian_cpf strictness=standard charset=digits max_len=14
rank 1: validate brazilian_cpf score=1100 positives=1/1 negatives_rejected=1/1
rank 2: validate brazilian_cpf score=1100 positives=1/1 negatives_rejected=1/1
rank 3: validate brazilian_cpf score=1100 positives=1/1 negatives_rejected=1/1
rank 4: validate brazilian_cpf score=1100 positives=1/1 negatives_rejected=1/1
explored: 64 candidates, 52 rejected
```

### `dfa` and `decode` — the firmware path

Compile a pattern into an artifact on a workstation, then match against the
artifact without compiling anything at run time:

```console
$ cargo run -p regie-cli --bin regie -- dfa '^ana@corp\.com$' ana.rdf
wrote 558 bytes to ana.rdf (14 states, 10 byte classes)

$ cargo run -p regie-cli --bin regie -- decode ana.rdf 'ana@corp.com'
artifact: 558 bytes, 14 states, 10 byte classes
match
```

A pattern the engine refuses is an error, never a crash:

```console
$ cargo run -p regie-cli --bin regie -- match '(?=ahead)' 'x'
error: invalid pattern `(?=ahead)`: unsupported construct `look-ahead` at byte 2
```

## In your own code

### Match a pattern

```rust
use regie_core::Regex;

let re = Regex::new(r"^\d{3}-\d{4}$").expect("the pattern compiles");
assert!(re.is_match(b"555-1234"));
assert!(!re.is_match(b"5551234"));

let caps = Regex::new(r"(\w+)@(\w+)").expect("the pattern compiles")
    .captures(b"mail: ana@corp").expect("the pattern matches");
assert_eq!(caps.get(1).expect("group 1 took part").start, 6);
```

### Compile a specification

A specification is a value, not a string. The model produces one, and so can
you:

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
assert!(re.is_match(b"ana@corp.com"));
assert!(!re.is_match(b"not-an-email"));
```

### Let the model interpret a request

```rust
use regie_core::spec::IntentSpec;
use regie_intent::IntentModel;

match IntentModel::embedded().interpret("validar um CPF") {
    IntentSpec::Confident(spec) => {
        let re = regie_core::Regex::from_spec(&spec).expect("the specification compiles");
        assert!(re.is_match(b"123.456.789-09"));
    }
    IntentSpec::Ambiguous { reason, .. } => {
        // The model refused to guess.  This is a normal outcome.
        println!("ambiguous: {reason:?}");
    }
}
```

### Narrow a request down with examples

```rust
use regie_search::{search, Examples};

let positives = ["123.456.789-09"];
let negatives = ["123.456.789"];
let report = search("validar CPF", Examples::new(&positives, &negatives));

assert!(report.is_confident());
```

### Embed an automaton in firmware

```rust
use regie_codec::decode;

// A `static` blob, produced by `regie dfa <pattern> <file>`.
static ARTIFACT: &[u8] = include_bytes!("ana.rdf");

let dfa = decode(ARTIFACT).expect("the artifact is well formed");
assert!(dfa.is_match(b"ana@corp.com"));
```

The decode path needs no allocator, which is why the artifact is embedded with
`include_bytes!` and handed to the decoder as a slice.

## The galleries

Every example in the project, in one place per crate:

* [regie-core](crates/regie-core/EXAMPLES.md) — the dialect, capture groups,
  specifications, refusals, limits, the DFA, the error type.
* [regie-intent](crates/regie-intent/EXAMPLES.md) — interpreting a request,
  the ambiguity path, the model profile.
* [regie-search](crates/regie-search/EXAMPLES.md) — searching with examples,
  discriminating between candidates, refusing to settle.
* [regie-codec](crates/regie-codec/EXAMPLES.md) — the artifact layout, the
  round trip, a corrupted artifact.
* [regie-cli](crates/regie-cli/EXAMPLES.md) — every subcommand, and scripting
  with the exit codes.

The reference for the pattern language itself is
[docs/SYNTAX.md](SYNTAX.md), with the complete list of what the dialect
accepts and what it refuses.

Every quoted output in these galleries is checked. The command
`python tools/check_examples.py` runs each one and compares what it prints
with what is written here, so a gallery cannot drift away from the code.
