# regie-codec

Serialization of compiled `regie-core` automata for embedded firmware.

A pattern is compiled once, on a workstation; the resulting automaton is
serialized into a compact binary artifact and linked into the firmware image
with `include_bytes!`. At boot the target decodes the artifact and matches
against it — nothing is compiled at runtime.

The crate is `no_std`, allocation free by default, uses no `unsafe` and has no
dependencies beyond `regie-core`.

```rust
use regie_core::dfa::Dfa;
use regie_codec::{decode, encode, encoded_len};

let mut byte_class = [0u8; 256];
byte_class[b'a' as usize] = 1;
let dfa = Dfa::from_parts(2, 3, &byte_class, &[0b0000_0100], &[0, 0, 0, 2, 0, 0]).unwrap();

let mut artifact = vec![0u8; encoded_len(&dfa)];
let written = encode(&dfa, &mut artifact).unwrap();
let restored = decode(&artifact).unwrap();

assert_eq!(written, artifact.len());
assert!(restored.is_match(b"a"));
```

## The `RGDF` format

The crate documentation is the canonical reference; this is the short version.

An artifact is a fixed 16-byte header, a payload, and a 4-byte checksum. Every
multi-byte integer is **little-endian**, on every host.

| Offset | Size | Field | Meaning |
|---|---|---|---|
| `0` | 4 | magic | Always `b"RGDF"`. |
| `4` | 2 | version | `u16`, currently `1`. |
| `6` | 1 | endianness | `0x01` means little-endian. |
| `7` | 1 | flags | Bit 0 is reserved (`has_asserts`). Bit 1 is `accept_at_end`: the automaton only accepts a match at the end of the input, which is how a `$` anchor survives the round trip. Bits 2-7 are reserved. |
| `8` | 2 | class_count | Number of byte equivalence classes. |
| `10` | 2 | state_count | Number of states, dead and start included. |
| `12` | 4 | payload_len | Length of the payload section. |
| `16` | 256 | byte_class | Byte-to-class table, one class per byte value. |
| `272` | `ceil(state_count / 8)` | accept_bitmap | One bit per state, least significant bit first. |
| `272 + bitmap` | `2 * state_count * class_count` | transitions | Target states, `u16` each, row-major over `(state, class)`. |
| end | 4 | checksum | CRC-32 (IEEE) of every preceding byte. |

Total length: `20 + 256 + ceil(state_count / 8) + 2 * state_count * class_count`
bytes, which is exactly what `encoded_len` reports and exactly what `encode`
writes. There is no padding.

The checksum is CRC-32 (IEEE 802.3): reflected polynomial `0xEDB88320`, initial
value `0xFFFFFFFF`, final XOR `0xFFFFFFFF` — the function `zlib` calls
`crc32`. It covers the header as well as the payload.

## Properties

* **Deterministic** — the same automaton always encodes to the same bytes.
* **Strict** — a truncated artifact, a trailing byte, a wrong magic, an
  unknown version, an unknown flag bit, an automaton larger than `regie-core`
  supports and a payload with a wrong checksum are all reported as explicit
  errors. No malformed input can panic.
* **Exact** — a decoded automaton has the same states, classes, byte table,
  accept bitmap, transitions and end anchor as the original, and matches
  identically.

## Features

* `alloc` — adds `encode_to_vec`, which returns an owned buffer.
* `std` — implements `std::error::Error` for `CodecError`. Implies `alloc`.

`decode` needs roughly 33 KiB of stack for the largest automaton this build
supports (the transition buffer plus the automaton itself).
