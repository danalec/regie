//! # regie-core
//!
//! A deterministic regular expression engine for embedded targets.
//!
//! `regie-core` compiles a small, safe pattern dialect into a Thompson NFA
//! and matches with a Pike VM (NFA simulation). That combination gives two
//! properties the project cares about most:
//!
//! * **No catastrophic backtracking.** The engine never backtracks, so the
//!   worst case is `O(m * n)` for a pattern of `m` instructions over an input
//!   of `n` bytes.
//! * **No allocator on the default path.** The crate is `#![no_std]` and
//!   allocation-free unless the `alloc` feature is enabled.
//!
//! ## Supported syntax
//!
//! | Construct | Example | Notes |
//! |---|---|---|
//! | literal | `abc` | bytes, not code points |
//! | class | `[a-z]`, `[^a-z]` | ASCII ranges, POSIX shorthands |
//! | alternation | `a\|b` | left branch preferred |
//! | group | `(...)` | capturing |
//! | non-capturing group | `(?:...)` | |
//! | quantifier | `*`, `+`, `?`, `{n}`, `{n,}`, `{n,m}` | greedy by default, `?` suffix for lazy |
//! | anchor | `^`, `$` | text anchors, line anchors in multi-line mode |
//! | escape | `\d`, `\w`, `\s`, `\n`, `\x41` | an unknown escape of a letter is refused |
//! | flags | `(?i)`, `(?i:...)` | `i`, `m`, `s` |
//!
//! Look-around, backreferences, recursion and conditionals are rejected
//! with [`RegexError::Unsupported`]: they are exactly the constructs that
//! make matching unpredictable.
//!
//! ## Example
//!
//! ```
//! use regie_core::Regex;
//!
//! let re = Regex::new(r"^\d{3}-\d{2}$").unwrap();
//! assert!(re.is_match(b"123-45"));
//! assert!(!re.is_match(b"123-456"));
//!
//! let re = Regex::new(r"(\w+)@(\w+)").unwrap();
//! let caps = re.captures(b"mail: ana@corp").unwrap();
//! assert_eq!(caps.get(1).unwrap().start, 6);
//! ```
//!
//! ## Structured intents
//!
//! The crate also compiles [`spec::CandidateSpec`] values directly, without
//! any pattern text in between. That is the path used by the intent model:
//! the model can only ever choose enumerated options, never write syntax.

#![cfg_attr(not(feature = "std"), no_std)]
#![doc(html_root_url = "https://docs.rs/regie-core/0.1.0")]
#![forbid(unsafe_code)]

#[cfg(feature = "alloc")]
extern crate alloc;

pub mod dfa;
pub mod error;
pub mod hir;
pub mod smallvec;
pub mod spec;

mod class;
#[cfg(feature = "alloc")]
mod dfa_build;
mod matcher;
mod parse;
mod program;
mod spec_compile;

pub use crate::dfa::Dfa;
pub use crate::error::RegexError;
pub use crate::smallvec::SmallVec;
pub use crate::spec::{CandidateSpec, IntentSpec};

use crate::class::ClassTable;
use crate::error::Result;
use crate::program::Program;

/// The longest pattern `regie-core` accepts, in bytes.
pub const MAX_PATTERN_LEN: usize = 512;

/// The largest number of capture groups a pattern may declare.
pub const MAX_CAPTURE_GROUPS: usize = 8;

/// The number of capture slots a compiled pattern tracks.
pub const MAX_CAPTURE_SLOTS: usize = MAX_CAPTURE_GROUPS * 2;

/// A byte range inside the input.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Match {
    /// The offset of the first byte of the match.
    pub start: usize,
    /// The offset one past the last byte of the match.
    pub end: usize,
}

impl Match {
    /// The length of the match, in bytes.
    pub const fn len(&self) -> usize {
        self.end - self.start
    }

    /// Whether the match is empty.
    pub const fn is_empty(&self) -> bool {
        self.end == self.start
    }
}

/// The groups captured by a successful match.
///
/// Group `0` is the whole match; groups `1..` are the capture groups in the
/// order they appear in the pattern.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Captures {
    groups: [Option<Match>; MAX_CAPTURE_GROUPS + 1],
    len: usize,
}

impl Captures {
    /// Returns the span of group `index`, or `None` when the group did not
    /// participate in the match.
    pub fn get(&self, index: usize) -> Option<Match> {
        if index < self.len {
            self.groups[index]
        } else {
            None
        }
    }

    /// The number of groups, including group `0`.
    pub const fn len(&self) -> usize {
        self.len
    }

    /// Whether the capture set is empty. Always `false` in practice: a
    /// successful match always has at least group `0`.
    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Iterates over the groups.
    pub fn iter(&self) -> impl Iterator<Item = Option<Match>> + '_ {
        self.groups[..self.len].iter().copied()
    }
}

/// The flags that influence how a pattern is compiled.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Flags {
    /// Fold ASCII letters so that `a` and `A` match each other.
    pub case_insensitive: bool,
    /// Treat `^` and `$` as line anchors instead of text anchors.
    pub multi_line: bool,
    /// Let `.` match a newline.
    pub dot_matches_newline: bool,
}

/// A compiled regular expression.
///
/// A `Regex` owns everything it needs: compiling never touches the
/// allocator and matching never mutates the value.
#[derive(Clone)]
pub struct Regex {
    pattern: [u8; MAX_PATTERN_LEN],
    pattern_len: u16,
    program: Program,
    classes: ClassTable,
    flags: Flags,
    group_count: u16,
    has_asserts: bool,
}

impl Regex {
    /// Compiles `pattern`.
    ///
    /// The pattern is interpreted as bytes: a class matches exactly one byte,
    /// never one code point. The `unicode` feature is reserved and changes
    /// nothing yet.
    pub fn new(pattern: &str) -> Result<Regex> {
        RegexBuilder::new(pattern).build()
    }

    /// Compiles a structured specification into an equivalent automaton.
    ///
    /// This is the path taken by the intent layer: no pattern text is
    /// involved, so a model cannot express anything outside the enumerated
    /// options of [`spec::CandidateSpec`].
    pub fn from_spec(spec: &CandidateSpec) -> Result<Regex> {
        let (program, classes, group_count, has_asserts) = program::compile_spec(spec)?;
        Ok(Regex {
            pattern: [0; MAX_PATTERN_LEN],
            pattern_len: 0,
            program,
            classes,
            flags: Flags::default(),
            group_count,
            has_asserts,
        })
    }

    /// Returns a builder for a pattern.
    pub fn builder() -> RegexBuilder {
        RegexBuilder::new("")
    }

    /// Whether the pattern matches anywhere in `haystack`.
    ///
    /// Runs in `O(m * n)` time in the worst case and uses no memory beyond
    /// the compiled program.
    pub fn is_match(&self, haystack: &[u8]) -> bool {
        matcher::is_match(self, haystack)
    }

    /// Returns the leftmost match in `haystack`.
    ///
    /// The match is the leftmost one; among matches starting at the same
    /// position, the branch preferred by the pattern wins.
    pub fn find(&self, haystack: &[u8]) -> Option<Match> {
        matcher::find(self, haystack)
    }

    /// Returns the leftmost match and its capture groups.
    pub fn captures(&self, haystack: &[u8]) -> Option<Captures> {
        matcher::captures(self, haystack)
    }

    /// The pattern this regex was compiled from.
    pub fn pattern(&self) -> &str {
        core::str::from_utf8(&self.pattern[..self.pattern_len as usize]).unwrap_or("")
    }

    /// The number of capture groups, excluding group `0`.
    pub const fn group_count(&self) -> usize {
        self.group_count as usize
    }

    /// The flags this regex was compiled with.
    pub const fn flags(&self) -> Flags {
        self.flags
    }

    /// Whether the pattern contains a zero-width assertion.
    pub const fn has_asserts(&self) -> bool {
        self.has_asserts
    }

    /// Builds a deterministic automaton for this pattern.
    ///
    /// The automaton matches with a single linear pass over the input and
    /// can be serialized by `regie-codec` for embedded use.
    #[cfg(feature = "alloc")]
    pub fn build_dfa(&self) -> Result<Dfa> {
        program::build_dfa(self)
    }
}

impl core::fmt::Debug for Regex {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Regex")
            .field("pattern", &self.pattern())
            .field("groups", &self.group_count)
            .field("flags", &self.flags)
            .finish()
    }
}

/// A builder for [`Regex`].
///
/// ```
/// use regie_core::Regex;
///
/// let re = Regex::builder()
///     .pattern("^hello$")
///     .case_insensitive(true)
///     .build()
///     .unwrap();
/// assert!(re.is_match(b"HELLO"));
/// ```
#[derive(Clone, Debug)]
pub struct RegexBuilder {
    pattern: [u8; MAX_PATTERN_LEN],
    pattern_len: u16,
    truncated: bool,
    flags: Flags,
}

impl RegexBuilder {
    /// Starts a builder for `pattern`.
    pub fn new(pattern: &str) -> RegexBuilder {
        let mut builder = RegexBuilder {
            pattern: [0; MAX_PATTERN_LEN],
            pattern_len: 0,
            truncated: false,
            flags: Flags::default(),
        };
        builder.set_pattern(pattern);
        builder
    }

    /// Sets the pattern to compile.
    pub fn pattern(mut self, pattern: &str) -> RegexBuilder {
        self.set_pattern(pattern);
        self
    }

    /// Enables or disables case-insensitive matching.
    pub fn case_insensitive(mut self, yes: bool) -> RegexBuilder {
        self.flags.case_insensitive = yes;
        self
    }

    /// Enables or disables multi-line anchors.
    pub fn multi_line(mut self, yes: bool) -> RegexBuilder {
        self.flags.multi_line = yes;
        self
    }

    /// Enables or disables matching newlines with `.`.
    pub fn dot_matches_newline(mut self, yes: bool) -> RegexBuilder {
        self.flags.dot_matches_newline = yes;
        self
    }

    /// Compiles the pattern.
    pub fn build(self) -> Result<Regex> {
        if self.truncated {
            return Err(RegexError::PatternTooLong {
                limit: MAX_PATTERN_LEN,
                actual: self.pattern_len as usize + 1,
            });
        }
        let pattern =
            core::str::from_utf8(&self.pattern[..self.pattern_len as usize]).map_err(|_| {
                RegexError::Syntax {
                    position: 0,
                    message: "pattern is not valid UTF-8",
                }
            })?;
        let (arena, mut classes, root) = parse::parse(pattern, self.flags)?;
        let (program, group_count, has_asserts) =
            program::compile(&arena, root, &mut classes, self.flags)?;
        Ok(Regex {
            pattern: self.pattern,
            pattern_len: self.pattern_len,
            program,
            classes,
            flags: self.flags,
            group_count,
            has_asserts,
        })
    }

    fn set_pattern(&mut self, pattern: &str) {
        let bytes = pattern.as_bytes();
        let len = bytes.len().min(MAX_PATTERN_LEN);
        self.pattern = [0; MAX_PATTERN_LEN];
        self.pattern[..len].copy_from_slice(&bytes[..len]);
        self.pattern_len = len as u16;
        self.truncated = bytes.len() > MAX_PATTERN_LEN;
    }
}
