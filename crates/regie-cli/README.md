# regie-cli

The command line front-end of [`regie`](../../README.md).

```console
cargo install --path crates/regie-cli
```

The tool is a thin shell over the workspace: it compiles patterns, asks the
intent model what a request means, runs the verified search, and writes and
reads the artifact format of [`regie-codec`](../regie-codec).

## The commands

| command | what it does |
|---|---|
| `match` | compiles a pattern and reports the match and its groups |
| `test` | compiles a pattern and reports whether it matches |
| `intent` | turns a request into a specification |
| `search` | narrows a request down with examples |
| `show` | interprets a request and shows what it matches |
| `serve` | serves the live view on the loopback interface |
| `dfa` | compiles a pattern into an artifact |
| `decode` | matches against an artifact |

```console
$ regie match '(\w+)@(\w+\.\w+)' 'mail: ana@corp.com'
match: 6..18 "ana@corp.com"
group 1: 6..9 "ana"
group 2: 10..18 "corp.com"

$ regie test '^\d{3}-\d{4}$' '555-1234'
match

$ regie intent 'validar CPF'
summary: confident validate brazilian_cpf strictness=standard charset=digits max_len=14
{"candidate":{"charset":"digits","kind":"brazilian_cpf","max_len":14,"strictness":"standard","type":"validate"},"status":"confident"}

$ regie search 'validar CPF' --positive '123.456.789-09' --negative '123.456.789'
search: confident
candidate: validate brazilian_cpf strictness=standard charset=digits max_len=14
rank 1: validate brazilian_cpf score=1100 positives=1/1 negatives_rejected=1/1
rank 2: validate brazilian_cpf score=1100 positives=1/1 negatives_rejected=1/1
rank 3: validate brazilian_cpf score=1100 positives=1/1 negatives_rejected=1/1
rank 4: validate brazilian_cpf score=1100 positives=1/1 negatives_rejected=1/1
explored: 64 candidates, 52 rejected

$ regie dfa '^ana@corp\.com$' ana.rdf
wrote 558 bytes to ana.rdf (14 states, 10 byte classes)

$ regie decode ana.rdf 'ana@corp.com'
artifact: 558 bytes, 14 states, 10 byte classes
match
```

Every command that prints a specification prints it twice: once in a human
form and once as JSON. The JSON is a single line, and its shape is stable.

## The live view

`show` is the subcommand that answers the question the other ones leave open:
what does this request actually catch? It interprets the request, compiles the
specification and prints the haystack with the part that matched underlined:

```console
$ regie show 'extrair o email' --input 'mail ana@corp.com now'
request: extrair o email
intent: confident extract emails boundary=greedy capture=whole
haystack: mail ana@corp.com now
     ^^^^^^^^^^^^ matched 5..17
groups:
  0: 5..17 "ana@corp.com"
  1: 5..17 "ana@corp.com"
```

A validation is anchored, so a value inside a sentence is not a match. The
subcommand says so, instead of leaving a bare "no match" behind:

```console
$ regie show 'validar CPF' --input 'the cpf of ana is 123.456.789-09'
request: validar CPF
intent: confident validate brazilian_cpf strictness=standard charset=digits max_len=14
haystack: the cpf of ana is 123.456.789-09
no match
note: a validation is anchored - the whole haystack has to be the value
```

`--watch <file>` prints the view again every time the file changes, which is
what a log file or a template needs. `serve` serves the same view as a page
that follows the typing:

```text
$ regie serve --port 8080
regie is serving the live view on http://127.0.0.1:8080
press ctrl-c to stop it
```

## The exit codes

| code | meaning |
|---|---|
| `0` | the command did what was asked |
| `1` | the command found no match, or could not settle on an answer |
| `2` | a usage error, an I/O error, or an input the engine rejected |

The third code is the important one: a malformed pattern or a corrupt
artifact is an error the caller can tell apart from "no match".

## The output formats

The `intent` and `search` commands print a stable, flat projection of the
specification: the family is the `type` field, every enumeration is a
snake-case name, and every optional field is `null` when it is absent.

The raw `serde` form of the specification - the externally tagged one the
dataset uses - is not what the tool prints. The flat form is easier to
read and easier to script against, and it is covered by the tests.

## More examples

The gallery of [`EXAMPLES.md`](EXAMPLES.md) documents every subcommand, with
the output it prints and the exit code it produced.

Compiling a pattern into an artifact and matching an input against it:

```console
$ regie dfa '^ana@corp\.com$' ana.rdf
wrote 558 bytes to ana.rdf (14 states, 10 byte classes)

$ regie decode ana.rdf 'ana@corp.com'
artifact: 558 bytes, 14 states, 10 byte classes
match
```
