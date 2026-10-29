# What comes next

This document says what the project is going to do, what it is not going to
do, and why. It is a plan, not a promise: the dates are not there because
the work is done when the work is done.

## The shape of the project

`regie` is a regex engine for embedded targets with an intent layer on top.
The engine has to be predictable, allocation-free and small; the intent
layer has to be local, deterministic and small. Everything in this plan
follows from those two constraints.

The plan is in three parts: what is missing from the engine, what is
missing from the intent layer, and what the project refuses to do.

## The engine

### A smaller automaton

The DFA is currently a fixed table of 256 states by 32 classes of `u16`,
which is 16 KiB. Most real patterns need far less. The next step is to
build the transition table in a compressed form - the usual techniques are
a row-offset index and a run of shared rows - and to let the size of the
artifact follow the pattern instead of following the limits.

This is the single most valuable change for the firmware path: an artifact
that is 16 KiB today would become a few hundred bytes for a typical
pattern.

### Ahead-of-time compilation

The `RGDF` artifact is already the compiled form of a pattern, but it is
produced by a separate tool and linked as a blob. The next step is to
compile the artifacts as part of the build, so that a firmware project
writes:

```toml
[build-dependencies]
regie-codec = "0.1"
```

and gets a `pattern.rdf` in `OUT_DIR` that is regenerated when the pattern
changes. The blob stops being something a human copies around.

### Captures that survive the automaton

The DFA can answer "does it match" and "where does it match", but it cannot
report the groups: a deterministic automaton has no notion of a capture.
The NFA engine can, and it is the one used when the groups are needed.

The plan is to make that choice explicit and cheap: the DFA answers the
cheap question, and the engine falls back to the NFA only when the caller
asks for the groups. The API already separates `is_match` from `captures`;
what is missing is the automatic choice, and a measurement of when each
one wins.

### A real Unicode mode

The engine works on bytes, which is the right choice for a firmware
parser: it never has to decode, and it never has to allocate for the
decoded text. But it also means that `.` matches half of a character, and
that `\w` is ASCII only.

The plan is a `unicode` feature that works on *code points* without
changing the byte-oriented core: the pattern is compiled to a byte
automaton, and the classes of the Unicode mode are pre-computed into byte
ranges that the automaton understands. The engine stays byte-oriented; the
classes get wider. A UTF-8 validity check would be a separate assertion
rather than a mode.

### A library of the validation families

The 31 validation families are the most valuable part of the project, and
they are also the part that will be asked for more often. The plan is to
keep adding them, each one with the same treatment: a shape that a regular
expression can express, a test that names what it accepts and what it
refuses, and an entry in the dataset.

The families that a regular expression *cannot* express - the checksums of
a CPF, a CNPJ or a credit card - are deliberately not there as families.
They belong to the search layer, which can check a value with a program.

### The dialect

Two additions are worth considering:

* **Named groups.** They are refused now because they change the numbering
  of the groups and the engine has eight slots. With named groups the
  numbering would be explicit and the limit could go.
* **A `find_iter` that does not allocate.** The current API finds one match
  at a time, which is enough for the firmware path. An iterator would need
  a small state machine, which is feasible without an allocator.

What will not be added: look-around, backreferences and recursion. The
reason is in [ARCHITECTURE.md](ARCHITECTURE.md) and it will not change.

## The intent layer

### A larger model

The model is 1,419 features over 43 classes and it reaches 100% accuracy
on the held-out split of its own dataset. That number is not impressive:
the dataset is generated from templates, and the model is a naive Bayes
over the words of the request.

The honest next step is a dataset that is not generated from templates:
requests written by people, with the mistakes they make. The tooling
already supports it - the trainer reads a JSON Lines file - and the
consistency test already checks that every record of the dataset agrees
with the engine, so a dataset of real requests would be checked the same
way.

Until that dataset exists, the accuracy of the model says very little, and
the documentation should keep saying so.

### A model that knows the language

The tokenizer folds accents and lower-cases the text, which is enough for
Portuguese and English. A model that knows the language would segment
"cnpj" as one word rather than three, and would tell a request in Spanish
from one in Portuguese.

The constraint is the size: the model has to stay under 256 KiB, and a
segmenter is a dictionary. The plan is a small, embedded dictionary of the
words the families use - a few hundred entries - rather than a general
model.

### More families

The intent layer covers 43 classes. The obvious next ones are the ones the
search layer already knows how to mutate: postal addresses, phone numbers
in more countries, currency amounts, MAC addresses in the EU format,
IPv6 in its compressed form, and the license plates of more countries.

### The search, with more axes

The search mutates a candidate along the axes of its family - strictness,
charset, boundary, capture, action, template, anchoring. Two axes are
missing:

* **The maximum length.** Every family has one, and the corpus of the
  caller often implies it.
* **The character set of a custom pattern.** A request like "six digits" is
  a template, and "six characters that are not digits" is the same template
  with a different class.

Both are cheap to add and both are the kind of thing that turns an
ambiguous request into a confident one.

## The project

### A crate per use case

The workspace has five crates, and the split follows the dependency graph
rather than the use cases. A firmware project that only wants the engine
pulls in `regie-core` and nothing else, which is right. But a project that
wants the artifact format and the intent layer has to take the whole
chain, and the intent layer is 97 KiB it may not need.

The plan is to keep the crates as they are, and to be careful about what
the `alloc` and `std` features pull in, so that a project can take
`regie-core` with `serde` without taking the model.

### A benchmark that stays honest

The benchmark suite compares against `regex-automata`, which is the right
reference. The plan is to keep the comparison in the suite rather than in a
document, so that a regression is visible in the pull request and not a
year later.

The numbers in [PERFORMANCE.md](PERFORMANCE.md) are measured on one
machine and are not a promise about any other. The suite is the promise.

### A formal specification of the matching semantics

The semantics are checked against Python's `re` on 21,980 cases, and the
property tests check the invariants. What is missing is a written
specification of the leftmost-first rule that the two of them implement,
so that a reader does not have to infer it from the code.

The plan is a short document - the rule, the capture semantics, the
anchoring, the empty match - and a property test for every clause of it.

### Faster review of the refusals

The dialect refuses a list of constructs, and each refusal is a test. The
list grows as the engine grows, and it is the kind of thing that should be
generated: one table, and the parser, the tests and the documentation
should all read it.

## What the project will not do

* **It will not become a general purpose regex library.** The dialect is
  small on purpose and the limits are low on purpose. A project that wants
  the full syntax should use `regex` or `regex-automata`, both of which
  are better at it.
* **It will not call a service.** The model is local, the training is
  offline, and the engine never touches the network. A device that cannot
  work without a network is not the device this project is for.
* **It will not grow the model.** The budget is 256 KiB and the model is
  97 KiB. A larger model would be a different project with a different
  purpose.
* **It will not add look-around.** The predictability of the matching time
  is the reason the project exists.
