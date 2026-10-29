//! The bounded tree search.
//!
//! With the `mcts` feature the search does not walk the proposal set flatly:
//! it walks the *mutation tree* and expands the most promising branch first.
//! The roots of the tree are the candidates the intent model proposed; every
//! other node is a mutation of its parent.
//!
//! The search is deterministic.  There is no randomness and no rollout: the
//! node that is expanded next is the one whose parent scored highest, with
//! the canonical proposal index breaking the ties.  A visit budget bounds the
//! work, and the budget is large enough to cover the whole proposal set, so
//! the tree search and the flat enumeration reach the same verdict.

use crate::propose::{ProposalSet, MAX_EXPLORED, NO_PARENT};
use crate::verify::{verify, Verdict};
use crate::Examples;
use regie_core::SmallVec;

/// The number of nodes the tree search may visit.
const VISIT_BUDGET: usize = MAX_EXPLORED;

/// Explores the mutation tree and returns the verdict of every visited node.
///
/// The verdicts are returned in the order the nodes were visited, together
/// with the canonical index of the candidate each verdict belongs to.
pub(crate) fn explore(
    proposals: &ProposalSet,
    examples: Examples<'_>,
) -> SmallVec<(u16, Verdict), MAX_EXPLORED> {
    let mut visited = [false; MAX_EXPLORED];
    let mut scores = [0u32; MAX_EXPLORED];
    let mut out: SmallVec<(u16, Verdict), MAX_EXPLORED> = SmallVec::new();

    // The roots carry no parent and are always visited first.
    for index in 0..proposals.len() {
        if proposals.parent(index) != NO_PARENT {
            continue;
        }
        if out.len() >= VISIT_BUDGET {
            break;
        }
        let verdict = verify(&proposals.spec(index), examples);
        scores[index] = verdict.score;
        visited[index] = true;
        out.push((index as u16, verdict));
    }

    // Then the tree is expanded best first: the unvisited node whose parent
    // scored highest is visited next.
    while out.len() < VISIT_BUDGET {
        let mut best: Option<usize> = None;
        for index in 0..proposals.len() {
            if visited[index] {
                continue;
            }
            let parent = proposals.parent(index);
            if parent == NO_PARENT || !visited[parent as usize] {
                continue;
            }
            let score = scores[parent as usize];
            match best {
                None => best = Some(index),
                Some(current) => {
                    let current_score = scores[proposals.parent(current) as usize];
                    if score > current_score || (score == current_score && index < current) {
                        best = Some(index);
                    }
                }
            }
        }
        let index = match best {
            Some(index) => index,
            None => break,
        };
        let verdict = verify(&proposals.spec(index), examples);
        scores[index] = verdict.score;
        visited[index] = true;
        out.push((index as u16, verdict));
    }

    out
}
