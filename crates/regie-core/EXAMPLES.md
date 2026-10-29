# The examples of `regie-core`

`regie-core` is the engine of [`regie`](../../README.md): a safe dialect, a
Thompson NFA, a Pike VM and a compact deterministic automaton. The examples
below walk through its public surface, from the dialect to the budgets of the
build.

Run an example from the root of the workspace:

```console
$ cargo run -p regie-core --example <name>
```

Every program is deterministic: the same run prints the same bytes, on every
platform, with no timestamp, no path and no random order. The only example
that needs a feature is `dfa_acceleration`, which needs `alloc`.

## `static_regex`

What it shows: the three ways to get a matcher out of the crate, in one
program.

Run it:

```console
$ cargo run -p regie-core --example static_regex
pattern: ^\d{3}-\d{4}$
matches "555-1234":  true
matches "5551234":   false
matches "x555-1234": false
whole match: Some(Match { start: 9, end: 21 }) (2 groups)
  group 1: ana
  group 2: corp
spec family: email
matches "ana@corp.com": true
matches "not-an-email":  false
(?=ahead)  rejected: unsupported construct `look-ahead` at byte 2
back\1     rejected: unsupported construct `backreference` at byte 4
[z-a]      rejected: invalid character class at byte 0: character range is reversed
a{3,2}     rejected: invalid character class at byte 1: repetition upper bound is smaller than the lower bound
the engine refuses look-ahead on purpose
```

Source: [`crates/regie-core/examples/static_regex.rs`](examples/static_regex.rs)

## `dialect_tour`

What it shows: every construct the dialect accepts, one line each, with an
input it matches and an input it refuses.

Run it:

```console
$ cargo run -p regie-core --example dialect_tour
CONSTRUCT          PATTERN      ACCEPTS                  REFUSES
literal            abc          "abc" -> match           "abd" -> no match
class              [a-z]        "m" -> match             "M" -> no match
range              [0-9]        "7" -> match             "x" -> no match
negated class      [^0-9]       "a" -> match             "5" -> no match
digit class        \d           "42" -> match            "a" -> no match
not a digit        \D           "ab" -> match            "7" -> no match
word class         \w           "a_1" -> match           "-" -> no match
not a word         \W           "-" -> match             "a" -> no match
space class        \s           " " -> match             "x" -> no match
not a space        \S           "x" -> match             " " -> no match
start anchor       ^abc         "abcdef" -> match        "xabc" -> no match
end anchor         abc$         "xxabc" -> match         "abcx" -> no match
word boundary      \bcat\b      "a cat here" -> match    "concatenate" -> no match
not a boundary     \Bcat        "concat" -> match        " cat" -> no match
capturing group    (ab)+        "abab" -> match          "ba" -> no match
plain group        (?:ab)+      "abab" -> match          "ba" -> no match
alternation        cat|dog      "dog" -> match           "cow" -> no match
zero or more       ab*          "a" -> match             "c" -> no match
one or more        ab+          "ab" -> match            "a" -> no match
optional           ab?          "a" -> match             "b" -> no match
exactly n          a{3}         "aaa" -> match           "aa" -> no match
at least n         a{2,}        "aaa" -> match           "a" -> no match
between n and m    a{2,3}       "aa" -> match            "a" -> no match
greedy             a+           "aaa" -> match           "b" -> no match
lazy               a+?          "aaa" -> match           "b" -> no match
flag i             (?i)abc      "ABC" -> match           "abd" -> no match
flag i scoped      (?i:abc)d    "ABCd" -> match          "ABCD" -> no match
flag m             (?m)^b$      "a\nb" -> match          "a\nbc" -> no match
flag m scoped      (?m:^b$)     "a\nb" -> match          "a\nbb" -> no match
dot                a.c          "abc" -> match           "a\nc" -> no match
flag s             (?s)a.c      "a\nc" -> match          "ac" -> no match
flag s scoped      (?s:a.c).    "a\ncX" -> match         "a\nc\n" -> no match
escape n           a\nb         "a\nb" -> match          "ab" -> no match
escape r           a\rb         "a\rb" -> match          "ab" -> no match
escape t           a\tb         "a\tb" -> match          "ab" -> no match
escape xHH         \x41         "A" -> match             "a" -> no match

greedy and lazy take different spans:
<.+>     matches "<b><i>"
<.+?>    matches "<b>"
```

Source: [`crates/regie-core/examples/dialect_tour.rs`](examples/dialect_tour.rs)

## `captures`

What it shows: the whole match, the numbered groups, a group that did not take
part, nested groups, and the span of a group inside the subject.

Run it:

```console
$ cargo run -p regie-core --example captures
pattern: (\w+)@(\w+)\.com
subject: "write to ana@corp.com today"
  group 0: 9..21 "ana@corp.com"
  group 1: 9..12 "ana"
  group 2: 13..17 "corp"
subject: "2026-10"
  group 0: 0..7 "2026-10"
  group 1: 0..4 "2026"
  group 2: 5..7 "10"
  group 3: did not take part
subject: "2026-10-29"
  group 0: 0..10 "2026-10-29"
  group 1: 0..4 "2026"
  group 2: 5..7 "10"
  group 3: 8..10 "29"
pattern: ((ab)(cd))
subject: "abcd"
  group 0: 0..4 "abcd"
  group 1: 0..4 "abcd"
  group 2: 0..2 "ab"
  group 3: 2..4 "cd"
group 1 is 9..12
its text is "ana"
```

Source: [`crates/regie-core/examples/captures.rs`](examples/captures.rs)

## `specifications`

What it shows: the four families of structured specification, compiled without
any pattern text, each matched against an input it accepts and one it refuses.

Run it:

```console
$ cargo run -p regie-core --example specifications
FAMILY     KIND           ACCEPTS                        REFUSES
validate   email          "ana@corp.com" -> match        "not-an-email" -> no match
extract    quoted         "he said \"hello there\"" -> match "no quotes here" -> no match
filter     filter         "buy spam now" -> match        "buy soap now" -> no match
custom     custom         "ID-1234" -> match             "ID-12" -> no match

the extracted part:
  group 1: 9..20 "hello there"
```

Source: [`crates/regie-core/examples/specifications.rs`](examples/specifications.rs)

## `refusals`

What it shows: the catalogue of syntax the engine refuses, with the exact
message it returns for each.

Run it:

```console
$ cargo run -p regie-core --example refusals
CONSTRUCT                        PATTERN        VARIANT            MESSAGE
look-ahead                       (?=ahead)      Unsupported        unsupported construct `look-ahead` at byte 2
negative look-ahead              (?!ahead)      Unsupported        unsupported construct `look-ahead` at byte 2
look-behind                      (?<=behind)    Unsupported        unsupported construct `look-behind` at byte 2
negative look-behind             (?<!behind)    Unsupported        unsupported construct `look-behind` at byte 2
backreference                    (a)\1          Unsupported        unsupported construct `backreference` at byte 3
named group                      (?P<name>x)    Unsupported        unsupported construct `named group` at byte 2
named group, angle brackets      (?<name>x)     Unsupported        unsupported construct `look-behind` at byte 2
recursion                        (?R)           Syntax             syntax error at byte 2: unrecognized group syntax
conditional                      (?(1)a|b)      Syntax             syntax error at byte 2: unrecognized group syntax
possessive quantifier            a*+            Syntax             syntax error at byte 2: repeated quantifier
unknown escape                   \q             Syntax             syntax error at byte 0: unknown escape sequence
reversed range                   [z-a]          InvalidClass       invalid character class at byte 0: character range is reversed
reversed bounds                  a{3,2}         InvalidClass       invalid character class at byte 1: repetition upper bound is smaller than the lower bound
unclosed group                   (abc           Syntax             syntax error at byte 4: unclosed group
stray parenthesis                abc)           Syntax             syntax error at byte 3: unbalanced ')'
stray quantifier                 *abc           Syntax             syntax error at byte 0: nothing to repeat
comment group                    (?#text)       Unsupported        unsupported construct `comment group` at byte 2
unicode escape                   \u0041         Unsupported        unsupported construct `\u escape` at byte 0
unicode property                 \p{L}          Unsupported        unsupported construct `Unicode property class` at byte 0
unknown flag                     (?x)           Unsupported        unsupported construct `flag` at byte 2
```

Source: [`crates/regie-core/examples/refusals.rs`](examples/refusals.rs)

## `limits`

What it shows: the budgets of the build, and exactly what the engine does when
one of them is exceeded.

Run it:

```console
$ cargo run -p regie-core --example limits
the budgets of this build:
  the length of a pattern: 512 bytes
  the number of groups:    8

the length of a pattern:
  512 bytes -> compiled
  513 bytes -> pattern is too long: 513 bytes, limit is 512
the number of capture groups:
  8 groups -> compiled
  9 groups -> refused: pattern uses more than 8 capture groups
the size of the compiled program:
  (?:a{170}){3} -> compiled
  (?:a{171}){3} -> refused: compiled program exceeds 512 instructions
the upper bound of a repetition:
  a{255} -> compiled
  a{256} -> repetition bound exceeds 255
the depth of the nesting:
  32 nested groups -> compiled
  33 nested groups -> pattern nests groups deeper than 32
```

Source: [`crates/regie-core/examples/limits.rs`](examples/limits.rs)

## `dfa_acceleration`

What it shows: the deterministic automaton of a pattern, its size, and its
verdict next to the verdict of the Pike VM on the same inputs.

Run it:

```console
$ cargo run -p regie-core --example dfa_acceleration --features alloc
pattern ^[0-9]{3}-[0-9]{4}$:
  automaton: 10 states, 3 byte classes
  "555-1234" -> pike vm: match, automaton: match (agree)
  "5551234" -> pike vm: no match, automaton: no match (agree)
  "x555-1234" -> pike vm: no match, automaton: no match (agree)
  "ana@corp.com" -> pike vm: no match, automaton: no match (agree)
  "aab" -> pike vm: no match, automaton: no match (agree)
  "b" -> pike vm: no match, automaton: no match (agree)
  "a cat" -> pike vm: no match, automaton: no match (agree)
  "" -> pike vm: no match, automaton: no match (agree)
pattern [a-z]+@[a-z]+\.[a-z]{2,}:
  automaton: 8 states, 4 byte classes
  "555-1234" -> pike vm: no match, automaton: no match (agree)
  "5551234" -> pike vm: no match, automaton: no match (agree)
  "x555-1234" -> pike vm: no match, automaton: no match (agree)
  "ana@corp.com" -> pike vm: match, automaton: match (agree)
  "aab" -> pike vm: no match, automaton: no match (agree)
  "b" -> pike vm: no match, automaton: no match (agree)
  "a cat" -> pike vm: no match, automaton: no match (agree)
  "" -> pike vm: no match, automaton: no match (agree)
pattern a+b:
  automaton: 4 states, 3 byte classes
  "555-1234" -> pike vm: no match, automaton: no match (agree)
  "5551234" -> pike vm: no match, automaton: no match (agree)
  "x555-1234" -> pike vm: no match, automaton: no match (agree)
  "ana@corp.com" -> pike vm: no match, automaton: no match (agree)
  "aab" -> pike vm: match, automaton: match (agree)
  "b" -> pike vm: no match, automaton: no match (agree)
  "a cat" -> pike vm: no match, automaton: no match (agree)
  "" -> pike vm: no match, automaton: no match (agree)
pattern \bcat\b:
  no automaton: unsupported construct `zero-width assertion in a deterministic automaton` at byte 0
  the Pike VM still answers: "a cat" matches
```

Source: [`crates/regie-core/examples/dfa_acceleration.rs`](examples/dfa_acceleration.rs)

## `error_handling`

What it shows: the error type as a value, with one line of the program per
variant and the reaction a caller can attach to it.

Run it:

```console
$ cargo run -p regie-core --example error_handling
VARIANT                REACTION                   MESSAGE
PatternTooLong         shorten the pattern        pattern is too long: 513 bytes, limit is 512
Syntax                 fix the pattern at byte 4  syntax error at byte 4: unbalanced ')'
Unsupported            do not use look-ahead      unsupported construct `look-ahead` at byte 2
ProgramTooLarge        use a smaller pattern      compiled program exceeds 512 instructions
TooManyGroups          use fewer than 8 groups    pattern uses more than 8 capture groups
RepetitionTooLarge     lower the bound to 255     repetition bound exceeds 255
NestingTooDeep         nest less than 32 levels   pattern nests groups deeper than 32
InvalidClass           fix the class at byte 0    invalid character class at byte 0: character range is reversed
DfaTooLarge            keep the Pike VM           determinized automaton exceeds 256 states
TooManyClasses         use fewer than 24 classes  pattern needs more than 24 byte equivalence classes
ThreadLimitExceeded    use fewer than 256 threads matcher exceeded its budget of 256 threads

an error as the engine returns it:
Unsupported            do not use look-ahead      unsupported construct `look-ahead` at byte 2
```

Source: [`crates/regie-core/examples/error_handling.rs`](examples/error_handling.rs)

## `differential`

What it shows: the helper of the differential test, which reads hex-encoded
patterns and inputs from stdin and prints the span of each match.

Run it:

```console
$ cargo run -p regie-core --example differential
```

It reads one `pattern input` pair of hex bytes per line, so it prints nothing
until it is given a payload:

```console
$ echo "616263 616263" | cargo run -p regie-core --example differential
0 3
```

The corpus comes from
[`tools/differential_test.py`](../../tools/differential_test.py), which
compares the engine with another implementation of the same semantics.

Source: [`crates/regie-core/examples/differential.rs`](examples/differential.rs)

## Choosing between the Pike VM and the DFA

The two engines answer the same question, and on the patterns the automaton
can be built from, they agreed on every input of the comparison above: 24
verdicts, no difference.

They do not cover the same dialect, though. The Pike VM takes the whole
dialect: it answered for `\bcat\b`, which has no automaton at all, and it is
the only one of the two that reports capture groups. The automaton covers the
patterns it can express: it refuses a zero-width assertion that is neither
`^` nor `$`, and the refusal is a value, not a panic.

The automaton is the form to embed. It is a single left-to-right pass with no
state per match, and the tables stay small: the three patterns measured above
built 4 to 10 states over 3 to 4 byte classes. The Pike VM is the form to
use when the pattern needs a capture group, a word boundary, or a
multi-line anchor, or when the pattern is only known at run time and there is
no memory to spend on a determinization.

In short: build the automaton when the pattern is fixed and the dialect allows
it, keep the Pike VM for everything else.

## `linear_time`

What it shows: that the worst case of the engine is linear. The five patterns
below are the ones that make a backtracking engine take exponential time on a
subject of a few hundred bytes. This engine simulates a Thompson NFA with a
Pike VM instead of backtracking, so the work is `O(m * n)` for a program of `m`
instructions over a subject of `n` bytes, and every case ends. The last two
cases end on a subject of one mebibyte.

The program prints no timings on purpose: the point is not that the run is
fast, but that it finishes at all.

A match of zero bytes is still a match: `(a|a)*$` finds the empty string at
the end of a subject that ends with a `b`, because `*` allows zero
repetitions. A backtracking engine answers the same.

Run it:

```console
$ cargo run -p regie-core --example linear_time
the patterns that break a backtracking engine:
PATTERN         LENGTH SUBJECT      VERDICT    SPAN
(a+)+$              64 all a        match      0..64
(a+)+$              65 all a + b    no match   -
(a+)+$             256 all a        match      0..256
(a+)+$             257 all a + b    no match   -
(a+)+$            1024 all a        match      0..1024
(a+)+$            1025 all a + b    no match   -
(a+)+$            4096 all a        match      0..4096
(a+)+$            4097 all a + b    no match   -
(a|aa)+$            64 all a        match      0..64
(a|aa)+$            65 all a + b    no match   -
(a|aa)+$           256 all a        match      0..256
(a|aa)+$           257 all a + b    no match   -
(a|aa)+$          1024 all a        match      0..1024
(a|aa)+$          1025 all a + b    no match   -
(a|aa)+$          4096 all a        match      0..4096
(a|aa)+$          4097 all a + b    no match   -
(a|a)*$             64 all a        match      0..64
(a|a)*$             65 all a + b    match      65..65
(a|a)*$            256 all a        match      0..256
(a|a)*$            257 all a + b    match      257..257
(a|a)*$           1024 all a        match      0..1024
(a|a)*$           1025 all a + b    match      1025..1025
(a|a)*$           4096 all a        match      0..4096
(a|a)*$           4097 all a + b    match      4097..4097
(a*)*b              64 all a        no match   -
(a*)*b              65 all a + b    match      0..65
(a*)*b             256 all a        no match   -
(a*)*b             257 all a + b    match      0..257
(a*)*b            1024 all a        no match   -
(a*)*b            1025 all a + b    match      0..1025
(a*)*b            4096 all a        no match   -
(a*)*b            4097 all a + b    match      0..4097
((a)|(a))*b         64 all a        no match   -
((a)|(a))*b         65 all a + b    match      0..65
((a)|(a))*b        256 all a        no match   -
((a)|(a))*b        257 all a + b    match      0..257
((a)|(a))*b       1024 all a        no match   -
((a)|(a))*b       1025 all a + b    match      0..1025
((a)|(a))*b       4096 all a        no match   -
((a)|(a))*b       4097 all a + b    match      0..4097

the same engine on a subject of one mebibyte:
PATTERN         LENGTH SUBJECT      VERDICT    SPAN
a+$            1048576 all a        match      0..1048576
a+$            1048577 all a + b    no match   -
```

Source: [`crates/regie-core/examples/linear_time.rs`](examples/linear_time.rs)

## `hard_patterns`

What it shows: the hard parts of the dialect. Nested groups and the way they
are numbered, the limit on the number of groups, greedy against lazy, the
precedence of the alternation, the anchors with and without the multi-line
flag, the scoped flag against the global one, the empty match, the word
boundary at the edges of the subject, the bounds of a repetition, and the
class the engine refuses.

Every case is one row: the verdict, the span of the match, or the exact error
the engine returns for a pattern it refuses.

Run it:

```console
$ cargo run -p regie-core --example hard_patterns
CONSTRUCT                    PATTERN                      SUBJECT      VERDICT   SPAN OR ERROR
nested groups                ((a)(b(c)))                  "abc"        match     0..3
group limit, eight           (a)(b)(c)(d)(e)(f)(g)(h)     "abcdefgh"   match     0..8
group limit, nine            (a)(b)(c)(d)(e)(f)(g)(h)(i)  "abcdefghi"  refused   pattern uses more than 8 capture groups
greedy                       <(.+)>                       "<a><b>"     match     0..6
lazy                         <(.+?)>                      "<a><b>"     match     0..3
alternation                  cat|dog                      "cats"       match     0..3
grouped alternation          (cat|dog)s?                  "cats"       match     0..4
text anchor                  ^b$                          "a\nb"       no match  -
line anchor                  (?m)^b$                      "a\nb"       match     2..3
scoped flag                  (?i:abc)d                    "ABCd"       match     0..4
scoped flag, other case      (?i:abc)d                    "ABCD"       no match  -
global flag                  (?i)abc                      "ABC"        match     0..3
empty match                  a*                           "bbb"        match     0..0
word boundary                \bcat\b                      "cat"        match     0..3
word boundary, in a word     \bcat\b                      "concat"     no match  -
zero repetitions             a{0}                         "aa"         match     0..0
bounded repetitions          a{2,3}                       "aa"         match     0..2
bounded repetitions, greedy  a{2,3}                       "aaaa"       match     0..3
reversed range               [z-a]                        "abc"        refused   invalid character class at byte 0: character range is reversed

the groups of ((a)(b(c))) on "abc":
  group 0: 0..3 "abc"
  group 1: 0..3 "abc"
  group 2: 0..1 "a"
  group 3: 1..3 "bc"
  group 4: 2..3 "c"
```

Source: [`crates/regie-core/examples/hard_patterns.rs`](examples/hard_patterns.rs)

## `dfa_vs_nfa`

What it shows: that the two engines answer the same question. For four hard
patterns it builds the deterministic automaton, prints its size, and compares
the Pike VM with the automaton on every subject: the verdict of one against
the verdict of the other, and the end of the match of one against the end of
the match of the other.

Thirty-two comparisons, thirty-two agreements.

Run it:

```console
$ cargo run -p regie-core --example dfa_vs_nfa --features alloc
pattern (a|aa)+$:
  automaton: 4 states, 2 byte classes
  SUBJECT                        PIKE VM        AUTOMATON        VERDICT  END
  "aaaa"                         match 0..4     match end 4      agree    agree
  "aaab"                         no match       no match         agree    agree
  "aa"                           match 0..2     match end 2      agree    agree
  "xx aaaa"                      match 3..7     match end 7      agree    agree
pattern [a-z]+@[a-z]+\.[a-z]{2,}:
  automaton: 8 states, 4 byte classes
  SUBJECT                        PIKE VM        AUTOMATON        VERDICT  END
  "ana@corp.io"                  match 0..11    match end 11     agree    agree
  "ana@corp"                     no match       no match         agree    agree
  "ana@corp.i"                   no match       no match         agree    agree
  "write to ana@corp.io today"   match 9..20    match end 20     agree    agree
pattern (\d{4})-(\d{2})-(\d{2}):
  automaton: 12 states, 3 byte classes
  SUBJECT                        PIKE VM        AUTOMATON        VERDICT  END
  "2026-10-29"                   match 0..10    match end 10     agree    agree
  "2026-10"                      no match       no match         agree    agree
  "2026-10-2"                    no match       no match         agree    agree
  "on 2026-10-29 ok"             match 3..13    match end 13     agree    agree
pattern ^(?:[a-z]+ ){2,4}[a-z]+$:
  automaton: 11 states, 3 byte classes
  SUBJECT                        PIKE VM        AUTOMATON        VERDICT  END
  "one two three"                match 0..13    match end 13     agree    agree
  "one two"                      no match       no match         agree    agree
  "one two thr"                  match 0..11    match end 11     agree    agree
  "one two three four five"      match 0..23    match end 23     agree    agree
```

Source: [`crates/regie-core/examples/dfa_vs_nfa.rs`](examples/dfa_vs_nfa.rs)
