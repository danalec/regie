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
request: "validate a CPF"
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

request: "validate a CPF"
  positives: ["this is not a cpf"]
  negatives: ["123.456.789-09"]
  explored: 64, rejected: 64
  outcome: ambiguous (InconsistentExamples)
  rank 1: iso_date_time score=100 positives=0/1 negatives_rejected=1/1
  rank 2: iso_date score=100 positives=0/1 negatives_rejected=1/1
  rank 3: currency_amount score=100 positives=0/1 negatives_rejected=1/1
  rank 4: brazilian_cpf score=100 positives=0/1 negatives_rejected=1/1
```

Source: [`crates/regie-search/examples/search.rs`](examples/search.rs)

## `discriminate`

What it shows: the same request with two different example sets, producing
two different specifications that both fit the examples they were given.

Run it:

```console
$ cargo run -p regie-search --example discriminate
request: "extract the bracketed text" - bracketed
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

request: "extract the bracketed text" - a bare number
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
request: "validate a CPF" - no candidate accepts the positive sample
  positives: ["zzz"]
  negatives: ["123.456.789-09"]
  explored: 64, rejected: 64
  outcome: ambiguous (InconsistentExamples)
  suggestions: (none)
  ranking:
    1. validate iso_date_time (Standard) score=100 positives=0/1 negatives=1/1
    2. validate iso_date (Standard) score=100 positives=0/1 negatives=1/1
    3. validate currency_amount (Standard) score=100 positives=0/1 negatives=1/1
    4. validate brazilian_cpf (Lenient) score=100 positives=0/1 negatives=1/1

request: "validate a CPF" - the same value is demanded to match and to be refused
  positives: ["123.456.789-09"]
  negatives: ["123.456.789-09"]
  explored: 64, rejected: 64
  outcome: ambiguous (InconsistentExamples)
  suggestions: (none)
  ranking:
    1. validate brazilian_cpf (Standard) score=1000 positives=1/1 negatives=0/1
    2. validate brazilian_cpf (Strict) score=1000 positives=1/1 negatives=0/1
    3. validate brazilian_cpf (Standard) score=1000 positives=1/1 negatives=0/1
    4. validate brazilian_cpf (Standard) score=1000 positives=1/1 negatives=0/1

request: "extract the bracketed text" - two families accept the positive sample
  positives: ["[42]"]
  negatives: ["abc"]
  explored: 24, rejected: 12
  outcome: ambiguous (ConflictingKinds)
  suggestions:
    extract numbers (Greedy, Whole)
    extract between_delimiters (Greedy, Group)
  ranking:
    1. extract numbers (Greedy, Whole) score=1100 positives=1/1 negatives=1/1
    2. extract between_delimiters (Greedy, Group) score=1100 positives=1/1 negatives=1/1
    3. extract numbers (Lazy, Whole) score=1100 positives=1/1 negatives=1/1
    4. extract numbers (Exclusive, Whole) score=1100 positives=1/1 negatives=1/1
```

Source: [`crates/regie-search/examples/refusal.rs`](examples/refusal.rs)

## `hard_discrimination`

What it shows: example sets that separate families that look alike, and the
proof that the **examples** decide, not the model.  A request like "validate
a CPF" or "extract the bracketed text" leaves several families open, and the
examples are what tell them apart: only a candidate that matches every
positive example and refuses every negative one survives.  The same request
with the examples the other way round gives a different answer - and when no
candidate fits the examples, the search refuses to settle and says why, rather
than guessing.  The last case is a contradiction: one CPF is demanded to
match and another CPF is demanded to be refused, so the family that accepts
the first accepts the second as well.

Run it:

```console
$ cargo run -p regie-search --example hard_discrimination
request: "validate a CPF" - the CPF family against the ones that look like it
  positives: ["123.456.789-09", "987.654.321-00"]
  negatives: ["12.345.678/0001-95", "+55 11 91234-5678", "12345"]
  explored: 64, rejected: 52
  outcome: confident validate brazilian_cpf (Standard)
  ranking:
    1. validate brazilian_cpf (Standard) score=2300 positives=2/2 negatives=3/3
    2. validate brazilian_cpf (Strict) score=2300 positives=2/2 negatives=3/3
    3. validate brazilian_cpf (Standard) score=2300 positives=2/2 negatives=3/3
    4. validate brazilian_cpf (Standard) score=2300 positives=2/2 negatives=3/3
  proof:
    the winner is validate brazilian_cpf (Standard)
    "123.456.789-09" -> match
    "987.654.321-00" -> match
    "12.345.678/0001-95" -> no match
    "+55 11 91234-5678" -> no match
    "12345" -> no match

request: "validate a date" - the ISO shape of a date
  positives: ["2026-10-29", "1999-01-31"]
  negatives: ["29/10/2026", "31/01/1999"]
  explored: 64, rejected: 46
  outcome: confident validate iso_date (Standard)
  ranking:
    1. validate iso_date (Standard) score=2200 positives=2/2 negatives=2/2
    2. validate iso_date (Strict) score=2200 positives=2/2 negatives=2/2
    3. validate iso_date (Lenient) score=2200 positives=2/2 negatives=2/2
    4. validate iso_date (Standard) score=2200 positives=2/2 negatives=2/2
  proof:
    the winner is validate iso_date (Standard)
    "2026-10-29" -> match
    "1999-01-31" -> match
    "29/10/2026" -> no match
    "31/01/1999" -> no match

request: "validate a date" - the Brazilian shape of a date, which no family accepts
  positives: ["29/10/2026", "31/01/1999"]
  negatives: ["2026-10-29", "1999-01-31"]
  explored: 64, rejected: 64
  outcome: ambiguous (InconsistentExamples)
  suggestions: (none)
  ranking:
    1. validate iso_date_time (Standard) score=200 positives=0/2 negatives=2/2
    2. validate currency_amount (Standard) score=200 positives=0/2 negatives=2/2
    3. validate semver (Standard) score=200 positives=0/2 negatives=2/2
    4. validate iso_date_time (Strict) score=200 positives=0/2 negatives=2/2
  proof:
    no winner, the best candidate is validate iso_date_time (Standard)
    "29/10/2026" -> no match
    "31/01/1999" -> no match
    "2026-10-29" -> no match
    "1999-01-31" -> no match

request: "extract the bracketed text" - a quoted value against a bracketed one
  positives: ["\"value\"", "\"outro\""]
  negatives: ["[value]", "[outro]"]
  explored: 24, rejected: 18
  outcome: confident extract quoted (Greedy, Group)
  ranking:
    1. extract quoted (Greedy, Group) score=2200 positives=2/2 negatives=2/2
    2. extract quoted (Lazy, Group) score=2200 positives=2/2 negatives=2/2
    3. extract quoted (Exclusive, Group) score=2200 positives=2/2 negatives=2/2
    4. extract quoted (Greedy, Whole) score=2200 positives=2/2 negatives=2/2
  proof:
    the winner is extract quoted (Greedy, Group)
    "\"value\"" -> match
    "\"outro\"" -> match
    "[value]" -> no match
    "[outro]" -> no match

request: "validate a CPF" - two CPFs, one demanded to match and the other to be refused
  positives: ["123.456.789-09"]
  negatives: ["987.654.321-00"]
  explored: 64, rejected: 64
  outcome: ambiguous (InconsistentExamples)
  suggestions: (none)
  ranking:
    1. validate brazilian_cpf (Standard) score=1000 positives=1/1 negatives=0/1
    2. validate brazilian_cpf (Strict) score=1000 positives=1/1 negatives=0/1
    3. validate brazilian_cpf (Standard) score=1000 positives=1/1 negatives=0/1
    4. validate brazilian_cpf (Standard) score=1000 positives=1/1 negatives=0/1
  proof:
    no winner, the best candidate is validate brazilian_cpf (Standard)
    "123.456.789-09" -> match
    "987.654.321-00" -> match
```

Source: [`crates/regie-search/examples/hard_discrimination.rs`](examples/hard_discrimination.rs)
