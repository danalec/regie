# Performance

The numbers below were measured on one machine with one version of the
compiler. They are a record of a run, not a promise about any other
machine. The benchmark suite is in `crates/regie-core/benches/engine.rs`,
and running it again is the only way to get a number that means anything
today.

```console
cargo bench -p regie-core --features alloc --bench engine
```

## The machine

| | |
|---|---|
| processor | AMD Ryzen 9 7950X3D, 16 cores |
| operating system | Windows 11 Pro |
| compiler | rustc 1.99.0, LLVM 23.1.1, `x86_64-pc-windows-msvc` |
| profile | `release`, with `lto = true` and `codegen-units = 1` |

## The engine

The input of the benchmarks is a haystack of 4,096 bytes of `a`, except
where the table says otherwise.

| benchmark | time | throughput |
|---|---|---|
| `literal/search` | 223 µs | 18 MB/s |
| `literal/hit` | 215 µs | 19 MB/s |
| `alternation` | 344 µs | 12 MB/s |
| `character_class` | 245 µs | 17 MB/s |
| `repetition` | 505 µs | 8 MB/s |
| `find` | 232 µs | 18 MB/s |
| `captures` (33 bytes) | 15.8 µs | - |
| `dfa/precompiled` | **8.5 µs** | **482 MB/s** |
| `compile/pattern` | 1.15 µs | - |
| `compile/specification` | 745 ns | - |

Two things are worth reading out of this table.

**The precompiled automaton is 26 times faster than the engine.** That is
the point of the artifact: a firmware target links a table and walks it,
and it never runs the Pike VM. 8.5 µs for 4 KiB is 482 MB/s, which is
enough to filter a telemetry stream on a microcontroller.

**Compiling is cheap.** A pattern compiles in about a microsecond and a
structured specification in three quarters of one. The engine is meant to
be used with a pattern that is compiled once and matched many times, but
nothing stops a caller from compiling per request.

## The comparison with `regex-automata`

`regex-automata` is the reference implementation of this family of
engines, and it is what `regie` should be measured against. The comparison
is part of the benchmark suite, so a regression is visible in the pull
request and not a year later.

Same haystack, same patterns, same machine.

| pattern | `regie` | `regex-automata` | ratio |
|---|---|---|---|
| `needle` | 226 µs | 4.61 µs | 49× |
| `^[a-z0-9._%+-]+@[a-z0-9.-]+\.[a-z]{2,}$` | 259 µs | 4.57 µs | 57× |
| `(alpha\|beta\|gamma\|delta)+` | 339 µs | 4.61 µs | 73× |
| `[a-zA-Z0-9_]+` | 244 µs | 16.1 ns\* | 15,000× |
| `a{2,8}b` | 510 µs | 4.71 µs | 108× |

\* The `character_class` row of `regex-automata` is measured on a
different pattern than the one above: its `meta::Regex` finds the literal
prefix of the haystack with a SIMD search and returns before it reaches
the automaton, so the 16 nanoseconds are the cost of the prefilter and not
the cost of the match. The comparable number is the 4.6 µs of the other
rows.

**`regie` is one to two orders of magnitude slower than `regex-automata`,
and it is not close.** That is the honest result, and it is worth being
precise about why, because the reasons are not all the same:

* `regex-automata` uses a lazy DFA with a one-pass DFA fallback and a
  literal prefilter, and it is written to be fast on a host, with
  prefilters that use SIMD. `regie` interprets a program, one byte at a
  time, with a thread list. The gap between an interpreter and a
  hand-optimised state machine is the gap between the two numbers.
* `regex-automata` can use the host's memory. `regie` cannot: the engine
  has no allocator on its default path, so the automaton is a fixed table
  of `u16` and the transition is an index, not a pointer chase.
* `regex-automata` is allowed to use `unsafe`. `regie` is
  `#![forbid(unsafe_code)]`, so the hot loops cannot use raw pointers,
  unchecked indexing or manual vectorisation.

None of these is a defect, and none of them is an accident. They are the
price of the properties the project exists for: no allocator, no `unsafe`,
a worst case that is known in advance, and a binary that behaves the same
on every target.

**The comparison that matters for the firmware path is the precompiled
automaton, and it is close.** 8.5 µs against 4.6 µs on the same haystack
is a factor of 1.8, on a machine where `regex-automata` can use SIMD
prefilters and `regie` can do neither.

## Where the time goes

The Pike VM spends its time in two places.

**The closure.** At every byte, the matcher computes the epsilon closure
of the thread list: it follows the jumps, the splits and the capture
markers. The closure is recursive, and its depth is bounded by the number
of instructions. This is the dominant cost of a pattern with many
alternations, which is why `alternation` and `repetition` are the slowest
of the benchmarks.

**The restart.** An unanchored search has to try a new match at every
position. The matcher appends the new start thread at the end of the
thread list, so the attempt that started earlier keeps its priority, and
the cost of a restart is one closure.

There are two ways out of this, and neither is planned for this release:
a literal prefilter of the kind `regex-automata` uses, and a lazy DFA that
keeps the state between the bytes. The second one is what
`regie-codec` already serializes, in a form that is not yet incremental.

## The size of the model

The intent model is measured separately, because it is a different kind of
number.

| | |
|---|---|
| the weight table | 61,189 bytes |
| the vocabulary | 1,419 features |
| the whole model, linked | 99,121 bytes |
| the budget of the project | 262,144 bytes |

The training is offline and takes a few seconds; the inference is integer
arithmetic over the table, and it is not on the path of the benchmark
above because it is not on the matching path at all.

## The size of an artifact

The artifact of the `^ana@corp\.com$` pattern, the one the
`precompiled_dfa` example links:

| | |
|---|---|
| the size of the artifact | 558 bytes |
| the states | 14 |
| the byte classes | 10 |

The format is described byte by byte in
[`regie-codec`](../crates/regie-codec). The artifact is much larger than
the automaton it holds, because the header is fixed-size: it reserves room
for the largest automaton the engine supports. A compressed format would
make a typical artifact an order of magnitude smaller; that is the first
item of [EVOLUTION.md](EVOLUTION.md).

## How to reproduce

```console
cargo bench -p regie-core --features alloc --bench engine -- --warm-up-time 1 --measurement-time 3
```

The comparison group needs `regex-automata`, which is a dev-dependency of
`regie-core` and is not part of the published crate.
