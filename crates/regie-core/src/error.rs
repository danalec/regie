//! Explicit error types for pattern compilation and matching.
//!
//! Every failure mode of `regie-core` is represented by a variant of
//! [`RegexError`]. The crate never panics on user input: malformed patterns,
//! oversized patterns and budget exhaustion are all reported as values.

use core::fmt;

/// The result type used across `regie-core`.
pub type Result<T> = core::result::Result<T, RegexError>;

/// Reasons a pattern could not be compiled, or a match could not be produced.
///
/// The variants are deliberately explicit: an embedded caller can react to a
/// [`RegexError::Syntax`] differently than to a [`RegexError::ProgramTooLarge`]
/// without parsing a message.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[non_exhaustive]
pub enum RegexError {
    /// The pattern is longer than the configured limit.
    PatternTooLong {
        /// The limit that was exceeded.
        limit: usize,
        /// The length of the offending pattern.
        actual: usize,
    },
    /// The pattern is not syntactically valid.
    Syntax {
        /// Byte offset into the pattern where the problem was detected.
        position: usize,
        /// A short, static description of the problem.
        message: &'static str,
    },
    /// The pattern uses a construct that this engine deliberately does not
    /// support (look-around, backreferences, recursion, ...).
    Unsupported {
        /// Byte offset into the pattern where the construct starts.
        position: usize,
        /// The name of the unsupported construct.
        feature: &'static str,
    },
    /// The compiled program exceeded the instruction budget of this build.
    ProgramTooLarge {
        /// The instruction budget that was exceeded.
        limit: usize,
    },
    /// The pattern declares more capture groups than this build supports.
    TooManyGroups {
        /// The maximum number of capture groups supported.
        limit: usize,
    },
    /// A repetition bound (`{n,m}`) is larger than this build supports.
    RepetitionTooLarge {
        /// The largest repetition bound supported.
        limit: usize,
    },
    /// The pattern nests groups more deeply than this build supports.
    NestingTooDeep {
        /// The maximum nesting depth supported.
        limit: usize,
    },
    /// A character class is malformed (for example an inverted range).
    InvalidClass {
        /// Byte offset into the pattern where the class starts.
        position: usize,
        /// A short, static description of the problem.
        message: &'static str,
    },
    /// The determinized automaton does not fit in the fixed capacity of this
    /// build. The pattern is still usable through the NFA engine.
    DfaTooLarge {
        /// The state budget that was exceeded.
        limit: usize,
    },
    /// The pattern requires more byte equivalence classes than this build
    /// supports.
    TooManyClasses {
        /// The maximum number of byte equivalence classes supported.
        limit: usize,
    },
    /// The matcher exhausted its thread budget before reaching a verdict.
    ThreadLimitExceeded {
        /// The thread budget that was exhausted.
        limit: usize,
    },
}

impl fmt::Display for RegexError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RegexError::PatternTooLong { limit, actual } => {
                write!(f, "pattern is too long: {actual} bytes, limit is {limit}")
            }
            RegexError::Syntax { position, message } => {
                write!(f, "syntax error at byte {position}: {message}")
            }
            RegexError::Unsupported { position, feature } => {
                write!(f, "unsupported construct `{feature}` at byte {position}")
            }
            RegexError::ProgramTooLarge { limit } => {
                write!(f, "compiled program exceeds {limit} instructions")
            }
            RegexError::TooManyGroups { limit } => {
                write!(f, "pattern uses more than {limit} capture groups")
            }
            RegexError::RepetitionTooLarge { limit } => {
                write!(f, "repetition bound exceeds {limit}")
            }
            RegexError::NestingTooDeep { limit } => {
                write!(f, "pattern nests groups deeper than {limit}")
            }
            RegexError::InvalidClass { position, message } => {
                write!(f, "invalid character class at byte {position}: {message}")
            }
            RegexError::DfaTooLarge { limit } => {
                write!(f, "determinized automaton exceeds {limit} states")
            }
            RegexError::TooManyClasses { limit } => {
                write!(
                    f,
                    "pattern needs more than {limit} byte equivalence classes"
                )
            }
            RegexError::ThreadLimitExceeded { limit } => {
                write!(f, "matcher exceeded its budget of {limit} threads")
            }
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for RegexError {}
