# regie-codec examples

`regie-codec` serializes a compiled automaton into the `RGDF` artifact format:
a 16-byte header, the byte-class table, the accept bitmap, the transition
table and a CRC-32, all little-endian. A pattern is compiled once, on a
workstation, and the artifact is linked into the firmware image; at boot the
target decodes the blob and matches against it without compiling anything.

Every example below is a program in `crates/regie-codec/examples/`. Run one
with:

```console
$ cargo run -p regie-codec --example <name>
```

Every run is deterministic: the same automaton always encodes to the same
bytes, and no example prints a timestamp, a path or anything else that
changes between runs.

## `artifact_layout`

What it shows: the byte layout of the format, field by field, read back out of a real artifact.

Run it:

```console
$ cargo run -p regie-codec --example artifact_layout
artifact: 289 bytes, 3 states, 2 byte classes

hex dump
  0000  52 47 44 46 01 00 01 00 02 00 03 00 0d 01 00 00
  0010  00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
  0020  00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
  0030  00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
  0040  00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
  0050  00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
  0060  00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
  0070  00 01 01 00 00 00 00 00 00 00 00 00 00 00 00 00
  0080  00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
  0090  00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
  00a0  00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
  00b0  00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
  00c0  00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
  00d0  00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
  00e0  00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
  00f0  00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
  0100  00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
  0110  04 00 00 00 00 00 00 02 00 00 00 00 00 49 d2 18
  0120  76

every field, read out of the bytes above
  magic         [  0..4   ]52 47 44 46               "RGDF"
  version       [  4..6   ]01 00                     1
  endianness    [  6..7   ]01                        little-endian (0x01)
  flags         [  7..8   ]00                        no flag set
  class_count   [  8..10  ]02 00                     2
  state_count   [ 10..12  ]03 00                     3
  payload_len   [ 12..16  ]0d 01 00 00               269 bytes
  byte_class    [ 16..272 ]256 bytes                 class per byte value
  accept_bitmap [272..273 ]04                        state 2 accepts
  transitions   [273..285 ]12 bytes                  3 states x 2 classes
  checksum      [285..289 ]49 d2 18 76               0x7618d249

the transition table, row major over (state, class)
  state 0:  0  0
  state 1:  0  2
  state 2:  0  0

checksum
  stored    [285..289]  49 d2 18 76  0x7618d249
  computed  over bytes [0..285]  0x7618d249
  the two agree: true

the same automaton with the end anchor set
  flags [7..8] 02 accept_at_end
  0x01 is reserved, 0x02 accepts at the end only

decode agrees: 3 states
```

Source: [`crates/regie-codec/examples/artifact_layout.rs`](examples/artifact_layout.rs)

## `emit_artifact`

What it shows: how a firmware build writes a compiled automaton to a file that it can link into the image.

Run it:

```console
$ cargo run -p regie-codec --features alloc --example emit_artifact
wrote 558 bytes to crates/regie-codec/examples/assets/ana_at_corp.rdf (14 states, 10 byte classes)
```

Source: [`crates/regie-codec/examples/emit_artifact.rs`](examples/emit_artifact.rs)

## `precompiled_dfa`

What it shows: matching against an automaton that was compiled ahead of time, with nothing compiled at boot.

Run it:

```console
$ cargo run -p regie-codec --example precompiled_dfa
artifact: 558 bytes, 14 states, 10 byte classes
accepts at the end only: true
  ana@corp.com             -> true
  mail ana@corp.com here   -> false
  ana@other.com            -> false
  ana@corp.com.br          -> false
  nobody here              -> false
```

Source: [`crates/regie-codec/examples/precompiled_dfa.rs`](examples/precompiled_dfa.rs)

## `round_trip`

What it shows: that an artifact decodes back into the same automaton, and what the decoder reports for a corrupt one.

Run it:

```console
$ cargo run -p regie-codec --example round_trip
artifact: 289 bytes, 3 states, 2 byte classes

the decoded automaton has the same parts as the original
  states 3 == 3
  classes 2 == 2
  byte table    true
  accept bitmap true
  transitions   true
  end anchor    true

both automata answer the same way
  input       original   restored
  ""          false      false
  "a"         true       true
  "b"         true       true
  "ab"        true       true
  "c"         false      false

corrupting the artifact
  a flipped magic byte (289 bytes)
    BadMagic { found: [83, 71, 68, 70] }
    bad magic: expected `RGDF`, found 53474446
  a truncated blob (288 bytes)
    Truncated { needed: 289, actual: 288 }
    artifact is truncated: 288 bytes available, 289 needed
  a flipped payload byte (289 bytes)
    BadChecksum { expected: 3175350764, found: 1981338185 }
    bad checksum: computed 0xbd4401ec, artifact says 0x7618d249
  a corrupted checksum byte (289 bytes)
    BadChecksum { expected: 1981338185, found: 1998115401 }
    bad checksum: computed 0x7618d249, artifact says 0x7718d249
```

Source: [`crates/regie-codec/examples/round_trip.rs`](examples/round_trip.rs)
