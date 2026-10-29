# Examples of `regie-intent`

`regie-intent` holds a small, local *micro-model of intent*: it turns a
request written in words into a structured specification, and it refuses to
guess when the request is not clear enough.  The model never emits pattern
text - it chooses one of the 43 enumerated families, and the deterministic
compiler of `regie-core` turns that specification into an automaton.

Run any example from the root of the workspace:

```console
$ cargo run -p regie-intent --example <name>
```

Every run is deterministic: the same request always produces the same bytes,
on every platform.  There is no randomness, no clock and no hash iteration
order in the inference path.

Two honest caveats:

* The corpus the model was fitted on is generated from templates
  ([`datasets/README.md`](../../datasets/README.md)), so the model is strong
  on the phrasings the corpus contains and weaker on the ones it does not.
  The `ambiguity` example shows both.
* A validation family checks the *shape* of a value, not its checksum:
  `123.456.789-00` is a well-formed CPF and is accepted.

## `interpret`

What it shows: what the model answers for a request - a specification, or an
ambiguity with the candidates it considered.

Run it:

```console
$ cargo run -p regie-intent --example interpret
request: "validar um CPF"
  -> confident: brazilian_cpf
       "123.456.789-09" -> true
       "nope" -> false

request: "check if this is a valid email address"
  -> confident: email
       "ana@corp.com" -> true
       "nope" -> false

request: "extrair os numeros entre colchetes"
  -> confident: between_delimiters
       "the value is [42] here" -> true
       "nothing here" -> false

request: "extrair as palavras entre aspas"
  -> confident: quoted
       "the value is [42] here" -> false
       "nothing here" -> false

request: "bloquear a palavra spam"
  -> confident: filter
       "this is spam" -> true
       "this is fine" -> false

request: "extract the url from the text"
  -> confident: urls
       "the value is [42] here" -> false
       "nothing here" -> false

request: "asdkjhaskjdhaskjdh"
  -> ambiguous: LowConfidence
```

Source: [`crates/regie-intent/examples/interpret.rs`](examples/interpret.rs)

## `spec_to_regex`

What it shows: the whole path from a request to a running automaton - the
specification the model picks, the automaton the compiler builds from it and
what that automaton does with a sample.

Run it:

```console
$ cargo run -p regie-intent --example spec_to_regex
request: "validar um CPF"
  intent:   confident
  spec:     validate brazilian_cpf (strictness=Standard, charset=Digits, max_len=Some(14))
  compiled: 0 capture groups
  "123.456.789-09" -> match
  "not a cpf" -> no match

request: "check if this is a valid email address"
  intent:   confident
  spec:     validate email (strictness=Standard, charset=PrintableAscii, max_len=Some(254))
  compiled: 0 capture groups
  "a@b.co" -> match
  "nope" -> no match

request: "extrair os numeros entre colchetes"
  intent:   confident
  spec:     extract between_delimiters (boundary=Greedy, capture=Group)
  compiled: 1 capture group
  "[42] here" -> match, group 1 = "42"
  "nope" -> no match

request: "extrair as palavras entre aspas"
  intent:   confident
  spec:     extract quoted (boundary=Greedy, capture=Group)
  compiled: 2 capture groups
  "say \"hi\"" -> match, group 1 = "hi"
  "no quotes" -> no match

request: "extract the url from the text"
  intent:   confident
  spec:     extract urls (boundary=Greedy, capture=Whole)
  compiled: 1 capture group
  "see https://x.io" -> match, group 1 = "https://x.io"
  "no url" -> no match

request: "bloquear a palavra spam"
  intent:   confident
  spec:     filter (action=Block, terms=["spam"])
  compiled: 0 capture groups
  "this is spam" -> match
  "this is fine" -> no match

request: "criar um padrão de 6 alfanuméricos"
  intent:   confident
  spec:     custom (template=Repeat { class: Alphanumeric, count: 6 }, min_len=Some(6), max_len=Some(6))
  compiled: 0 capture groups
  "abc123" -> match
  "abc12" -> no match
```

Source: [`crates/regie-intent/examples/spec_to_regex.rs`](examples/spec_to_regex.rs)

## `ambiguity`

What it shows: what the model does when it does not settle - the reason it
refused, the candidates it offers and the question a caller asks the user.

Run it:

```console
$ cargo run -p regie-intent --example ambiguity
request: "extrair o texto"
  reason:      ConflictingKinds
  suggestions: between_delimiters, quoted, ip_addresses
  caller asks:
    Which one did you mean?
      1) between_delimiters
      2) quoted
      3) ip_addresses

request: "data"
  reason:      ConflictingKinds
  suggestions: iso_date, dates, iso_date_time
  caller asks:
    Which one did you mean?
      1) iso_date
      2) dates
      3) iso_date_time

request: "validar algo"
  reason:      LowConfidence
  suggestions: url, uri, brazilian_cpf
  caller asks:
    I am not sure. Which one did you mean?
      1) url
      2) uri
      3) brazilian_cpf

request: "me ajuda"
  reason:      LowConfidence
  suggestions: (none)
  caller asks:
    I could not turn this request into a specification.
    Could you rephrase it?

request: "qwertyuiop zzzz"
  reason:      LowConfidence
  suggestions: (none)
  caller asks:
    I could not turn this request into a specification.
    Could you rephrase it?
```

Source: [`crates/regie-intent/examples/ambiguity.rs`](examples/ambiguity.rs)

## `model_profile`

What it shows: the numbers of the model itself - the classes, the features
and the bytes it occupies in the binary.

Run it:

```console
$ cargo run -p regie-intent --example model_profile
model: regie-intent, embedded and quantized
  intent classes:   43
    validate:       31
    extract:        10
    filter:         1
    custom:         1
  lexical features: 1419
    words:          388
    bigrams:        1018
    shape buckets:  13
  weight table:     61189 bytes
    i8 weights:     61017 bytes
    i32 biases:     172 bytes
  class names:      43 entries, 712 bytes
  vocabulary:       1419 entries, 13828 bytes
  fat pointers:     23392 bytes (16 bytes per &str)
  linked footprint: 99121 bytes
  budget:           262144 bytes
  a class wins when the score reaches 12 and the margin reaches 52
```

Source: [`crates/regie-intent/examples/model_profile.rs`](examples/model_profile.rs)
