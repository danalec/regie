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
request: "validate a CPF"
  -> confident: brazilian_cpf
       "123.456.789-09" -> true
       "nope" -> false

request: "check if this is a valid email address"
  -> confident: email
       "ana@corp.com" -> true
       "nope" -> false

request: "extract the text between brackets"
  -> confident: between_delimiters
       "the value is [42] here" -> true
       "nothing here" -> false

request: "extract the quoted text"
  -> confident: quoted
       "the value is [42] here" -> false
       "nothing here" -> false

request: "block messages containing the word spam"
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
request: "validate a CPF"
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

request: "extract the text between brackets"
  intent:   confident
  spec:     extract between_delimiters (boundary=Greedy, capture=Group)
  compiled: 1 capture group
  "[42] here" -> match, group 1 = "42"
  "nope" -> no match

request: "extract the quoted text"
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

request: "block messages containing the word spam"
  intent:   confident
  spec:     filter (action=Block, terms=["spam"])
  compiled: 0 capture groups
  "this is spam" -> match
  "this is fine" -> no match

request: "build a custom pattern of 6 alphanumeric characters"
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
request: "extract the text"
  reason:      ConflictingKinds
  suggestions: words, urls, ip_addresses
  caller asks:
    Which one did you mean?
      1) words
      2) urls
      3) ip_addresses

request: "date"
  reason:      ConflictingKinds
  suggestions: iso_date, dates, email
  caller asks:
    Which one did you mean?
      1) iso_date
      2) dates
      3) email

request: "validate something"
  reason:      LowConfidence
  suggestions: url, uri, brazilian_cpf
  caller asks:
    I am not sure. Which one did you mean?
      1) url
      2) uri
      3) brazilian_cpf

request: "help me"
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

## `hard_requests`

What it shows: the requests that sit on the edge of a family boundary, and
what the model does with them.  The model chooses one value out of a closed
enumeration, so a wrong guess yields a specification that is *valid* but
answers a different question than the request asked - "extract a date in ISO
format" is answered with a validation of an ISO date and not with an
extraction, so the date inside a sentence is refused.  A request the model
cannot settle on is not answered at all: it comes back as an ambiguity that
carries the candidates it could not choose between, and a request that is not
a request at all comes back with no candidate whatsoever.

Run it:

```console
$ cargo run -p regie-intent --example hard_requests
request: "validate a CNPJ"
  outcome:   confident
  spec:      validation brazilian_cnpj (strictness=Standard, charset=Digits, max_len=Some(18))
  value:          "12.345.678/0001-95" -> match
  one digit short:"12.345.678/0001-9" -> no match

request: "validate a CPF"
  outcome:   confident
  spec:      validation brazilian_cpf (strictness=Standard, charset=Digits, max_len=Some(14))
  value:          "123.456.789-09" -> match
  no check digits:"123.456.789" -> no match

request: "validate a NIF"
  outcome:   confident
  spec:      validation portuguese_nif (strictness=Standard, charset=Digits, max_len=Some(9))
  value:          "123456789" -> match
  punctuated:     "123.456.789" -> no match

request: "validate a brazilian phone number"
  outcome:   confident
  spec:      validation brazilian_phone (strictness=Standard, charset=Digits, max_len=Some(17))
  value:          "+55 11 91234-5678" -> match
  free text:      "not a phone" -> no match

request: "validate a brazilian postal code"
  outcome:   confident
  spec:      validation brazilian_postal_code (strictness=Standard, charset=Digits, max_len=Some(9))
  value:          "01310-100" -> match
  truncated:      "01310" -> no match

request: "extract a date in ISO format"
  outcome:   confident
  spec:      validation iso_date (strictness=Standard, charset=Digits, max_len=Some(10))
  value:          "2026-10-29" -> match
  in a sentence:  "the date is 2026-10-29" -> no match

request: "extract the quoted text"
  outcome:   confident
  spec:      extraction quoted (boundary=Greedy, capture=Group)
  value:          "say \"hi\"" -> match
  no quotes:      "no quotes here" -> no match

request: "extract the numbers"
  outcome:   confident
  spec:      extraction numbers (boundary=Greedy, capture=Whole)
  value:          "process 12345" -> match
  no digits:      "no digits here" -> no match

request: "block messages containing the word urgent"
  outcome:   confident
  spec:      filter (action=Block, terms=["urgent"])
  value:          "an urgent message" -> match
  no term:        "a quiet message" -> no match

request: "check if this is a valid email address"
  outcome:   confident
  spec:      validation email (strictness=Standard, charset=PrintableAscii, max_len=Some(254))
  value:          "ana@corp.com" -> match
  not an address: "not an email" -> no match

request: "validate a number"
  outcome:   ambiguous (LowConfidence)
  candidates:
    validation decimal (strictness=Standard, charset=Digits, max_len=Some(32))
    validation brazilian_phone (strictness=Standard, charset=Digits, max_len=Some(17))
    validation credit_card (strictness=Standard, charset=Digits, max_len=Some(19))

request: "asdkjhaskjdhaskjdh"
  outcome:   ambiguous (LowConfidence)
  candidates: (none)

```

Source: [`crates/regie-intent/examples/hard_requests.rs`](examples/hard_requests.rs)

## `languages`

What it shows: the languages the model understands.  The same request in
English and in Brazilian Portuguese reaches the same family, a request in a
language the model does not understand comes back as an ambiguity, and the one
exception - a request that carries nothing but the acronym of a family - is
answered in any language, because the acronym is the same everywhere and the
model keys on it rather than on the sentence around it.

The list of languages is written down in
[`docs/LANGUAGES.md`](../../docs/LANGUAGES.md).

Run it:

```console
$ cargo run -p regie-intent --example languages
the same request in the two supported languages:

a CPF:
  english     "validate a CPF"
  portuguese  "validar um CPF"
  english     -> confident brazilian_cpf
  portuguese  -> confident brazilian_cpf
  the same    -> yes

a CNPJ:
  english     "validate a CNPJ"
  portuguese  "validar um CNPJ"
  english     -> confident brazilian_cnpj
  portuguese  -> confident brazilian_cnpj
  the same    -> yes

a NIF:
  english     "validate a NIF"
  portuguese  "validar um NIF"
  english     -> confident portuguese_nif
  portuguese  -> confident portuguese_nif
  the same    -> yes

a Brazilian phone number:
  english     "validate a brazilian phone number"
  portuguese  "validar telefone brasileiro"
  english     -> confident brazilian_phone
  portuguese  -> confident brazilian_phone
  the same    -> yes

a Brazilian postal code:
  english     "validate a brazilian postal code"
  portuguese  "validar CEP"
  english     -> confident brazilian_postal_code
  portuguese  -> confident brazilian_postal_code
  the same    -> yes

an email address:
  english     "validate an email address"
  portuguese  "validar email"
  english     -> confident email
  portuguese  -> confident email
  the same    -> yes

a date:
  english     "validate a date"
  portuguese  "validar data"
  english     -> confident iso_date
  portuguese  -> confident iso_date
  the same    -> yes

the quoted text:
  english     "extract the quoted text"
  portuguese  "extrair as palavras entre aspas"
  english     -> confident quoted
  portuguese  -> confident quoted
  the same    -> yes

the numbers:
  english     "extract the numbers"
  portuguese  "extrair o numero do processo"
  english     -> confident numbers
  portuguese  -> confident numbers
  the same    -> yes

the text between brackets:
  english     "extract the text between brackets"
  portuguese  "extrair os numeros entre colchetes"
  english     -> confident between_delimiters
  portuguese  -> confident between_delimiters
  the same    -> yes

a word to block:
  english     "block messages containing the word spam"
  portuguese  "bloquear a palavra spam"
  english     -> confident filter
  portuguese  -> confident filter
  the same    -> yes

a custom shape:
  english     "build a custom pattern of 6 alphanumeric characters"
  portuguese  "criar um padrão de 6 caracteres alfanuméricos"
  english     -> confident custom
  portuguese  -> confident custom
  the same    -> yes

a request in a language the model does not understand:

  "validar un correo electrónico"
    -> ambiguous LowConfidence [["uri", "brazilian_postal_code", "brazilian_license_plate"]]
  "valider une adresse e-mail"
    -> ambiguous ConflictingKinds [["email", "emails", "dates"]]
  "eine E-Mail-Adresse validieren"
    -> ambiguous ConflictingKinds [["email", "emails", "dates"]]
  "validare un indirizzo email"
    -> ambiguous LowConfidence [["email", "emails", "numbers"]]

the exception: an acronym is not a language

  "cpf valideren"
    -> confident brazilian_cpf
  "cpf を検証する"
    -> confident brazilian_cpf
  "验证 CPF"
    -> confident brazilian_cpf
```

Source: [`crates/regie-intent/examples/languages.rs`](examples/languages.rs)
