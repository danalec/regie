//! Candidate generation and mutation.
//!
//! The search starts from the candidates the intent model proposes and
//! mutates them along the axes the design defines: the strictness and the
//! character repertoire of a validation, the boundary and the capture policy
//! of an extraction, the action of a filter and the anchoring of a custom
//! pattern.
//!
//! The result is a small, deterministic proposal set.  Every candidate
//! appears exactly once and every candidate remembers its parent, so the set
//! can be walked flatly or as a tree.

use regie_core::spec::{
    BoundaryPolicy, CandidateSpec, CapturePolicy, CharsetPolicy, FilterAction, PatternParams,
    PatternTemplate, Strictness, TemplatePiece,
};
use regie_core::SmallVec;

/// The maximum number of candidates a single search explores.
///
/// The budget bounds the work of one search: the proposal set is filled in
/// breadth-first order and stops as soon as it holds this many candidates.
pub const MAX_EXPLORED: usize = 64;

/// The number of mutation steps taken from a seed candidate.
///
/// A candidate may be mutated at most twice, so the proposal set holds the
/// seeds, their mutations and the mutations of those mutations.
pub const MAX_DEPTH: usize = 2;

/// The parent of a seed candidate, which has none.
pub(crate) const NO_PARENT: u16 = u16::MAX;

/// The strictness values, in a stable order.
const STRICTNESSES: [Strictness; 3] = [
    Strictness::Strict,
    Strictness::Standard,
    Strictness::Lenient,
];

/// The character repertoire policies, in a stable order.
const CHARSETS: [CharsetPolicy; 6] = [
    CharsetPolicy::Digits,
    CharsetPolicy::Letters,
    CharsetPolicy::Alphanumeric,
    CharsetPolicy::Word,
    CharsetPolicy::PrintableAscii,
    CharsetPolicy::Any,
];

/// The boundary policies, in a stable order.
const BOUNDARIES: [BoundaryPolicy; 3] = [
    BoundaryPolicy::Greedy,
    BoundaryPolicy::Lazy,
    BoundaryPolicy::Exclusive,
];

/// The capture policies, in a stable order.
const CAPTURES: [CapturePolicy; 2] = [CapturePolicy::Whole, CapturePolicy::Group];

/// The filter actions, in a stable order.
const ACTIONS: [FilterAction; 4] = [
    FilterAction::Block,
    FilterAction::Allow,
    FilterAction::Highlight,
    FilterAction::Count,
];

/// The anchoring combinations, in a stable order.
const ANCHORINGS: [(bool, bool); 4] = [(false, false), (true, false), (false, true), (true, true)];

/// The proposals of one search.
///
/// The set holds the candidates in a canonical, breadth-first order.  The
/// position of a candidate in that order is its index, and the index is what
/// breaks the ties of the scoring: it depends on the proposal alone, never on
/// the order in which the candidates happen to be explored.
pub(crate) struct ProposalSet {
    specs: SmallVec<CandidateSpec, MAX_EXPLORED>,
    parents: SmallVec<u16, MAX_EXPLORED>,
}

impl ProposalSet {
    /// Creates an empty proposal set.
    fn new() -> ProposalSet {
        ProposalSet {
            specs: SmallVec::new(),
            parents: SmallVec::new(),
        }
    }

    /// The number of proposals.
    pub fn len(&self) -> usize {
        self.specs.len()
    }

    /// The candidate at `index`.
    pub fn spec(&self, index: usize) -> CandidateSpec {
        self.specs.get(index).copied().unwrap_or_default()
    }

    /// The parent of the candidate at `index`.
    #[cfg(feature = "mcts")]
    pub fn parent(&self, index: usize) -> u16 {
        self.parents.get(index).copied().unwrap_or(NO_PARENT)
    }

    /// Whether `spec` is already part of the set.
    fn contains(&self, spec: CandidateSpec) -> bool {
        self.specs.iter().any(|known| *known == spec)
    }

    /// Adds a candidate to the set, unless it is already there.
    ///
    /// Returns whether the candidate was added.
    fn insert(&mut self, spec: CandidateSpec, parent: u16) -> bool {
        if self.len() >= MAX_EXPLORED || self.contains(spec) {
            return false;
        }
        self.specs.push(spec);
        self.parents.push(parent);
        true
    }
}

/// Builds the proposal set of one search.
///
/// The seeds come first, in the order the intent model ranked them.  They
/// are followed by their mutations and by the mutations of those mutations,
/// in breadth-first order.  The set never holds more than [`MAX_EXPLORED`]
/// candidates and never holds the same candidate twice.
pub(crate) fn build(seeds: &[CandidateSpec]) -> ProposalSet {
    let mut set = ProposalSet::new();
    for seed in seeds {
        set.insert(*seed, NO_PARENT);
    }

    let mut level_start = 0usize;
    let mut level_end = set.len();
    for _ in 0..MAX_DEPTH {
        if set.len() >= MAX_EXPLORED {
            break;
        }
        let mut index = level_start;
        while index < level_end && set.len() < MAX_EXPLORED {
            let spec = set.spec(index);
            let parent = index as u16;
            let alternatives = mutations(&spec);
            for alternative in alternatives.iter() {
                if set.len() >= MAX_EXPLORED {
                    break;
                }
                set.insert(*alternative, parent);
            }
            index += 1;
        }
        level_start = level_end;
        level_end = set.len();
    }
    set
}

/// The alternatives of one candidate, one axis at a time.
///
/// The alternatives are returned in a fixed order, so the proposal set is
/// reproducible.  A mutation never changes the family of a candidate: it only
/// changes how strict, how greedy or how anchored the candidate is.
fn mutations(spec: &CandidateSpec) -> SmallVec<CandidateSpec, 10> {
    let mut out: SmallVec<CandidateSpec, 10> = SmallVec::new();
    match spec {
        CandidateSpec::Validate {
            kind,
            strictness,
            charset,
            max_len,
        } => {
            for alternative in STRICTNESSES {
                if alternative != *strictness {
                    push(
                        &mut out,
                        CandidateSpec::Validate {
                            kind: *kind,
                            strictness: alternative,
                            charset: *charset,
                            max_len: *max_len,
                        },
                    );
                }
            }
            for alternative in CHARSETS {
                if alternative != *charset {
                    push(
                        &mut out,
                        CandidateSpec::Validate {
                            kind: *kind,
                            strictness: *strictness,
                            charset: alternative,
                            max_len: *max_len,
                        },
                    );
                }
            }
        }
        CandidateSpec::Extract {
            kind,
            boundary,
            capture,
        } => {
            for alternative in BOUNDARIES {
                if alternative != *boundary {
                    push(
                        &mut out,
                        CandidateSpec::Extract {
                            kind: *kind,
                            boundary: alternative,
                            capture: *capture,
                        },
                    );
                }
            }
            for alternative in CAPTURES {
                if alternative != *capture {
                    push(
                        &mut out,
                        CandidateSpec::Extract {
                            kind: *kind,
                            boundary: *boundary,
                            capture: alternative,
                        },
                    );
                }
            }
        }
        CandidateSpec::Filter { action, terms } => {
            for alternative in ACTIONS {
                if alternative != *action {
                    push(
                        &mut out,
                        CandidateSpec::Filter {
                            action: alternative,
                            terms: *terms,
                        },
                    );
                }
            }
        }
        CandidateSpec::Custom { template, params } => {
            if let Some(current) = template_class(template) {
                for alternative in CHARSETS {
                    if alternative != current {
                        if let Some(replacement) = with_class(template, alternative) {
                            push(
                                &mut out,
                                CandidateSpec::Custom {
                                    template: replacement,
                                    params: *params,
                                },
                            );
                        }
                    }
                }
            }
            for (start, end) in ANCHORINGS {
                if start != params.anchored_start || end != params.anchored_end {
                    push(
                        &mut out,
                        CandidateSpec::Custom {
                            template: *template,
                            params: PatternParams {
                                anchored_start: start,
                                anchored_end: end,
                                ..*params
                            },
                        },
                    );
                }
            }
        }
    }
    out
}

/// Appends a candidate to the list of alternatives.
fn push(out: &mut SmallVec<CandidateSpec, 10>, spec: CandidateSpec) {
    out.push(spec);
}

/// The character repertoire a template is built on, when it has one.
fn template_class(template: &PatternTemplate) -> Option<CharsetPolicy> {
    match template {
        PatternTemplate::Repeat { class, .. } => Some(*class),
        PatternTemplate::LiteralThenClass { class, .. } => Some(*class),
        PatternTemplate::Sequence(pieces) => pieces.iter().find_map(|piece| match piece {
            TemplatePiece::Class(policy) => Some(*policy),
            _ => None,
        }),
    }
}

/// The same template, built on another character repertoire.
fn with_class(template: &PatternTemplate, class: CharsetPolicy) -> Option<PatternTemplate> {
    match template {
        PatternTemplate::Repeat { count, .. } => Some(PatternTemplate::Repeat {
            class,
            count: *count,
        }),
        PatternTemplate::LiteralThenClass { literal, count, .. } => {
            Some(PatternTemplate::LiteralThenClass {
                literal: *literal,
                class,
                count: *count,
            })
        }
        PatternTemplate::Sequence(pieces) => {
            let mut out: SmallVec<TemplatePiece, 8> = SmallVec::new();
            let mut changed = false;
            for piece in pieces.iter() {
                let piece = match piece {
                    TemplatePiece::Class(_) => {
                        changed = true;
                        TemplatePiece::Class(class)
                    }
                    other => *other,
                };
                if !out.push(piece) {
                    break;
                }
            }
            if changed {
                Some(PatternTemplate::Sequence(out))
            } else {
                None
            }
        }
    }
}
