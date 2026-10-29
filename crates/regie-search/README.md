# regie-search

The verified hypothesis search of the `regie` project.

The intent model proposes a handful of *structured* specifications for a
request; this crate takes those proposals, mutates them along the axes the
design defines, and verifies every candidate against the examples the caller
supplies. Only a candidate that matches **every** positive example and **no**
negative example survives.

The crate is `no_std`, allocation free by default, uses no `unsafe` and has no
dependencies beyond `regie-core` and `regie-intent`.

```rust
use regie_core::spec::{CandidateSpec, IntentSpec, ValidationKind};
use regie_search::{search, Examples};

let positives = ["123.456.789-09"];
let negatives = ["1234567890a"];
let report = search("validar CPF", Examples { positive: &positives, negative: &negatives });

match report.outcome {
    IntentSpec::Confident(CandidateSpec::Validate { kind, .. }) => {
        assert_eq!(kind, ValidationKind::BrazilianCpf);
    }
    other => panic!("unexpected outcome: {:?}", other),
}
```

## The algorithm

1. **Propose.** The candidates of the intent model are the seeds of the
   search, in the order the model ranked them.
2. **Mutate.** Every seed is mutated along the axes of its family, one axis at
   a time:

   | Family | Axes |
   |---|---|
   | `Validate` | `Strictness` (`Strict` / `Standard` / `Lenient`), `CharsetPolicy` (six policies) |
   | `Extract` | `BoundaryPolicy` (`Greedy` / `Lazy` / `Exclusive`), `CapturePolicy` (`Whole` / `Group`) |
   | `Filter` | `FilterAction` (`Block` / `Allow` / `Highlight` / `Count`) |
   | `Custom` | the character repertoire of the template, the anchoring (`anchored_start` / `anchored_end`) |

   A mutation never changes the family of a candidate, so the family of a
   request is decided by the model alone. The mutations of the mutations are
   explored as well, so a combination of two axes is reachable.
3. **Verify.** Every candidate is compiled with `regie_core::Regex::from_spec`
   and matched against the examples. A candidate that does not compile is
   rejected, not reported as an error.
4. **Score.** A candidate earns 1000 points for every positive example it
   matches and 100 points for every negative example it rejects, so a
   candidate that satisfies every example always scores the most.
5. **Select.** One surviving family yields `IntentSpec::Confident` with the
   best candidate of that family. Several surviving families yield
   `IntentSpec::Ambiguous` with `AmbiguityReason::ConflictingKinds` and the
   best candidate of each family, most promising first. No survivor at all
   yields `IntentSpec::Ambiguous` with
   `AmbiguityReason::InconsistentExamples` and no suggestion: the search
   never invents a specification.

When the caller supplies no example at all, nothing can be verified, so the
search falls back to the ranking of the intent model: the top candidate is
returned as `IntentSpec::Confident`, unless the model itself was ambiguous.

## The budget and the ties

The search explores at most `MAX_EXPLORED` (64) candidates. The proposal set
is filled in breadth-first order - the seeds, then their mutations, then the
mutations of those mutations - and stops as soon as it holds 64 candidates.
The set never holds the same candidate twice.

Ties are broken by the *canonical proposal index*: the position of a
candidate in the proposal set. The index depends on the proposal alone, so
the outcome never depends on the order in which the candidates happened to be
explored. The seed the model ranked first therefore wins a tie inside its
family.

## More examples

The gallery of [`EXAMPLES.md`](EXAMPLES.md) documents every example of the
crate, with the output each one prints.  It also has the hard discrimination:
the example sets that separate families that look alike, and the shapes that
no family accepts, where the search refuses to settle instead of guessing.

```console
$ cargo run -p regie-search --example discriminate
request: "extrair o texto" - bracketed
  positives: ["[42]", "[valor]"]
  negatives: ["abc", "42"]
  explored: 24, rejected: 18
  outcome: confident extract between_delimiters (Greedy, Group)
  ...
```

## Features

| Feature | Effect |
|---|---|
| `alloc` | forwards to `regie-core/alloc` |
| `std` | forwards to `regie-core/std` |
| `mcts` | replaces the flat enumeration with a bounded tree search |

With `mcts`, the search walks the *mutation tree* and expands the most
promising branch first: the roots are the seeds, every other node is a
mutation of its parent, and the node expanded next is the one whose parent
scored highest, with the canonical proposal index breaking the ties. The
search stays deterministic - there is no randomness and no rollout - and the
visit budget covers the whole proposal set, so the tree search and the flat
enumeration reach the same outcome. `search_flat` is always available and is
the reference implementation.
