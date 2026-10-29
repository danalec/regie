//! Byte sets and byte equivalence classes.
//!
//! A pattern rarely distinguishes every byte value. `regie-core` therefore
//! collects the byte sets a pattern mentions (`[a-z]`, `\d`, a literal `x`,
//! ...) into a small table, and records for every byte which of those sets
//! contain it. The membership mask doubles as the *signature* used to build
//! the equivalence classes of the deterministic automaton: two bytes behave
//! identically in the automaton exactly when they belong to the same sets.

use crate::error::{RegexError, Result};

/// The maximum number of distinct byte sets a pattern may mention.
pub const MAX_CLASS_SETS: usize = 24;

/// A set of byte values.
pub type ByteSet = [u64; 4];

/// An empty byte set.
pub const EMPTY_SET: ByteSet = [0; 4];

/// Adds `byte` to `set`.
pub fn set_add(set: &mut ByteSet, byte: u8) {
    let index = byte as usize;
    set[index / 64] |= 1u64 << (index % 64);
}

/// Whether `byte` belongs to `set`.
pub fn set_contains(set: &ByteSet, byte: u8) -> bool {
    let index = byte as usize;
    set[index / 64] & (1u64 << (index % 64)) != 0
}

/// Adds every byte of `range` to `set`.
pub fn set_add_range(set: &mut ByteSet, start: u8, end: u8) {
    let mut byte = start as u16;
    let end = end as u16;
    while byte <= end {
        set_add(set, byte as u8);
        byte += 1;
    }
}

/// Negates `set` in place.
pub fn set_negate(set: &mut ByteSet) {
    for word in set.iter_mut() {
        *word = !*word;
    }
}

/// The table of byte sets used by a pattern.
#[derive(Clone, Copy)]
pub(crate) struct ClassTable {
    sets: [ByteSet; MAX_CLASS_SETS],
    membership: [u32; 256],
    count: u16,
}

impl ClassTable {
    /// Creates an empty table.
    pub fn new() -> Self {
        Self {
            sets: [EMPTY_SET; MAX_CLASS_SETS],
            membership: [0; 256],
            count: 0,
        }
    }

    /// Registers a byte set, reusing an identical one when possible.
    pub fn add(&mut self, set: &ByteSet) -> Result<u16> {
        for (index, existing) in self.sets.iter().take(self.count as usize).enumerate() {
            if *existing == *set {
                return Ok(index as u16);
            }
        }
        if self.count as usize >= MAX_CLASS_SETS {
            return Err(RegexError::TooManyClasses {
                limit: MAX_CLASS_SETS,
            });
        }
        let id = self.count;
        self.sets[id as usize] = *set;
        self.count += 1;
        let mask = 1u32 << id;
        for (byte, slot) in self.membership.iter_mut().enumerate() {
            if set_contains(set, byte as u8) {
                *slot |= mask;
            }
        }
        Ok(id)
    }

    /// Whether `byte` belongs to the set `id`.
    pub fn contains(&self, id: u16, byte: u8) -> bool {
        if id >= self.count {
            return false;
        }
        self.membership[byte as usize] & (1u32 << id) != 0
    }

    /// The membership signature of `byte`: one bit per registered set.
    ///
    /// Only the determinizer needs this, so it is part of the `alloc` build.
    #[cfg(feature = "alloc")]
    pub fn signature(&self, byte: u8) -> u32 {
        self.membership[byte as usize]
    }

    /// Builds the byte equivalence classes induced by the registered sets.
    ///
    /// Returns the number of classes and, for every byte, the class it
    /// belongs to. Two bytes share a class exactly when they belong to the
    /// same sets, which is precisely when the automaton cannot tell them
    /// apart.
    ///
    /// Only the determinizer needs this, so it is part of the `alloc` build.
    #[cfg(feature = "alloc")]
    pub fn partition(&self) -> (u16, [u8; 256]) {
        let mut class_of = [0u8; 256];
        let mut seen: [Option<u8>; 32] = [None; 32];
        let mut count = 0u8;
        for (byte, slot) in class_of.iter_mut().enumerate() {
            let signature = self.signature(byte as u8);
            // The signature uses at most `MAX_CLASS_SETS` bits, so it never
            // fits in the 32-entry lookup; fall back to a linear scan when
            // the table is full.
            let mut found = None;
            for (index, existing) in seen.iter().enumerate().take(count as usize) {
                if let Some(existing) = existing {
                    if self.signature(*existing) == signature {
                        found = Some(index as u8);
                        break;
                    }
                }
            }
            match found {
                Some(class) => *slot = class,
                None => {
                    if count as usize >= seen.len() {
                        // Cannot happen with the current limits, but keep the
                        // function total.
                        *slot = 0;
                        continue;
                    }
                    seen[count as usize] = Some(byte as u8);
                    *slot = count;
                    count += 1;
                }
            }
        }
        (count as u16, class_of)
    }
}

impl Default for ClassTable {
    fn default() -> Self {
        Self::new()
    }
}
