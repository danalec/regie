//! The discriminator: compile a candidate and check it against the examples.
//!
//! The discriminator is the only component that talks to the engine.  It
//! compiles a candidate with the deterministic compiler of `regie-core` and
//! counts how many positive examples the candidate matches and how many
//! negative examples it rejects.
//!
//! A candidate that does not compile is rejected, not reported as an error:
//! the search treats a broken candidate exactly like a candidate that
//! contradicts the examples.

use crate::Examples;
use regie_core::spec::CandidateSpec;
use regie_core::Regex;

/// The points awarded for every positive example a candidate matches.
const POSITIVE_WEIGHT: u32 = 1000;

/// The points awarded for every negative example a candidate rejects.
const NEGATIVE_WEIGHT: u32 = 100;

/// What the verification of one candidate found.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub(crate) struct Verdict {
    /// Whether the candidate compiled at all.
    pub compiled: bool,
    /// The number of positive examples the candidate matched.
    pub positives_matched: u16,
    /// The number of negative examples the candidate rejected.
    pub negatives_rejected: u16,
    /// The score of the candidate.
    pub score: u32,
}

impl Verdict {
    /// Whether the candidate satisfies every example.
    ///
    /// A candidate is consistent when it compiled, matched every positive
    /// example and rejected every negative example.
    pub fn is_consistent(&self, examples: Examples<'_>) -> bool {
        self.compiled
            && self.positives_matched == count(examples.positive.len())
            && self.negatives_rejected == count(examples.negative.len())
    }
}

/// Verifies one candidate against the examples.
pub(crate) fn verify(spec: &CandidateSpec, examples: Examples<'_>) -> Verdict {
    let regex = match Regex::from_spec(spec) {
        Ok(regex) => regex,
        // A candidate that does not compile is rejected, never an error.
        Err(_) => return Verdict::default(),
    };

    let mut positives_matched = 0u16;
    for example in examples.positive {
        if regex.is_match(example.as_bytes()) {
            positives_matched = positives_matched.saturating_add(1);
        }
    }

    let mut negatives_matched = 0u16;
    for example in examples.negative {
        if regex.is_match(example.as_bytes()) {
            negatives_matched = negatives_matched.saturating_add(1);
        }
    }
    let negatives_rejected = count(examples.negative.len()).saturating_sub(negatives_matched);

    let score =
        POSITIVE_WEIGHT * positives_matched as u32 + NEGATIVE_WEIGHT * negatives_rejected as u32;

    Verdict {
        compiled: true,
        positives_matched,
        negatives_rejected,
        score,
    }
}

/// The number of examples, clamped to what a `u16` counter can hold.
fn count(len: usize) -> u16 {
    if len > u16::MAX as usize {
        u16::MAX
    } else {
        len as u16
    }
}
