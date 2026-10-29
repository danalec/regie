# regie examples

`regie` is the command line front-end of the workspace. It exposes the four
layers of the project behind one binary: `match` and `test` run a pattern
against an input, `intent` turns a natural-language request into a
structured specification, `search` verifies a request against examples, and
`dfa` and `decode` compile a pattern into a `regie-codec` artifact and match
an input against it.

Run the tool from the workspace with
`cargo run -p regie-cli --bin regie -- <subcommand> <args>`.

The exit code carries the verdict, and every example below says which one it
produced:

| code | meaning |
|---|---|
| `0` | the command did what was asked |
| `1` | no match, or no answer the tool could settle on |
| `2` | usage error, I/O error, or an input the engine rejected |

## `match`

What it does: compiles a pattern and reports the span of the leftmost match
and the span of every capture group.

The spans are byte offsets into the input. With a match the exit code is `0`:

```console
$ cargo run -p regie-cli --bin regie -- match '(\w+)@(\w+\.\w+)' 'ana@corp.com'
match: 0..12 "ana@corp.com"
group 1: 0..3 "ana"
group 2: 4..12 "corp.com"
```

When the pattern does not match, the tool prints `no match` and exits with
code `1`:

```console
$ cargo run -p regie-cli --bin regie -- match '(\w+)@(\w+\.\w+)' 'nobody'
no match
```

## `test`

What it does: compiles a pattern and reports only whether it matches, which
makes it a predicate for a shell pipeline.

A match exits with code `0`:

```console
$ cargo run -p regie-cli --bin regie -- test '^\d{3}-\d{4}$' '555-1234'
match
```

No match exits with code `1`:

```console
$ cargo run -p regie-cli --bin regie -- test '^\d{3}-\d{4}$' '5551234'
no match
```

## `intent`

What it does: interprets a natural-language request with the embedded
micro-model of intent and prints the specification it settled on.

A request the model understands exits with code `0` and prints one line of
prose followed by the specification as JSON:

```console
$ cargo run -p regie-cli --bin regie -- intent 'validar CPF'
summary: confident validate brazilian_cpf strictness=standard charset=digits max_len=14
{"candidate":{"charset":"digits","kind":"brazilian_cpf","max_len":14,"strictness":"standard","type":"validate"},"status":"confident"}
```

A request that stays ambiguous exits with code `1` and prints the reason and
the suggestions it could not choose between:

```console
$ cargo run -p regie-cli --bin regie -- intent 'validar numero'
summary: ambiguous conflicting_kinds (3 suggestion(s))
{"reason":"conflicting_kinds","status":"ambiguous","suggestions":[{"charset":"digits","kind":"integer","max_len":20,"strictness":"standard","type":"validate"},{"charset":"digits","kind":"portuguese_nif","max_len":9,"strictness":"standard","type":"validate"},{"charset":"digits","kind":"brazilian_phone","max_len":17,"strictness":"standard","type":"validate"}]}
```

## `search`

What it does: verifies a request against examples. Every candidate the
micro-model proposes is checked against the examples, and only a candidate
that accepts every positive example and rejects every negative one can become
the outcome.

The exit code is `0` when exactly one candidate survives:

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

The last line counts the candidates the search looked at and the ones it
rejected; the counters on each rank line say how many positive examples the
candidate accepted and how many negative examples it rejected.

## `dfa`

What it does: compiles a pattern into a `regie-codec` artifact and writes it
to a file. The exit code is `0` when the artifact was written.

The file name is the second argument, so the tool writes it into the current
directory under the name given here:

```console
$ cargo run -p regie-cli --bin regie -- dfa '^ana@corp\.com$' ana.rdf
wrote 558 bytes to ana.rdf (14 states, 10 byte classes)
```

## `decode`

What it does: reads an artifact back and matches an input against the
decoded automaton. The artifact carries no capture groups, so only the verdict
is reported.

A match exits with code `0`:

```console
$ cargo run -p regie-cli --bin regie -- decode ana.rdf 'ana@corp.com'
artifact: 558 bytes, 14 states, 10 byte classes
match
```

An input the automaton does not match exits with code `1`:

```console
$ cargo run -p regie-cli --bin regie -- decode ana.rdf 'ana@other.com'
artifact: 558 bytes, 14 states, 10 byte classes
no match
```

## Scripting with regie

The exit code is the whole interface: `0` for a match, `1` for no match, and
`2` for a pattern the engine rejected. A script can branch on it. The
snippets below assume the `regie` binary is on the path, which
`cargo install --path crates/regie-cli` takes care of.

A `grep`-style filter over a file — keep the lines that carry a corporate
address:

```bash
while read -r line; do
  regie test '^[\w.]+@corp\.com$' "$line" > /dev/null && printf '%s\n' "$line"
done < users.txt
```

A loop that classifies each line of a log:

```bash
while read -r line; do
  if regie test '^\d{4}-\d{2}-\d{2} ' "$line" > /dev/null; then
    echo "dated:  $line"
  elif regie test 'ERROR' "$line" > /dev/null; then
    echo "failed: $line"
  else
    echo "other:  $line"
  fi
done < app.log
```

A check that refuses to continue when the engine rejects the pattern, rather
than treating the rejection as a plain mismatch:

```bash
if regie test "$PATTERN" "$SAMPLE" > /dev/null; then
  echo "the pattern matched"
else
  status=$?
  if [ "$status" -eq 2 ]; then
    echo "the engine rejected the pattern" >&2
    exit 1
  fi
  echo "the pattern did not match"
fi
```
