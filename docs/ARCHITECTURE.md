# Architecture

This document explains how `regie` is put together and, more importantly, why
it is put together this way. The short version: every design decision below
exists to make the engine's behaviour predictable enough to be trusted on a
device that cannot be restarted.

## The shape of the workspace

```text
                     regie-core
                    /    |     \
          regie-intent  |   regie-codec
                    \    |     /
                    regie-search
                          |
                      regie-cli
```

The graph is a line, and it is a line on purpose.

`regie-core` is the engine and nothing else. It has no dependency on the
intent layer, on a file format, on a model, or on the standard library. That
means the part of the project that has to be correct on an embedded target
has no surface area that the interesting parts of the project can reach.

`regie-intent` depends on `regie-core` for the *types* of the specification
and for nothing else. It never matches anything.

`regie-search` depends on both: it takes the candidates the model proposes
and checks them with the engine.

`regie-codec` depends on `regie-core` for the `Dfa` type, and only for that.

`regie-cli` is the only crate that depends on everything, and it is the only
one that needs an operating system.

## The specification is the contract

The central type of the project is `CandidateSpec`, in
`regie_core::spec`:

```rust
pub enum CandidateSpec {
    Validate { kind: ValidationKind, strictness: Strictness, charset: CharsetPolicy, max_len: Option<u16> },
    Extract  { kind: ExtractionKind, boundary: BoundaryPolicy, capture: CapturePolicy },
    Filter   { action: FilterAction, terms: SmallVec<PatternTerm, 4> },
    Custom   { template: PatternTemplate, params: PatternParams },
}
```

Every field is an enumerated choice or a bounded inline value. There is no
string of pattern syntax anywhere in it. This is the whole reason the intent
layer can be allowed to exist at all:

* The model chooses a variant and fills in the enumerated fields. It cannot
  write a pattern, because there is nowhere to write one.
* The compiler in `regie-core` turns the specification into an automaton
  directly, with no textual intermediate step. There is no parser in that
  path, so there is no injection.
* Every specification the model can produce is one the compiler is known to
  handle. `VALIDATION_KINDS` has 31 entries, `EXTRACTION_KINDS` has 10, and
  the compiler has a test that compiles all 41 of them.

`IntentSpec` wraps a `CandidateSpec` with the possibility of refusing to
choose:

```rust
pub enum IntentSpec {
    Confident(CandidateSpec),
    Ambiguous { reason: AmbiguityReason, suggestions: SmallVec<CandidateSpec, 3> },
}
```

The `Ambiguous` variant is not an error case. It is the answer when the
request is not specific enough, and it carries the candidates that survived,
so the caller can ask the user which one they meant.

## The engine

### Parsing

`parse.rs` is a recursive-descent parser over the bytes of the pattern. It
produces a `HirArena`: a fixed-capacity array of `HirNode`s, where children
are arena indices. The tree is deliberately shallow — `Concat` and
`Alternate` are binary — and it is `Copy`, so a compiled pattern never
touches the allocator.

The parser refuses rather than reinterprets. Look-around, backreferences,
named groups, possessive quantifiers and unknown escapes are all errors with
a position and a name. The reasoning is in the crate documentation: a
construct whose behaviour depends on the rest of the pattern is a construct
whose behaviour you cannot reason about.

### Compilation

`program.rs` lowers the tree into a program of instructions:

```rust
enum Op { Char, Assert, Split, Save, Match }
```

This is a Thompson NFA. The important property is not the representation but
what it makes impossible: there is no backtracking stack, so there is no
input for which the matcher takes superlinear time. The `Split` instruction
carries both alternatives; both are explored, in parallel, in one pass.

### Matching

`matcher.rs` is a Pike VM. It keeps a list of threads — each one a program
counter, a start offset and a copy of the capture slots — and advances all
of them one input byte at a time. The list is capped at 256 threads; when a
pattern would need more, the match fails with `ThreadLimitExceeded` rather
than growing without bound.

The capture slots are inline: eight groups, sixteen slots, `u32::MAX` for
"not set". A thread's captures are copied when the thread forks, so a
capture never needs a heap.

Match selection is leftmost-first, like every other regular expression
engine: among the matches that start at the same position, the branch the
pattern prefers wins. The implementation is simple to state and easy to
verify: a later match always replaces the provisional one, and threads that
are worse than the provisional match are dropped when they reach the `Match`
instruction.

### The deterministic automaton

`dfa_build.rs` runs a subset construction over the program, indexed by byte
equivalence classes rather than by raw byte. Two bytes share a class exactly
when they belong to the same set of character classes, which is exactly when
the automaton cannot tell them apart. A pattern that distinguishes only
`[a-z]` from everything else needs two classes, and therefore a transition
table of `states × 2` entries.

The automaton is unanchored by default: every state includes the program
start, so one left-to-right pass finds a match anywhere. A pattern that
starts with `^` is anchored instead, and a pattern that ends with `$` sets
the `accept_at_end` flag, so that a match may not be followed by more bytes.

Every other zero-width assertion is refused with `Unsupported`. `\b` in the
middle of a pattern cannot be expressed by a deterministic automaton without
look-around, and silently compiling it into something else would be a lie.

The automaton has a fixed capacity: 256 states, 32 classes, 16 KiB of
transition table. A pattern that does not fit is reported with
`DfaTooLarge` and remains usable through the NFA engine.

## The intent layer

### The model

`regie-intent` is a quantized Bernoulli Naive Bayes over binary lexical
features. The features of a request are its tokens, its pairs of adjacent
tokens, and a few coarse shape buckets (`#tok=5`, `#len=11-20`, `#digits`,
`#accent`).

The scoring is integer-only. Every weight is an `i8` in a `&'static` table,
and the decision thresholds are `i32`s, so the argmax and the margin
comparison are exact integer operations. There is no floating point in the
inference path, which is what makes the answer identical on every platform.

The vocabulary is 1,419 features and there are 43 classes, so the weight
table is 61,189 bytes and the whole model, vocabulary included, is 97 KiB.

### The training

`tools/train_intent.py` is standard-library only and fully deterministic: the
corpus is a fixed table in the script, the split is a fixed per-class rule,
and the fit is closed-form. Running it twice produces byte-identical
output.

The corpus is generated from templates with the nouns rotated underneath
them. That is deliberate. If every request of a family shared the same verb,
the model would learn the verb and the family would not matter; rotating the
nouns forces it to learn the words that actually describe the value.

### The decision

The model returns its best class, but only when the winner is far enough
ahead of the runner-up: a minimum score of 12 and a minimum margin of 52.
Otherwise the layer returns `Ambiguous`. On the held-out split of the
dataset, 95.8% of the requests are resolved confidently and 100% get the
right class — the remaining 4.2% are refused rather than guessed, which is
the intended behaviour.

## The verified search

`regie-search` closes the loop between a request and the examples the user
has.

The candidates the intent model proposes are the seeds. Each seed is mutated
along the axes of its family — strictness and charset for a validation,
boundary and capture for an extraction, the action for a filter, the
template and the anchoring for a custom pattern — and the mutations of the
mutations are explored too, so that combinations of axes are reachable.

Every candidate is compiled with `Regex::from_spec` and matched against the
examples. A candidate that does not compile is rejected, not an error: the
specification was legal but the engine could not build it, and that is
information about the candidate, not about the caller.

A candidate is *consistent* when it matches every positive example and
rejects every negative one. One surviving family gives a confident answer;
several genuinely different families give `Ambiguous` with the best
candidate of each family; none gives `Ambiguous` with no suggestion.

The budget is 64 explored candidates. The proposal set is filled
breadth-first and deduplicated, so the search is deterministic, and ties are
broken by the canonical proposal index, so the outcome never depends on the
order of exploration.

## The artifact format

`regie-codec` serializes an automaton into a `RGDF` blob. The format is
described byte by byte in the crate documentation; the properties that matter
are:

* **Explicit endianness.** Every multi-byte integer is little-endian,
  whatever the host that produced or consumed the artifact, and the header
  carries a marker that says so.
* **A checksum.** A CRC-32 (IEEE) over everything before it, so a truncated
  or corrupted artifact is detected rather than misread.
* **Strict decoding.** A decoder rejects a wrong magic, an unknown version,
  unknown flag bits, an automaton larger than the engine supports, a
  truncated payload, a trailing byte and a bad checksum. No malformed input
  can panic.
* **Determinism.** The same automaton always encodes to the same bytes.

## What is deliberately absent

* **Backtracking.** There is none, by construction.
* **Look-around.** Rejected, because it is where predictability goes to die.
* **An allocator.** Not on the default path of any crate.
* **Floating point.** Not in the matching path, and not in the model.
* **A network.** Nothing in the workspace talks to anything.
* **`unsafe`.** Forbidden in every crate.

## The testing strategy

The engine is tested at four levels, and the levels answer different
questions:

1. **Unit and integration tests** pin the behaviour: 176 tests across the
   workspace, covering the matcher, the parser, the compiler, the automaton,
   the model, the search, the codec and the command line.
2. **A differential test** compares the engine against Python's `re` on
   21,323 cases. It compares not only the spans of the matches, but also the
   cases where neither engine matches and the cases where both refuse the
   pattern.
3. **Property tests** check the invariants that hold for every input: a
   match is reproducible, `find` agrees with `is_match`, the captures agree
   with the span, and neither compiling nor matching ever panics.
4. **Fuzzing** attacks the two entry points that see untrusted bytes: the
   parser and the compiler.

Coverage of `regie-core` is measured with `cargo llvm-cov` and is held
above 90% by the CI.

## Where to go next

[EVOLUTION.md](EVOLUTION.md) lists what is planned and what is not.
