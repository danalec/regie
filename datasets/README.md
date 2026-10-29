# Intent dataset

`intent_train.jsonl` is the synthetic corpus the intent model is trained on
and evaluated against.  It is the only hand-curated input of the intent
layer: `tools/train_intent.py` reads it and writes the quantized weights to
`crates/regie-intent/src/weights.rs`.

The file holds one JSON object per line, UTF-8, LF line endings.

## Format

```json
{"request": "validar um cpf", "spec": {"Validate": {"kind": "BrazilianCpf", "strictness": "Standard", "charset": "Digits", "max_len": 14}}, "positives": ["123.456.789-09"], "negatives": ["123.456.789-0"]}
```

| field | meaning |
|---|---|
| `request` | the natural-language request, as a user would write it |
| `spec` | the specification the request must be classified as |
| `positives` | sample inputs the specification accepts |
| `negatives` | sample inputs the specification rejects |

The `spec` is a tagged object with exactly one key, the family of the intent:

* `Validate`: `kind` (a `ValidationKind`), `strictness` (a `Strictness`),
  `charset` (a `CharsetPolicy`) and `max_len` (a number or `null`).
* `Extract`: `kind` (an `ExtractionKind`), `boundary` (a `BoundaryPolicy`)
  and `capture` (a `CapturePolicy`).
* `Filter`: `action` (a `FilterAction`) and `terms`, a list of
  `{"kind": ..., "bytes": ...}` objects, where `kind` is a `TermKind` and
  `bytes` is the term as a UTF-8 string.
* `Custom`: `template`, either `{"Repeat": {"class": ..., "count": ...}}` or
  `{"LiteralThenClass": {"literal": ..., "class": ..., "count": ...}}`, and
  `params`, the `PatternParams` of the pattern.

The `positives` and `negatives` are not used by the trainer; they document
what the specification means and are meant for the search layer, which
verifies a candidate against them.

## Coverage

The dataset holds 744 requests over 43 classes:

* 31 validation families, 16 requests each;
* 10 extraction families, 20 requests each;
* the filter family, 18 requests;
* the custom family, 30 requests.

Every request of a family is rendered from the same set of templates, with
the words that describe the value rotated underneath.  That is deliberate:
the template words then carry no information about the class, so the model
has to learn the words that actually describe the value instead of latching
onto a favourite verb.  The requests mix Portuguese and English, and some
of them carry accents on purpose, so that the accent folding is exercised.

The extraction families carry two extra phrasings each, built around the
singular noun ("extract the url from the text").  The tokenizer does not
stem, so "the url" and "the urls" are two different features, and asking
for a single value is the common case.

## Regenerating

The file is regenerated from the corpus table in `tools/train_intent.py`:

```
python tools/train_intent.py --emit-dataset
```

The generator refuses to write a file whose expectations disagree with the
rule engine that derives the non-classified parts of a specification, so the
dataset and the runtime always agree.
