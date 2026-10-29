//! A compact, fixed-capacity deterministic finite automaton.
//!
//! The [`Dfa`] type is the artifact that `regie-core` produces and
//! `regie-codec` serializes. It has a fixed memory footprint, needs no
//! allocator, and can be embedded in firmware with `include_bytes!`.
//!
//! State `0` is the dead state; state `1` is the start state. Transitions
//! are indexed by *byte class* rather than by raw byte, which keeps the table
//! small: a pattern that only distinguishes a handful of byte sets needs only
//! a handful of columns.

use crate::error::{RegexError, Result};

/// The maximum number of byte equivalence classes a pattern may use.
pub const MAX_CLASSES: usize = 32;

/// The maximum number of states a determinized automaton may have.
pub const MAX_DFA_STATES: usize = 256;

/// The maximum number of transitions a determinized automaton may have.
pub const MAX_DFA_TRANSITIONS: usize = MAX_DFA_STATES * MAX_CLASSES;

/// The dead state of every automaton.
pub const DEAD_STATE: u16 = 0;

/// The start state of every automaton.
pub const START_STATE: u16 = 1;

/// A deterministic automaton over byte equivalence classes.
#[derive(Clone, Copy)]
pub struct Dfa {
    class_count: u16,
    state_count: u16,
    accept_at_end: bool,
    byte_class: [u8; 256],
    accept: [u8; MAX_DFA_STATES / 8],
    trans: [u16; MAX_DFA_TRANSITIONS],
}

impl Dfa {
    /// Creates an empty automaton that matches nothing.
    pub fn new() -> Self {
        Self {
            class_count: 1,
            state_count: 2,
            accept_at_end: false,
            byte_class: [0; 256],
            accept: [0; MAX_DFA_STATES / 8],
            trans: [DEAD_STATE; MAX_DFA_TRANSITIONS],
        }
    }

    /// Whether a match is only accepted at the end of the input.
    ///
    /// This is how a `$` anchor is represented in a deterministic automaton.
    pub fn accepts_at_end_only(&self) -> bool {
        self.accept_at_end
    }

    /// Sets whether the automaton only accepts a match at the end of the input.
    ///
    /// This is how a `$` anchor is represented in a deterministic automaton.
    /// It has to be restored explicitly when an automaton is rebuilt from its
    /// raw parts - see [`Dfa::from_parts`] - because the fixed layout of
    /// [`Dfa`] has no room for per-state information.
    pub fn set_accept_at_end(&mut self, value: bool) {
        self.accept_at_end = value;
    }

    /// The number of states, including the dead and start states.
    pub fn state_count(&self) -> usize {
        self.state_count as usize
    }

    /// The number of byte equivalence classes.
    pub fn class_count(&self) -> usize {
        self.class_count as usize
    }

    /// The class of every byte value.
    pub fn byte_classes(&self) -> &[u8; 256] {
        &self.byte_class
    }

    /// The raw transition table, row-major over `(state, class)`.
    pub fn transitions(&self) -> &[u16] {
        &self.trans[..self.transition_len()]
    }

    /// The raw bitmap of accepting states.
    pub fn accept_bitmap(&self) -> &[u8] {
        &self.accept[..(self.state_count as usize).div_ceil(8)]
    }

    /// Whether `state` accepts a match.
    pub fn is_accept(&self, state: u16) -> bool {
        let index = state as usize;
        if index >= self.state_count as usize {
            return false;
        }
        self.accept[index / 8] & (1u8 << (index % 8)) != 0
    }

    /// Rebuilds an automaton from its raw parts.
    ///
    /// This is the entry point used when decoding a serialized artifact: the
    /// parts are exactly what [`Dfa::byte_classes`],
    /// [`Dfa::accept_bitmap`] and [`Dfa::transitions`] report.
    ///
    /// Returns [`RegexError::DfaTooLarge`] or [`RegexError::TooManyClasses`]
    /// when the parts do not fit the fixed capacity of this build.
    pub fn from_parts(
        class_count: u16,
        state_count: u16,
        byte_class: &[u8; 256],
        accept_bitmap: &[u8],
        transitions: &[u16],
    ) -> Result<Dfa> {
        Dfa::check_capacity(state_count as usize, class_count as usize)?;
        let states = state_count as usize;
        if accept_bitmap.len() < states.div_ceil(8) {
            return Err(RegexError::Syntax {
                position: 0,
                message: "accept bitmap is truncated",
            });
        }
        if transitions.len() < states * class_count as usize {
            return Err(RegexError::Syntax {
                position: 0,
                message: "transition table is truncated",
            });
        }
        let mut dfa = Dfa::new();
        dfa.set_class_count(class_count);
        dfa.set_state_count(state_count);
        for (byte, class) in byte_class.iter().enumerate() {
            dfa.set_byte_class(byte as u8, *class);
        }
        for state in 0..states {
            dfa.set_accept(
                state as u16,
                accept_bitmap[state / 8] & (1u8 << (state % 8)) != 0,
            );
        }
        for state in 0..states {
            for class in 0..class_count as usize {
                let target = transitions[state * class_count as usize + class];
                dfa.set_transition(state as u16, class as u8, target);
            }
        }
        Ok(dfa)
    }

    /// The state reached from `state` when reading a byte of class `class`.
    pub fn transition(&self, state: u16, class: u8) -> u16 {
        if state as usize >= self.state_count as usize {
            return DEAD_STATE;
        }
        let class = class as usize;
        if class >= self.class_count as usize {
            return DEAD_STATE;
        }
        self.trans[state as usize * self.class_count as usize + class]
    }

    /// The class of `byte`.
    pub fn class_of(&self, byte: u8) -> u8 {
        self.byte_class[byte as usize]
    }

    /// Whether the automaton finds a match anywhere in `haystack`.
    ///
    /// This is a single left-to-right pass: the automaton is built with
    /// unanchored semantics, so it may restart at any position. The run is
    /// `O(len(haystack))` and uses no memory beyond the automaton itself.
    pub fn is_match(&self, haystack: &[u8]) -> bool {
        let mut state = START_STATE;
        for &byte in haystack {
            state = self.transition(state, self.class_of(byte));
            if state == DEAD_STATE {
                return false;
            }
            if !self.accept_at_end && self.is_accept(state) {
                return true;
            }
        }
        self.is_accept(state)
    }

    /// The end offset of the earliest match in `haystack`, if any.
    ///
    /// The start offset is not reported: an unanchored automaton does not
    /// carry it. Use [`crate::Regex::find`] when the start offset matters.
    pub fn find_end(&self, haystack: &[u8]) -> Option<usize> {
        let mut state = START_STATE;
        for (offset, &byte) in haystack.iter().enumerate() {
            state = self.transition(state, self.class_of(byte));
            if state == DEAD_STATE {
                return None;
            }
            if !self.accept_at_end && self.is_accept(state) {
                return Some(offset + 1);
            }
        }
        if self.is_accept(state) {
            Some(haystack.len())
        } else {
            None
        }
    }

    pub(crate) fn transition_len(&self) -> usize {
        self.state_count as usize * self.class_count as usize
    }

    pub(crate) fn set_class_count(&mut self, count: u16) {
        self.class_count = count.max(1);
    }

    pub(crate) fn set_state_count(&mut self, count: u16) {
        self.state_count = count;
    }

    pub(crate) fn set_byte_class(&mut self, byte: u8, class: u8) {
        self.byte_class[byte as usize] = class;
    }

    pub(crate) fn set_accept(&mut self, state: u16, value: bool) {
        let index = state as usize;
        if index >= MAX_DFA_STATES {
            return;
        }
        if value {
            self.accept[index / 8] |= 1u8 << (index % 8);
        } else {
            self.accept[index / 8] &= !(1u8 << (index % 8));
        }
    }

    pub(crate) fn set_transition(&mut self, state: u16, class: u8, target: u16) {
        let index = state as usize * self.class_count as usize + class as usize;
        if index < MAX_DFA_TRANSITIONS {
            self.trans[index] = target;
        }
    }

    pub(crate) fn check_capacity(states: usize, classes: usize) -> Result<()> {
        if states > MAX_DFA_STATES {
            return Err(RegexError::DfaTooLarge {
                limit: MAX_DFA_STATES,
            });
        }
        if classes > MAX_CLASSES {
            return Err(RegexError::TooManyClasses { limit: MAX_CLASSES });
        }
        Ok(())
    }
}

impl Default for Dfa {
    fn default() -> Self {
        Self::new()
    }
}

impl core::fmt::Debug for Dfa {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Dfa")
            .field("states", &self.state_count)
            .field("classes", &self.class_count)
            .finish()
    }
}
