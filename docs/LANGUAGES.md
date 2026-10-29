# The languages the model understands

The intent layer turns a sentence into a specification.  Which sentences it
understands is a decision of the project, not an accident of the corpus, so it
is written down here.

## The list

| language | supported | phrasings in the corpus |
|---|---|---|
| English | **yes** | 8 per validation family, 10 per extraction family, 3 to 7 per filter family, 12 for the custom shape |
| Brazilian Portuguese | **yes** | 8 per validation family, 10 per extraction family, 2 per filter family, 18 for the custom shape |
| every other language | **no** | none |

The corpus of [`datasets/intent_train.jsonl`](../datasets/intent_train.jsonl)
carries both of the first two, so every one of the 43 families of the
specification has phrasings in the two languages.  The numbers in the table
are measured, not estimated: `python tools/measure_languages.py` renders the
corpus the way the training script does and counts the phrasings of each
family in each language.

The Portuguese half of the corpus is Brazilian: it says `caractere` and
`alfanuméricos`, not the `caracteres alfanuméricos` of Lisbon.

## What "supported" means

A language is supported when the model answers a request written in it with
the family the request names.  It is not supported when the request comes back
as an ambiguity, which is the honest answer: the model did not understand it,
and it says so instead of guessing.

```text
"validate a CPF"                 -> confident  brazilian_cpf
"validar um CPF"                 -> confident  brazilian_cpf
"validar un correo electrónico"   -> ambiguous  (not understood)
"valider une adresse e-mail"      -> ambiguous  (not understood)
"eine E-Mail-Adresse validieren"  -> ambiguous  (not understood)
```

The same request in the two supported languages reaches the same family:

| the request | in English | in Brazilian Portuguese |
|---|---|---|
| a CPF | `validate a CPF` | `validar um CPF` |
| a CNPJ | `validate a CNPJ` | `validar um CNPJ` |
| a NIF | `validate a NIF` | `validar um NIF` |
| a Brazilian phone number | `validate a brazilian phone number` | `validar telefone brasileiro` |
| a Brazilian postal code | `validate a brazilian postal code` | `validar CEP` |
| an email address | `validate an email address` | `validar email` |
| a date | `validate a date` | `validar data` |
| the quoted text | `extract the quoted text` | `extrair as palavras entre aspas` |
| the numbers | `extract the numbers` | `extrair o numero do processo` |
| the text between brackets | `extract the text between brackets` | `extrair os numeros entre colchetes` |
| a word to block | `block messages containing the word spam` | `bloquear a palavra spam` |
| a custom shape | `build a custom pattern of 6 alphanumeric characters` | `criar um padrão de 6 caracteres alfanuméricos` |

## The exception: an acronym is not a language

A request that carries nothing but the acronym of a family is answered
confidently in **any** language, because the acronym is the same everywhere
and the model keys on it rather than on the sentence around it:

```text
"cpf valideren"      -> confident  brazilian_cpf
"cpf を検証する"       -> confident  brazilian_cpf
"验证 CPF"             -> confident  brazilian_cpf
```

This is not support for Dutch, Japanese or Chinese.  It is the model
recognising the string `CPF`.  A request in an unsupported language that
carries an acronym is answered, and the answer may be right; a request
without one is refused.  The behaviour is pinned by a test so it cannot
change silently.

## Where the list is enforced

The list is a claim that is checked, not a paragraph that is believed:

* [`crates/regie-intent/tests/languages.rs`](../crates/regie-intent/tests/languages.rs)
  runs the same intent in both supported languages and checks that the two
  reach the same family, and checks that requests in unsupported languages
  come back ambiguous.
* [`crates/regie-intent/examples/languages.rs`](../crates/regie-intent/examples/languages.rs)
  is the runnable version of the table above.

## The examples of the project are in English

The documentation of the project is written in English, so the examples are
too, and every request in them is an English one.  The Brazilian Portuguese
half of the model is shown in one place, the example above, and nowhere else
by accident.
