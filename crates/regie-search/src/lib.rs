//! # regie-search
//!
//! The verified hypothesis search of the `regie` project.
//!
//! The intent model of [`regie-intent`](https://docs.rs/regie-intent)
//! proposes a handful of *structured* specifications for a request.  The
//! search takes those proposals, mutates them along the axes the design
//! defines, and verifies every candidate against the examples the caller
//! supplies.  Only a candidate that matches **every** positive example and
//! **no** negative example survives.
//!
//! * **Deterministic.** No randomness, no clock, no hash iteration order:
//!   the same request and the same examples always produce the same report.
//! * **Verified.** A candidate that does not compile is rejected instead of
//!   reported as an error, and a candidate that contradicts the examples
//!   never reaches the outcome.
//! * **Honest.** When no candidate survives, the search returns
//!   [`IntentSpec::Ambiguous`] with
//!   [`AmbiguityReason::InconsistentExamples`]: it never invents a
//!   specification.
//!
//! ## The algorithm
//!
//! 1. **Propose.** The candidates of the intent model are the seeds of the
//!    search.
//! 2. **Mutate.** Every seed is mutated along the axes of its family: the
//!    strictness and the character repertoire of a validation, the boundary
//!    and the capture policy of an extraction, the action of a filter and the
//!    anchoring of a custom pattern.
//! 3. **Verify.** Every candidate is compiled with
//!    [`regie_core::Regex::from_spec`] and matched against the examples.
//! 4. **Score.** A candidate earns points for every positive example it
//!    matches and for every negative example it rejects.
//! 5. **Select.** One surviving family yields
//!    [`IntentSpec::Confident`]; several surviving families yield
//!    [`IntentSpec::Ambiguous`] with the best candidates of each family; no
//!    survivor at all yields [`IntentSpec::Ambiguous`] with
//!    [`AmbiguityReason::InconsistentExamples`].
//!
//! ## Example
//!
//! ```
//! use regie_core::spec::{CandidateSpec, IntentSpec, ValidationKind};
//! use regie_search::{search, Examples};
//!
//! let positives = ["123.456.789-09"];
//! let negatives = ["1234567890a"];
//! let report = search("validar CPF", Examples { positive: &positives, negative: &negatives });
//!
//! match report.outcome {
//!     IntentSpec::Confident(CandidateSpec::Validate { kind, .. }) => {
//!         assert_eq!(kind, ValidationKind::BrazilianCpf);
//!     }
//!     other => panic!("unexpected outcome: {:?}", other),
//! }
//! ```
//!
//! ## Features
//!
//! * `alloc` and `std` are forwarded to `regie-core`; the crate needs no
//!   allocator of its own.
//! * `mcts` replaces the flat enumeration of the proposal set with a bounded
//!   tree search over the same mutation axes.  The public API is identical
//!   either way and both paths reach the same outcome.

#![cfg_attr(not(feature = "std"), no_std)]
#![doc(html_root_url = "https://docs.rs/regie-search/0.1.0")]
#![forbid(unsafe_code)]

#[cfg(feature = "mcts")]
mod mcts;
mod propose;
mod verify;

pub use propose::MAX_EXPLORED;
pub use regie_core::spec;
pub use regie_core::spec::{
    AmbiguityReason, BoundaryPolicy, CandidateSpec, CapturePolicy, CharsetPolicy, ExtractionKind,
    FilterAction, IntentSpec, PatternParams, PatternTemplate, PatternTerm, Strictness,
    ValidationKind,
};
pub use regie_core::SmallVec;

use propose::ProposalSet;
use regie_intent::IntentModel;
use verify::Verdict;

/// The examples a candidate specification has to satisfy.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Examples<'a> {
    /// The values a candidate has to match.
    pub positive: &'a [&'a str],
    /// The values a candidate has to reject.
    pub negative: &'a [&'a str],
}

impl<'a> Examples<'a> {
    /// Builds a set of examples.
    pub const fn new(positive: &'a [&'a str], negative: &'a [&'a str]) -> Self {
        Self { positive, negative }
    }

    /// Whether the set holds no example at all.
    ///
    /// A search without examples cannot verify anything, so it falls back to
    /// the ranking of the intent model.
    pub fn is_empty(&self) -> bool {
        self.positive.is_empty() && self.negative.is_empty()
    }
}

/// A candidate that was checked against the examples.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct ScoredCandidate {
    /// The specification of the candidate.
    pub spec: CandidateSpec,
    /// The score of the candidate.
    ///
    /// The score is the sum of the points earned by the positive examples the
    /// candidate matched and the points earned by the negative examples it
    /// rejected, so a candidate that satisfies every example always scores
    /// the most.
    pub score: u32,
    /// The number of positive examples the candidate matched.
    pub positives_matched: u16,
    /// The number of negative examples the candidate rejected.
    pub negatives_rejected: u16,
}

/// The outcome of one search.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct SearchReport {
    /// The intent the search settled on.
    pub outcome: IntentSpec,
    /// The best candidates of the search, most promising first.
    pub ranked: SmallVec<ScoredCandidate, 4>,
    /// The number of candidates the search explored.
    pub explored: u16,
    /// The number of candidates the search rejected.
    ///
    /// A candidate is rejected when it does not compile or when it fails one
    /// of the examples.
    pub rejected: u16,
}

impl SearchReport {
    /// The specification of a confident outcome.
    pub fn candidate(&self) -> Option<&CandidateSpec> {
        self.outcome.candidate()
    }

    /// Whether the search settled on a single specification.
    pub fn is_confident(&self) -> bool {
        self.outcome.is_confident()
    }
}

/// Runs the verified search for one request.
///
/// The candidates of the intent model are the seeds of the search; they are
/// mutated along the axes of their family and every candidate is verified
/// against `examples`.  At most [`MAX_EXPLORED`] candidates are explored.
///
/// When `examples` holds no example at all, nothing can be verified, so the
/// search falls back to the intent model: the top candidate of its ranking is
/// returned as [`IntentSpec::Confident`], unless the model itself was
/// ambiguous.
///
/// ```
/// use regie_core::spec::IntentSpec;
/// use regie_search::{search, Examples};
///
/// let positives = ["123.456.789-09"];
/// let negatives = ["1234567890a"];
/// let report = search("validar CPF", Examples::new(&positives, &negatives));
/// assert!(matches!(report.outcome, IntentSpec::Confident(_)));
/// ```
pub fn search(request: &str, examples: Examples<'_>) -> SearchReport {
    let model = IntentModel::embedded();
    let seeds = model.candidates(request);
    if examples.is_empty() {
        return model_fallback(request, seeds.as_slice());
    }
    explore(seeds.as_slice(), examples, default_mode())
}

/// Runs the verified search with the flat enumeration of the proposal set.
///
/// This is the reference implementation of the search: it walks the proposal
/// set in canonical order instead of expanding the mutation tree.  Both
/// paths reach the same outcome, and the tree search only exists to explore
/// the alternatives in a different order.
///
/// ```
/// use regie_search::{search_flat, Examples};
///
/// let positives = ["123.456.789-09"];
/// let negatives = ["1234567890a"];
/// let report = search_flat("validar CPF", Examples::new(&positives, &negatives));
/// assert!(report.is_confident());
/// ```
pub fn search_flat(request: &str, examples: Examples<'_>) -> SearchReport {
    let model = IntentModel::embedded();
    let seeds = model.candidates(request);
    if examples.is_empty() {
        return model_fallback(request, seeds.as_slice());
    }
    explore(seeds.as_slice(), examples, Mode::Flat)
}

/// Runs the verified search over an explicit list of candidates.
///
/// The list plays the role of the seeds the intent model would have
/// proposed: the candidates are mutated along the axes of their family and
/// every candidate is verified against `examples`.
///
/// When `examples` holds no example at all, the ranking of the caller is
/// trusted: the first candidate is returned as [`IntentSpec::Confident`].  An
/// empty list yields [`IntentSpec::Ambiguous`] with
/// [`AmbiguityReason::LowConfidence`].
///
/// ```
/// use regie_core::spec::{CandidateSpec, CharsetPolicy, Strictness, ValidationKind};
/// use regie_search::{search_with_candidates, Examples};
///
/// let candidate = CandidateSpec::Validate {
///     kind: ValidationKind::BrazilianCpf,
///     strictness: Strictness::Standard,
///     charset: CharsetPolicy::Digits,
///     max_len: Some(14),
/// };
/// let positives = ["123.456.789-09"];
/// let negatives = ["1234567890a"];
/// let report = search_with_candidates(&[candidate], Examples::new(&positives, &negatives));
/// assert!(report.is_confident());
/// ```
pub fn search_with_candidates(
    candidates: &[CandidateSpec],
    examples: Examples<'_>,
) -> SearchReport {
    if examples.is_empty() {
        return explicit_fallback(candidates);
    }
    explore(candidates, examples, default_mode())
}

/// Runs the verified search over an explicit list of candidates, with the
/// flat enumeration of the proposal set.
///
/// This is the flat counterpart of [`search_with_candidates`].
pub fn search_with_candidates_flat(
    candidates: &[CandidateSpec],
    examples: Examples<'_>,
) -> SearchReport {
    if examples.is_empty() {
        return explicit_fallback(candidates);
    }
    explore(candidates, examples, Mode::Flat)
}

/// How the search explores the proposal set.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Mode {
    /// The flat enumeration of the proposal set.
    Flat,
    /// The bounded tree search over the mutation tree.
    #[cfg(feature = "mcts")]
    Tree,
}

/// The exploration mode of a build.
fn default_mode() -> Mode {
    #[cfg(feature = "mcts")]
    {
        Mode::Tree
    }
    #[cfg(not(feature = "mcts"))]
    {
        Mode::Flat
    }
}

/// Explores the candidates and builds the report of the search.
fn explore(candidates: &[CandidateSpec], examples: Examples<'_>, mode: Mode) -> SearchReport {
    let proposals = propose::build(candidates);
    let explored = match mode {
        Mode::Flat => explore_flat(&proposals, examples),
        #[cfg(feature = "mcts")]
        Mode::Tree => mcts::explore(&proposals, examples),
    };
    build_report(&proposals, explored.as_slice(), examples)
}

/// Walks the proposal set in canonical order.
fn explore_flat(
    proposals: &ProposalSet,
    examples: Examples<'_>,
) -> SmallVec<(u16, Verdict), MAX_EXPLORED> {
    let mut out: SmallVec<(u16, Verdict), MAX_EXPLORED> = SmallVec::new();
    for index in 0..proposals.len() {
        if out.len() >= MAX_EXPLORED {
            break;
        }
        let verdict = verify::verify(&proposals.spec(index), examples);
        out.push((index as u16, verdict));
    }
    out
}

/// Builds the report of a search from the verdicts of the explored
/// candidates.
fn build_report(
    proposals: &ProposalSet,
    explored: &[(u16, Verdict)],
    examples: Examples<'_>,
) -> SearchReport {
    let mut ranked: SmallVec<Ranked, 4> = SmallVec::new();
    let mut families: [(Family, u16); MAX_EXPLORED] = [(Family::Filter, 0); MAX_EXPLORED];
    let mut family_count = 0usize;
    let mut rejected = 0u16;

    for (index, verdict) in explored.iter() {
        let spec = proposals.spec(*index as usize);
        push_ranked(
            &mut ranked,
            Ranked {
                candidate: ScoredCandidate {
                    spec,
                    score: verdict.score,
                    positives_matched: verdict.positives_matched,
                    negatives_rejected: verdict.negatives_rejected,
                },
                index: *index,
            },
        );

        if !verdict.is_consistent(examples) {
            rejected = rejected.saturating_add(1);
            continue;
        }

        // Remember the best candidate of every surviving family.
        let family = Family::of(spec);
        let mut slot = None;
        for (position, (known, _)) in families.iter().take(family_count).enumerate() {
            if *known == family {
                slot = Some(position);
                break;
            }
        }
        match slot {
            Some(position) => {
                if *index < families[position].1 {
                    families[position].1 = *index;
                }
            }
            None => {
                if family_count < families.len() {
                    families[family_count] = (family, *index);
                    family_count += 1;
                }
            }
        }
    }

    // The families are ordered by the index of their best candidate, which is
    // the canonical proposal order: the most promising family comes first.
    for position in 1..family_count {
        let mut cursor = position;
        while cursor > 0 && families[cursor].1 < families[cursor - 1].1 {
            families.swap(cursor, cursor - 1);
            cursor -= 1;
        }
    }

    let outcome = match family_count {
        0 => IntentSpec::Ambiguous {
            reason: AmbiguityReason::InconsistentExamples,
            suggestions: SmallVec::new(),
        },
        1 => IntentSpec::Confident(proposals.spec(families[0].1 as usize)),
        _ => {
            let mut suggestions: SmallVec<CandidateSpec, 3> = SmallVec::new();
            for (_, index) in families.iter().take(family_count) {
                if suggestions.len() == suggestions.capacity() {
                    break;
                }
                suggestions.push(proposals.spec(*index as usize));
            }
            IntentSpec::Ambiguous {
                reason: AmbiguityReason::ConflictingKinds,
                suggestions,
            }
        }
    };

    let mut best: SmallVec<ScoredCandidate, 4> = SmallVec::new();
    for entry in ranked.iter() {
        if best.len() == best.capacity() {
            break;
        }
        best.push(entry.candidate);
    }

    SearchReport {
        outcome,
        ranked: best,
        explored: explored.len() as u16,
        rejected,
    }
}

/// A ranked candidate, with the index that breaks the ties.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
struct Ranked {
    candidate: ScoredCandidate,
    index: u16,
}

/// Whether `left` is more promising than `right`.
///
/// The score comes first; the canonical proposal index breaks the ties, so
/// the order never depends on the order in which the candidates were
/// explored.
fn is_better(left: &Ranked, right: &Ranked) -> bool {
    if left.candidate.score != right.candidate.score {
        left.candidate.score > right.candidate.score
    } else {
        left.index < right.index
    }
}

/// Inserts a candidate into the list of the best candidates.
fn push_ranked(ranked: &mut SmallVec<Ranked, 4>, entry: Ranked) {
    let mut position = ranked.len();
    for (index, existing) in ranked.iter().enumerate() {
        if is_better(&entry, existing) {
            position = index;
            break;
        }
    }
    if position >= ranked.capacity() {
        return;
    }
    if ranked.push(entry) {
        let mut index = ranked.len() - 1;
        while index > position {
            ranked.as_mut_slice()[index] = ranked.as_mut_slice()[index - 1];
            index -= 1;
        }
        ranked.as_mut_slice()[position] = entry;
    } else {
        // The list is full: the last candidate makes room for the new one.
        let last = ranked.capacity() - 1;
        ranked.as_mut_slice()[last] = entry;
        let mut index = last;
        while index > position {
            ranked.as_mut_slice()[index] = ranked.as_mut_slice()[index - 1];
            index -= 1;
        }
        ranked.as_mut_slice()[position] = entry;
    }
}

/// The family a candidate belongs to.
///
/// Two candidates of the same family describe the same thing, so they are
/// not a reason for ambiguity.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Family {
    /// A validation family.
    Validation(ValidationKind),
    /// An extraction family.
    Extraction(ExtractionKind),
    /// The filter family.
    Filter,
    /// The custom family.
    Custom,
}

impl Family {
    /// The family of a candidate.
    fn of(spec: CandidateSpec) -> Family {
        match spec {
            CandidateSpec::Validate { kind, .. } => Family::Validation(kind),
            CandidateSpec::Extract { kind, .. } => Family::Extraction(kind),
            CandidateSpec::Filter { .. } => Family::Filter,
            CandidateSpec::Custom { .. } => Family::Custom,
        }
    }
}

/// The report of a search without examples, taken from the intent model.
fn model_fallback(request: &str, candidates: &[CandidateSpec]) -> SearchReport {
    let model = IntentModel::embedded();
    let outcome = model.interpret(request);
    SearchReport {
        outcome,
        ranked: ranked_without_score(candidates),
        explored: clamp(candidates.len()),
        rejected: 0,
    }
}

/// The report of a search without examples, taken from an explicit list.
fn explicit_fallback(candidates: &[CandidateSpec]) -> SearchReport {
    let outcome = match candidates.first() {
        Some(spec) => IntentSpec::Confident(*spec),
        None => IntentSpec::Ambiguous {
            reason: AmbiguityReason::LowConfidence,
            suggestions: SmallVec::new(),
        },
    };
    SearchReport {
        outcome,
        ranked: ranked_without_score(candidates),
        explored: clamp(candidates.len()),
        rejected: 0,
    }
}

/// The candidates of a ranking that was not verified.
fn ranked_without_score(candidates: &[CandidateSpec]) -> SmallVec<ScoredCandidate, 4> {
    let mut out: SmallVec<ScoredCandidate, 4> = SmallVec::new();
    for candidate in candidates.iter() {
        if out.len() == out.capacity() {
            break;
        }
        out.push(ScoredCandidate {
            spec: *candidate,
            score: 0,
            positives_matched: 0,
            negatives_rejected: 0,
        });
    }
    out
}

/// Clamps a length to what a `u16` counter can hold.
fn clamp(len: usize) -> u16 {
    if len > u16::MAX as usize {
        u16::MAX
    } else {
        len as u16
    }
}
