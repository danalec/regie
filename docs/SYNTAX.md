# The pattern dialect

The dialect is deliberately small. Every construct that makes matching
unpredictable is refused with an explicit error instead of being silently
reinterpreted, and everything that is accepted has a defined, tested
meaning.

The engine works on **bytes**. A pattern is a sequence of bytes, and the
input is a sequence of bytes. The `unicode` feature is reserved; nothing in
the dialect depends on it yet.

## The grammar

```text
alternation := concat ('|' concat)*
concat      := repeat*
repeat      := atom quantifier?
quantifier  := '*' | '+' | '?' | '{n}' | '{n,}' | '{n,m}' ('?')?
atom        := '(' group ')' | '[' class ']' | '.' | '^' | '$' | escape | literal
group       := '?:' pattern | '?' flags ':' pattern | pattern
flags       := 'i' | 'm' | 's' | 'ims'
```

## Literals

Any byte that is not a metacharacter stands for itself. The metacharacters
are:

| character | meaning |
|---|---|
| `.` | any byte except a newline |
| `^` | the start of the text |
| `$` | the end of the text |
| `(` `)` | a group |
| `[` `]` | a character class |
| `\` | an escape |
| `\|` `\*` `\+` `\?` `\{` `\}` | the metacharacters themselves |

A literal metacharacter is written with a backslash: `\.` is a dot, not "any
byte".

## Character classes

```text
[a-z]      a byte between 'a' and 'z'
[^a-z]     any byte except those between 'a' and 'z'
[a-z0-9_]  a union of the ranges and the bytes
```

* A range whose end is before its start is an error: `[z-a]` is refused.
* An empty class is an error: `[]` is refused.
* A `]` has to be escaped inside a class: `[^]]` is refused, `[\]]` is the
  class of the `]` byte.
* A `-` at the beginning or the end of a class is a literal.

The shorthands are classes:

| shorthand | bytes |
|---|---|
| `\d` | `0` to `9` |
| `\D` | anything but `0` to `9` |
| `\w` | `a` to `z`, `A` to `Z`, `0` to `9`, `_` |
| `\W` | anything but `\w` |
| `\s` | space, tab, newline, carriage return, form feed, vertical tab |
| `\S` | anything but `\s` |

A shorthand inside a class is a class too: `[\d-]` is the class of the
digits and of the `-` byte.

## Escapes

| escape | meaning |
|---|---|
| `\n` `\r` `\t` `\f` `\v` `\a` `\e` | the control character |
| `\0` | the null byte |
| `\xHH` | the byte with that hexadecimal value |
| `\b` | a word boundary, outside a class |
| `\B` | the negation of a word boundary |
| `\d` `\D` `\w` `\W` `\s` `\S` | the shorthands above |

An escape of a punctuation mark stands for that byte: `\-`, `\/`, `\#` and
`\ ` are all literal.

**An unknown escape of a letter is an error.** `\q` is refused with
`unknown escape sequence`; it is not read as the letter `q`. The reason is
the same as for every other refusal in this document: a typo must be
reported, not matched.

## Groups

```text
(ab)      a capturing group, numbered from one
(?:ab)     a group that captures nothing
(?i)      a flag that applies to the rest of the pattern
(?i:ab)   a flag that applies to the group
```

There are at most **eight** capture groups. A pattern with more is refused
with `TooManyGroups`.

The flags are:

| flag | meaning |
|---|---|
| `i` | the pattern is matched without regard to case |
| `m` | `^` and `$` match at the ends of a line, not only of the text |
| `s` | `.` matches a newline too |

## Quantifiers

| quantifier | meaning |
|---|---|
| `*` | zero or more |
| `+` | one or more |
| `?` | zero or one |
| `{n}` | exactly `n` |
| `{n,}` | `n` or more |
| `{n,m}` | between `n` and `m` |

A quantifier is greedy by default: it takes as much as it can. A `?` right
after it makes it lazy: it takes as little as it can.

```text
a+       greedy
a+?      lazy
```

The upper bound is at most **255**. A larger one is refused with
`RepetitionTooLarge`.

## Anchors and assertions

| construct | meaning |
|---|---|
| `^` | the start of the text (of a line, with the `m` flag) |
| `$` | the end of the text (of a line, with the `m` flag) |
| `\b` | a word boundary |
| `\B` | not a word boundary |

`^` and `$` are the only anchors the **deterministic** automaton can
express. A `\b` in the middle of a pattern is matched by the NFA engine but
cannot be compiled into a DFA, so building one is refused with
`Unsupported`; the pattern still works through the NFA.

## What the dialect refuses

Every refusal is an explicit error with a position. The list is:

| construct | error |
|---|---|
| `(?=x)` `(?!)` | `Unsupported { feature: "look-ahead" }` |
| `(?<=x)` `(?<!x)` | `Unsupported { feature: "look-behind" }` |
| `(?#text)` | `Unsupported { feature: "comment group" }` |
| `(?<name>x)` `(?P<name>x)` | `Unsupported { feature: "named group" }` |
| `(?U)` `(?x)` and any other flag | `Unsupported { feature: "flag" }` |
| `(?z)` | `Syntax { message: "unrecognized group syntax" }` |
| `\1` … `\9` | `Unsupported { feature: "backreference" }` |
| `\uXXXX` | `Unsupported { feature: "\\u escape" }` |
| `\p{L}` | `Unsupported { feature: "Unicode property class" }` |
| `a*+`, `a++`, `a?+` | `Syntax { message: "repeated quantifier" }` |
| `*a` | `Syntax { message: "nothing to repeat" }` |
| `a{3,2}` | `Syntax { message: "repetition upper bound is smaller than the lower bound" }` |
| `[z-a]` | `InvalidClass { message: "character range is reversed" }` |
| `[]` | `InvalidClass { message: "empty character class" }` |
| `[^]]` | `InvalidClass` - the `]` has to be escaped |
| `\q` | `Syntax { message: "unknown escape sequence" }` |
| a pattern longer than 512 bytes | `PatternTooLong` |
| more than 32 levels of nesting | `NestingTooDeep` |

Look-around and backreferences are refused because they are where the
predictability of a regular expression ends. A possessive quantifier is
refused because it is a different language, and the engine would rather
say so than match something else. An unknown escape is refused because it
is almost always a mistake.

## How the dialect compares with Python's `re`

The dialect is checked against Python's `re` by a differential test
(`tools/differential_test.py`) that compares **21,323** cases. The two
dialects differ on purpose in four places:

| difference | Python | `regie` |
|---|---|---|
| `$` before a trailing newline | matches | does not match |
| `[^]]` | a `]` right after `^` is literal | the `]` has to be escaped |
| a possessive quantifier (`a*+`) | accepted | refused |
| `\01` | an octal escape | `\0` is the null byte, `1` is a literal |

Apart from those, the two agree: the test compares the spans of the matches,
the cases where neither matches, and the cases where both refuse the
pattern. The current result is zero mismatches.

## The limits of the build

| limit | value | error |
|---|---|---|
| the length of a pattern | 512 bytes | `PatternTooLong` |
| the number of capture groups | 8 | `TooManyGroups` |
| the number of capture slots | 16 | - |
| the number of instructions | 512 | `ProgramTooLarge` |
| the number of nodes of the syntax tree | 512 | `ProgramTooLarge` |
| the depth of the nesting | 32 | `NestingTooDeep` |
| the upper bound of a repetition | 255 | `RepetitionTooLarge` |
| the number of threads of a match | 256 | `ThreadLimitExceeded` |
| the number of byte classes | 24 | `TooManyClasses` |
| the number of states of a DFA | 256 | `DfaTooLarge` |
| the number of classes of a DFA | 32 | `TooManyClasses` |

The limits exist so that every value of the engine has a size that is known
at compile time, and so that no input can make the engine allocate.

## The structured specification

The intent layer does not produce patterns. It produces a
`CandidateSpec`, which is one of four families:

```rust
pub enum CandidateSpec {
    Validate { kind: ValidationKind, strictness: Strictness, charset: CharsetPolicy, max_len: Option<u16> },
    Extract  { kind: ExtractionKind, boundary: BoundaryPolicy, capture: CapturePolicy },
    Filter   { action: FilterAction, terms: SmallVec<PatternTerm, 4> },
    Custom   { template: PatternTemplate, params: PatternParams },
}
```

Every field is an enumerated choice or a bounded value, so a specification
can always be compiled, and it can never carry syntax that the compiler has
not seen. The 31 validation families and the 10 extraction families are
listed in the documentation of [`regie_core::spec`](../crates/regie-core/src/spec.rs).

The families validate the **shape** of a value. A regular expression cannot
compute a checksum, so the CPF family checks the shape of a CPF and not the
value of its check digits; the same goes for the CNPJ, for the credit card
and for every other family. A request that needs the checksum is a request
for a program, not for a regular expression, and the verified search is
where the extra rule belongs: it takes the examples of the caller as the
discriminator.
