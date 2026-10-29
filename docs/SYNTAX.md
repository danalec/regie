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

## A catalogue of accepted constructs

Every row of this section was run against the engine. The last two columns
are inputs, not patterns: the left one matches, the right one does not.

In the tables, `<SP>` is a space, `<TAB>` a tab, `<LF>` a line feed, `<CR>` a
carriage return, `<FF>` a form feed, `<VT>` a vertical tab, `<BEL>` a bell
(0x07), `<ESC>` an escape (0x1b) and `<NUL>` the null byte.

### Literals and byte escapes

| syntax | pattern | matches | does not match |
|---|---|---|---|
| literal text | `abc` | `abc` | `abd` |
| any byte but a newline | `a.c` | `abc` | `ac` |
| an escaped metacharacter | `\.` | `.` | `x` |
| an escaped backslash | `\\` | `\` | `x` |
| an escaped punctuation mark | `\-` | `-` | `x` |
| the line feed | `\n` | `<LF>` | `n` |
| the carriage return | `\r` | `<CR>` | `n` |
| the tab | `\t` | `<TAB>` | `n` |
| the form feed | `\f` | `<FF>` | `n` |
| the vertical tab | `\v` | `<VT>` | `n` |
| the bell | `\a` | `<BEL>` | `n` |
| the escape | `\e` | `<ESC>` | `n` |
| the null byte | `\0` | `<NUL>` | `1` |
| a hexadecimal byte | `\x41` | `A` | `B` |

### Classes

| syntax | pattern | matches | does not match |
|---|---|---|---|
| a set of bytes | `[abc]` | `b` | `d` |
| a range | `[a-z]` | `q` | `Q` |
| a union of ranges and bytes | `[a-z0-9_]` | `_` | `-` |
| a negated class | `[^a-z]` | `Q` | `q` |
| a leading `-` is a literal | `[-a]` | `-` | `b` |
| a trailing `-` is a literal | `[a-]` | `a` | `b` |
| a metacharacter inside a class is literal | `[.]` | `.` | `x` |
| an escape inside a class | `[\x41]` | `A` | `B` |

### The shorthands

| syntax | pattern | matches | does not match |
|---|---|---|---|
| a digit | `\d` | `7` | `x` |
| not a digit | `\D` | `x` | `7` |
| a word byte | `\w` | `_` | `-` |
| not a word byte | `\W` | `-` | `_` |
| white space | `\s` | `<SP>` | `x` |
| not white space | `\S` | `x` | `<SP>` |

### The shorthands inside a class

| syntax | pattern | matches | does not match |
|---|---|---|---|
| a digit | `[\d]` | `7` | `x` |
| not a digit | `[\D]` | `x` | `7` |
| a union of two shorthands | `[\d\s]` | `<SP>` | `x` |
| a shorthand and a literal | `[\w-]` | `x` | `.` |

### Anchors

| syntax | pattern | matches | does not match |
|---|---|---|---|
| the start of the text | `^abc` | `abc` | `xabc` |
| the end of the text | `abc$` | `abc` | `abcx` |
| a word boundary | `\bcat` | `a cat` | `concat` |
| not a word boundary | `\Bcat` | `concat` | `a cat` |

### Groups and alternation

| syntax | pattern | matches | does not match |
|---|---|---|---|
| a capturing group | `(ab)c` | `abc` | `ac` |
| a non-capturing group | `(?:ab)c` | `abc` | `ac` |
| alternation | `cat\|dog` | `dog` | `bird` |

### Quantifiers

| syntax | pattern | matches | does not match |
|---|---|---|---|
| zero or more | `a*b` | `aaab` | `aaac` |
| one or more | `a+b` | `aaab` | `b` |
| zero or one | `^ab?$` | `a` | `ac` |
| exactly `n` | `^a{3}$` | `aaa` | `aa` |
| `n` or more | `^a{2,}$` | `aa` | `a` |
| between `n` and `m` | `^a{2,3}$` | `aa` | `a` |
| lazy, zero or more | `a*?b` | `aab` | `aac` |
| lazy, one or more | `a+?b` | `ab` | `aac` |
| lazy, zero or one | `^a??$` | `a` | `ab` |
| lazy, between `n` and `m` | `^a{2,3}?$` | `aa` | `a` |

### The inline flags

| syntax | pattern | matches | does not match |
|---|---|---|---|
| `i` for the rest of the pattern | `(?i)abc` | `ABC` | `abd` |
| `i` for a group | `(?i:abc)` | `ABC` | `abd` |
| `i` turned off inside a group | `(?i)a(?-i:b)c` | `Abc` | `ABc` |
| `s` for the rest of the pattern | `(?s)a.b` | `a<LF>b` | `ab` |
| `s` for a group | `(?s:a.b)` | `a<LF>b` | `ab` |
| `m` for the rest of the pattern | `(?m)^b` | `a<LF>b` | `xb` |
| `m` for a group | `(?m:^b)` | `a<LF>b` | `xb` |
| two flags at once | `(?is)a.c` | `A<LF>C` | `xyz` |

The `i` flag folds the case of a literal and of a class alike: `(?i)[a-z]`
matches `Q`.

## What the dialect refuses

Every refusal is an explicit error with a position. The list is:

| construct | error |
|---|---|
| `(?=x)` `(?!)` | `Unsupported { feature: "look-ahead" }` |
| `(?<=x)` `(?<!x)` | `Unsupported { feature: "look-behind" }` |
| `(?#text)` | `Unsupported { feature: "comment group" }` |
| `(?<name>x)` | `Unsupported { feature: "look-behind" }` - the parser reads `(?<` as a look-behind |
| `(?P<name>x)` | `Unsupported { feature: "named group" }` |
| `(?U)` `(?x)` and any other flag | `Unsupported { feature: "flag" }` |
| `(?z)` | `Syntax { message: "unrecognized group syntax" }` |
| `\1` … `\9` | `Unsupported { feature: "backreference" }` |
| `\uXXXX` | `Unsupported { feature: "\\u escape" }` |
| `\p{L}` | `Unsupported { feature: "Unicode property class" }` |
| `a*+`, `a++`, `a?+` | `Syntax { message: "repeated quantifier" }` |
| `*a` | `Syntax { message: "nothing to repeat" }` |
| `a{3,2}` | `InvalidClass { message: "repetition upper bound is smaller than the lower bound" }` |
| `[z-a]` | `InvalidClass { message: "character range is reversed" }` |
| `[]` | `InvalidClass { message: "empty character class" }` |
| `[^]]` | `InvalidClass { message: "empty character class" }` - the `]` closes the class |
| `\q` | `Syntax { message: "unknown escape sequence" }` |
| a pattern longer than 512 bytes | `PatternTooLong` |
| more than 32 levels of nesting | `NestingTooDeep` |

Look-around and backreferences are refused because they are where the
predictability of a regular expression ends. A possessive quantifier is
refused because it is a different language, and the engine would rather
say so than match something else. An unknown escape is refused because it
is almost always a mistake.

## A catalogue of refused constructs

Every row of this section was run against the engine. The message is the one
the engine returns, without the `error: invalid pattern ...: ` prefix that
the command line tool adds in front of it.

| syntax | example | message | why |
|---|---|---|---|
| look-ahead | `(?=x)` | ``unsupported construct `look-ahead` at byte 2`` | a look-ahead makes the match depend on text the pattern has not consumed |
| negative look-ahead | `(?!x)` | ``unsupported construct `look-ahead` at byte 2`` | the same reason, with the opposite sense |
| look-behind | `(?<=x)` | ``unsupported construct `look-behind` at byte 2`` | a look-behind needs the text before the match |
| negative look-behind | `(?<!x)` | ``unsupported construct `look-behind` at byte 2`` | the same reason, with the opposite sense |
| a named group, `(?P` form | `(?P<name>x)` | ``unsupported construct `named group` at byte 2`` | the engine has numbered captures only |
| a named group, `(?<` form | `(?<name>x)` | ``unsupported construct `look-behind` at byte 2`` | the parser reads `(?<` as the start of a look-behind |
| a comment group | `(?#text)` | ``unsupported construct `comment group` at byte 2`` | a comment is not part of the grammar |
| an unknown flag | `(?U)` | ``unsupported construct `flag` at byte 2`` | the engine has the three flags `i`, `m` and `s` |
| an unknown group form | `(?z)` | `syntax error at byte 2: unrecognized group syntax` | a `(?` that names nothing known is a mistake |
| a backreference | `\1` | ``unsupported construct `backreference` at byte 0`` | a backreference makes the match depend on what was matched before |
| a code point escape | `\u0041` | ``unsupported construct `\u escape` at byte 0`` | the dialect is byte-based, so a code point has no meaning in it |
| a Unicode property class | `\p{L}` | ``unsupported construct `Unicode property class` at byte 0`` | a property class needs a Unicode table the engine does not carry |
| a possessive quantifier | `a*+` | `syntax error at byte 2: repeated quantifier` | a possessive quantifier is a different language, and the engine would rather say so |
| a quantifier with nothing to repeat | `*a` | `syntax error at byte 0: nothing to repeat` | a quantifier needs an atom in front of it |
| an impossible bound | `a{3,2}` | `invalid character class at byte 1: repetition upper bound is smaller than the lower bound` | no repetition satisfies the bound |
| a reversed range | `[z-a]` | `invalid character class at byte 0: character range is reversed` | the range is empty |
| an empty class | `[]` | `invalid character class at byte 0: empty character class` | a class needs at least one byte |
| a negated class closed by `]` | `[^]]` | `invalid character class at byte 0: empty character class` | the `]` closes the class, so nothing is left to negate |
| an unknown escape of a letter | `\q` | `syntax error at byte 0: unknown escape sequence` | a typo must be reported, not matched |
| an unclosed group | `(` | `syntax error at byte 1: unclosed group` | the group is never closed |
| an unmatched `)` | `abc)` | `syntax error at byte 3: unbalanced ')'` | there is no group for it to close |
| a trailing backslash | `abc\` | `syntax error at byte 3: trailing backslash` | the escape has no body |
| a truncated hexadecimal escape | `\x4` | `syntax error at byte 0: truncated hexadecimal escape` | the escape needs two digits |
| an invalid hexadecimal escape | `\xZZ` | `syntax error at byte 0: invalid hexadecimal escape` | `Z` is not a hexadecimal digit |
| an unclosed class | `[a-` | `invalid character class at byte 0: unclosed character class` | the class is never closed |
| a pattern over 512 bytes | 513 bytes of `a` | `pattern is too long: 513 bytes, limit is 512` | the pattern lives in a buffer of a known size |
| nine capture groups | `(a)(b)(c)(d)(e)(f)(g)(h)(i)` | `pattern uses more than 8 capture groups` | the capture slots are a fixed array |
| thirty-three levels of nesting | 33 levels of `(?:` | `pattern nests groups deeper than 32` | the parser is recursive, and its depth is bounded |
| a bound over 255 | `a{256}` | `repetition bound exceeds 255` | the bound is compiled into a program of a known size |

## How the engine resolves a match

The engine simulates an NFA and never backtracks. Its threads are kept in
priority order, so the match that is reported is the leftmost one, and among
the matches that start at the same position, the branch the pattern prefers.

### Alternation is leftmost-first

The branch that comes first in the pattern wins, even when the other branch
would match more:

| pattern | input | match |
|---|---|---|
| `a\|ab` | `ab` | `a` (0..1) |
| `ab\|a` | `ab` | `ab` (0..2) |

### The longest match is the one the chosen branch produces

Inside the branch that won, the quantifiers are greedy, so the match is as
long as that branch can make it:

| pattern | input | match |
|---|---|---|
| `a+` | `aaa` | `aaa` (0..3) |
| `a+?` | `aaa` | `a` (0..1) |
| `a*` | `aaa` | `aaa` (0..3) |
| `a*?` | `aaa` | the empty match (0..0) |
| `.*` | `abc` | `abc` (0..3) |
| `.*?` | `abc` | the empty match (0..0) |

The engine does not look for the longest match anywhere in the input. It
returns the match that starts earliest, so `a+` against `xxaa` matches `aa`
at 2..4.

### A group that does not take part holds nothing

`captures.get(index)` is `None` for a group that did not take part in the
match, and not an empty string:

| pattern | input | the groups |
|---|---|---|
| `(a)?b` | `b` | group 1 is `None` |
| `(a)\|b` | `b` | group 1 is `None` |
| `(x)(y)?z` | `xz` | group 1 is `x`, group 2 is `None` |

A group that matched zero bytes is a different case: `(a*)b` against `b`
gives group 1 as the empty span 0..0.

### The anchors in multi-line mode

Without the `m` flag, `^` matches only at offset 0 and `$` only at the end of
the input. With it, they match at the ends of the lines as well:

| pattern | input | match |
|---|---|---|
| `^b` | `a<LF>b` | no match |
| `(?m)^b` | `a<LF>b` | `b` (2..3) |
| `(?m)$` | `a<LF>b` | the empty match at 1..1, right after the line feed |
| `(?m)^$` | `a<LF><LF>b` | the empty match at 2..2, the empty line |

## Gotchas

* **The engine matches bytes, not code points.** `.` against `é` matches one
  of its two bytes, and it takes two dots to cover the character. The
  `unicode` feature is declared in `Cargo.toml` but reserved: turning it on
  changes nothing today.
* **`.` does not match a newline** unless the `s` flag is on.
* **`^` and `$` are text anchors**, not line anchors, unless the `m` flag is
  on.
* **A pattern is at most 512 bytes long.** A pattern of 513 bytes is refused
  with `pattern is too long: 513 bytes, limit is 512`.
* **There are at most eight capture groups.** A pattern with a ninth group is
  refused with `pattern uses more than 8 capture groups`. Eight groups compile,
  whether they are nested or written one after another.
* **A class matches exactly one byte.** `[a-z]` never matches `ab`; use
  `[a-z]+` for that.
* **The engine returns the leftmost match**, not the longest one anywhere in
  the input.
* **An empty match is a match.** `a*` matches every input, because it can
  match zero bytes. Anchor the pattern with `^` and `$` when it has to
  cover the whole text.
* **A `{` that does not open a valid bound is a literal.** `a{` matches
  `a{`, and `a{b}` matches `a{b}`.
* **A scoped flag group stays scoped.** `(?i:...)` turns the flag on for the
  group and for nothing else, so `a(?i:b)C` does not match `aBc`: the `C`
  that follows the group is still case-sensitive. Putting `(?i)` at the front
  of the pattern is the way to ask for case-insensitive matching for the rest
  of the pattern.
* **A `\b` in the middle of a pattern cannot be compiled into a
  deterministic automaton.** Building one is refused with
  ``unsupported construct `zero-width assertion in a deterministic automaton` at byte 0``;
  the pattern still works through the NFA engine.

## Checking a pattern quickly

The `regie` command line tool compiles a pattern and runs it against an
input. `match` prints the span of the match and of every capture group;
`test` prints nothing but the verdict, and reports it in the exit code.

```console
$ cargo run -p regie-cli --bin regie -- match '(\w+)@(\w+)' 'mail: ana@corp'
match: 6..14 "ana@corp"
group 1: 6..9 "ana"
group 2: 10..14 "corp"

$ cargo run -p regie-cli --bin regie -- match '(?=x)' 'x'
error: invalid pattern `(?=x)`: unsupported construct `look-ahead` at byte 2
```

The exit code carries the verdict: `0` when the pattern matched, `1` when it
did not, `2` when the tool could not run or the engine refused the pattern.

```console
$ cargo run -p regie-cli --bin regie -- test '^\d{3}$' '123'
match
$ cargo run -p regie-cli --bin regie -- test '^\d{3}$' '1234'
no match
```

The examples are the galleries of the workspace:

| gallery | what it shows |
|---|---|
| [`regie-core/EXAMPLES.md`](../crates/regie-core/EXAMPLES.md) | the index of the gallery of `regie-core` |
| [`regie-core/examples/dialect_tour.rs`](../crates/regie-core/examples/dialect_tour.rs) | every construct the dialect accepts, with an input it matches and one it refuses |
| [`regie-core/examples/refusals.rs`](../crates/regie-core/examples/refusals.rs) | the refused syntax, with the exact message of each |
| [`regie-core/examples/captures.rs`](../crates/regie-core/examples/captures.rs) | the groups of a match, including one that did not take part |
| [`regie-core/examples/specifications.rs`](../crates/regie-core/examples/specifications.rs) | the four families of structured specification, compiled |
| [`regie-core/examples/limits.rs`](../crates/regie-core/examples/limits.rs) | the budgets of this build, and what happens when one of them is exceeded |
| [`regie-core/examples/error_handling.rs`](../crates/regie-core/examples/error_handling.rs) | every variant of `RegexError`, and the reaction a caller can attach to it |
| [`regie-core/examples/dfa_acceleration.rs`](../crates/regie-core/examples/dfa_acceleration.rs) | the deterministic automaton, and its verdict against the Pike VM |
| [`regie-core/examples/static_regex.rs`](../crates/regie-core/examples/static_regex.rs) | patterns, capture groups and a structured specification, in one run |
| [`regie-intent/EXAMPLES.md`](../crates/regie-intent/EXAMPLES.md) | a request turned into a specification, and the requests the model refuses |
| [`regie-search/EXAMPLES.md`](../crates/regie-search/EXAMPLES.md) | a request plus examples, narrowed down to one specification |
| [`regie-codec/examples/emit_artifact.rs`](../crates/regie-codec/examples/emit_artifact.rs) | a pattern written out as an artifact for a firmware image |
| [`regie-codec/examples/precompiled_dfa.rs`](../crates/regie-codec/examples/precompiled_dfa.rs) | matching against an automaton that was compiled ahead of time |

## How the dialect compares with Python's `re`

The dialect is checked against Python's `re` by a differential test
(`tools/differential_test.py`) that compares **21,980** cases. The two
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

## The specification layer

The structured specification is the alternative to writing a pattern by hand:
the compiler turns it into an automaton directly, with no pattern text in
between.

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

The intent model can only ever produce one of these values: it picks a
variant and fills in the enumerated fields, and there is nowhere in it to
write a pattern. A specification is compiled by handing it to
`Regex::from_spec`:

```rust
use regie_core::spec::{CandidateSpec, CharsetPolicy, Strictness, ValidationKind};
use regie_core::Regex;

let spec = CandidateSpec::Validate {
    kind: ValidationKind::BrazilianCpf,
    strictness: Strictness::Standard,
    charset: CharsetPolicy::Digits,
    max_len: Some(14),
};

let re = Regex::from_spec(&spec).expect("the specification compiles");
assert!(re.is_match(b"123.456.789-09"));
assert!(!re.is_match(b"123.456.789"));
```

The gallery of `regie-core` walks the same path with the other three
families:
[`crates/regie-core/examples/specifications.rs`](../crates/regie-core/examples/specifications.rs).

The families validate the **shape** of a value. A regular expression cannot
compute a checksum, so the CPF family checks the shape of a CPF and not the
value of its check digits; the same goes for the CNPJ, for the credit card
and for every other family. A request that needs the checksum is a request
for a program, not for a regular expression, and the verified search is
where the extra rule belongs: it takes the examples of the caller as the
discriminator.
