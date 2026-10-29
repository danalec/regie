# Examples of `regie-search`

`regie-search` is the verified hypothesis search of the project: the intent
model proposes a handful of *structured* specifications, the search mutates
them along the axes of their family and verifies every candidate against the
examples the caller supplies.  Only a candidate that matches **every** positive
example and **no** negative example survives.  When nothing survives, the
search returns an ambiguity instead of a guess.

Run any example from the root of the workspace:

```console
$ cargo run -p regie-search --example <name>
```

Every run is deterministic: there is no randomness, no clock and no hash
iteration order, so the same request and the same examples always produce the
same bytes, and the counters (`explored`, `rejected`) are the same on every
run.

The counters are the cost of the search: `explored` is the number of
candidates the search compiled and ran against the examples, and `rejected` is
the number of those that contradicted them.

The example that shows the search refusing to settle is called `refusal` and
not `ambiguity`, because Cargo writes the examples of every crate of a
workspace into one directory: two examples with the same name would
overwrite each other, and the crate `regie-intent` has an `ambiguity` example
of its own.

## `search`

What it shows: the search in action - the specification it settles on for a
request with examples, and the two ways it refuses to settle.

Run it:

```console
$ cargo run -p regie-search --example search
request: "validar um CPF"
  positives: ["123.456.789-09", "987.654.321-00"]
  negatives: ["123.456.789", "not a cpf at all"]
  explored: 64, rejected: 52
  outcome: confident brazilian_cpf
  rank 1: brazilian_cpf score=2200 positives=2/2 negatives_rejected=2/2
  rank 2: brazilian_cpf score=2200 positives=2/2 negatives_rejected=2/2
  rank 3: brazilian_cpf score=2200 positives=2/2 negatives_rejected=2/2
  rank 4: brazilian_cpf score=2200 positives=2/2 negatives_rejected=2/2

request: "validate something"
  positives: ["123-45"]
  negatives: []
  explored: 64, rejected: 64
  outcome: ambiguous (InconsistentExamples)
  rank 1: url score=0 positives=0/1 negatives_rejected=0/0
  rank 2: uri score=0 positives=0/1 negatives_rejected=0/0
  rank 3: brazilian_cpf score=0 positives=0/1 negatives_rejected=0/0
  rank 4: brazilian_cnpj score=0 positives=0/1 negatives_rejected=0/0

request: "validar um CPF"
  positives: ["this is not a cpf"]
  negatives: ["123.456.789-09"]
  explored: 64, rejected: 58
  outcome: confident custom
  rank 1: custom score=1100 positives=1/1 negatives_rejected=1/1
  rank 2: custom score=1100 positives=1/1 negatives_rejected=1/1
  rank 3: custom score=1100 positives=1/1 negatives_rejected=1/1
  rank 4: custom score=1100 positives=1/1 negatives_rejected=1/1
```

Source: [`crates/regie-search/examples/search.rs`](examples/search.rs)

## `discriminate`

What it shows: the same request with two different example sets, producing
two different specifications that both fit the examples they were given.

Run it:

```console
$ cargo run -p regie-search --example discriminate
request: "extrair o texto" - bracketed
  positives: ["[42]", "[valor]"]
  negatives: ["abc", "42"]
  explored: 24, rejected: 18
  outcome: confident extract between_delimiters (Greedy, Group)
  ranking:
    1. extract between_delimiters (Greedy, Group) score=2200 positives=2/2 negatives=2/2
    2. extract between_delimiters (Lazy, Group) score=2200 positives=2/2 negatives=2/2
    3. extract between_delimiters (Exclusive, Group) score=2200 positives=2/2 negatives=2/2
    4. extract between_delimiters (Greedy, Whole) score=2200 positives=2/2 negatives=2/2
  proof:
    "[42]" -> match
    "[valor]" -> match
    "abc" -> no match
    "42" -> no match

request: "extrair o texto" - a bare number
  positives: ["42", "7"]
  negatives: ["abc", "xyz"]
  explored: 24, rejected: 18
  outcome: confident extract numbers (Greedy, Whole)
  ranking:
    1. extract numbers (Greedy, Whole) score=2200 positives=2/2 negatives=2/2
    2. extract numbers (Lazy, Whole) score=2200 positives=2/2 negatives=2/2
    3. extract numbers (Exclusive, Whole) score=2200 positives=2/2 negatives=2/2
    4. extract numbers (Greedy, Group) score=2200 positives=2/2 negatives=2/2
  proof:
    "42" -> match
    "7" -> match
    "abc" -> no match
    "xyz" -> no match
```

Source: [`crates/regie-search/examples/discriminate.rs`](examples/discriminate.rs)

## `refusal`

What it shows: the search refusing to settle - examples no candidate
satisfies, and examples more than one candidate satisfies.

Run it:

```console
$ cargo run -p regie-search --example refusal
request: "validar um CPF" - no candidate accepts the positive sample
  positives: ["zzz"]
  negatives: ["123.456.789-09"]
  explored: 64, rejected: 64
  outcome: ambiguous (InconsistentExamples)
  suggestions: (none)
  ranking:
    1. custom score=100 positives=0/1 negatives=1/1
    2. validate currency_amount (Standard) score=100 positives=0/1 negatives=1/1
    3. validate brazilian_cpf (Lenient) score=100 positives=0/1 negatives=1/1
    4. custom score=100 positives=0/1 negatives=1/1

request: "validar um CPF" - the same value is demanded to match and to be refused
  positives: ["123.456.789-09"]
  negatives: ["123.456.789-09"]
  explored: 64, rejected: 64
  outcome: ambiguous (InconsistentExamples)
  suggestions: (none)
  ranking:
    1. validate brazilian_cpf (Standard) score=1000 positives=1/1 negatives=0/1
    2. validate semver (Standard) score=1000 positives=1/1 negatives=0/1
    3. validate brazilian_cpf (Strict) score=1000 positives=1/1 negatives=0/1
    4. validate brazilian_cpf (Standard) score=1000 positives=1/1 negatives=0/1

request: "extrair o texto" - two families accept the positive sample
  positives: ["[42]"]
  negatives: ["abc"]
  explored: 24, rejected: 12
  outcome: ambiguous (ConflictingKinds)
  suggestions:
    extract between_delimiters (Greedy, Group)
    extract numbers (Greedy, Whole)
  ranking:
    1. extract between_delimiters (Greedy, Group) score=1100 positives=1/1 negatives=1/1
    2. extract numbers (Greedy, Whole) score=1100 positives=1/1 negatives=1/1
    3. extract between_delimiters (Lazy, Group) score=1100 positives=1/1 negatives=1/1
    4. extract between_delimiters (Exclusive, Group) score=1100 positives=1/1 negatives=1/1
```

Source: [`crates/regie-search/examples/refusal.rs`](examples/refusal.rs)
