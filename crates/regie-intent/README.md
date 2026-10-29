# regie-intent

The micro-model of [`regie`](../../README.md): it turns a request written in
words into a structured specification, and it refuses to guess when the
request is not clear enough.

```toml
[dependencies]
regie-intent = "0.1"
```

## What it is

A quantized Bernoulli naive Bayes over binary lexical features. The
features of a request are its tokens, its pairs of adjacent tokens and a
few coarse shape buckets.

The scoring is integer-only: every weight is an `i8` in a `static` table
and the thresholds are `i32`, so the answer is exact and identical on every
platform. There is no floating point in the inference path.

The model has 1,419 features over 43 classes. The weight table is 61,189
bytes and the whole model, vocabulary included, is 97 KiB, against a budget
of 256 KiB.

## The API

```rust
use regie_core::spec::IntentSpec;
use regie_intent::IntentModel;

let model = IntentModel::embedded();

match model.interpret("validar um CPF") {
    IntentSpec::Confident(spec) => {
        let re = regie_core::Regex::from_spec(&spec).expect("the specification compiles");
        assert!(re.is_match(b"123.456.789-09"));
    }
    IntentSpec::Ambiguous { reason, suggestions } => {
        // The model refused to guess.  This is a normal outcome.
        println!("ambiguous: {reason:?} ({})", suggestions.len());
    }
}
```

The model never produces pattern text. It chooses one of the enumerated
families and fills in the enumerated fields, so it cannot smuggle syntax
into the engine.

## The candidates

`candidates` returns the families that survived the scoring, which is what
the verified search uses as its seeds:

```rust
let candidates = model.candidates("extrair os numeros");
assert!(!candidates.is_empty());
```

## More examples

The gallery of [`EXAMPLES.md`](EXAMPLES.md) documents every example of the
crate, with the output each one prints.

```console
$ cargo run -p regie-intent --example spec_to_regex
request: "validar um CPF"
  intent:   confident
  spec:     validate brazilian_cpf (strictness=Standard, charset=Digits, max_len=Some(14))
  compiled: 0 capture groups
  "123.456.789-09" -> match
  "not a cpf" -> no match
  ...
```

## The training

The corpus, the trainer and the weights are in the workspace:

```console
python tools/train_intent.py --emit-dataset   # rewrite the dataset
python tools/train_intent.py                   # rewrite the weights
```

The trainer is standard-library only and fully deterministic: the corpus is
a table in the script, the split is a fixed per-class rule and the fit is
closed-form. Running it twice produces byte-identical output.

The held-out split of the dataset is at 100% accuracy and 95.8% resolved.
That number says little about the model on requests the corpus does not
contain, because the corpus is generated from templates; see
[docs/EVOLUTION.md](../../docs/EVOLUTION.md).

## The Cargo features

| feature | effect |
|---|---|
| *(none)* | `no_std`, no allocator |
| `alloc` | the heap-backed helpers |
| `std` | `std::error::Error` |
| `serde` | forwards to `regie-core/serde`, which derives `Serialize` and `Deserialize` for the **specification types**. The model itself is not serializable. |

## The tests

```console
cargo test -p regie-intent --all-features
```

* `tests/behaviour.rs` - what the model answers, and the size of the model.
* `tests/dataset.rs` - the model reproduces the dataset.
* `tests/consistency.rs` - every record of the dataset agrees with the
  engine: the specification of a record accepts the values the dataset
  attributes to it and refuses the others.
