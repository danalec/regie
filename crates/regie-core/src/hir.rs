//! The lowered syntax tree.
//!
//! Parsing produces a tree of [`HirNode`]s stored in a fixed-capacity arena.
//! The tree is deliberately small and `Copy`-friendly: it lives inside the
//! compiled [`crate::Regex`] value, so a compiled pattern never needs the
//! allocator.

use crate::error::{RegexError, Result};

/// The maximum number of nodes a single pattern may lower to.
pub const MAX_HIR_NODES: usize = 512;

/// A zero-width assertion.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum AssertKind {
    /// Start of the whole input.
    StartText,
    /// End of the whole input.
    EndText,
    /// Start of a line.
    StartLine,
    /// End of a line.
    EndLine,
    /// Between a word and a non-word character.
    WordBoundary,
    /// Between two word or two non-word characters.
    NotWordBoundary,
}

/// A node of the lowered syntax tree.
///
/// Children are arena indices; `Concat` and `Alternate` are binary so that
/// the tree stays shallow and the traversal order is explicit.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum HirNode {
    /// Matches the empty string.
    Empty,
    /// Matches one literal byte.
    LiteralByte(u8),
    /// Matches one byte belonging to the class with the given index.
    Class(u16),
    /// Matches any single byte (subject to the dot-all flag).
    Any,
    /// Matches the empty string at a zero-width position.
    Assert(AssertKind),
    /// Matches the left child followed by the right child.
    Concat(u16, u16),
    /// Matches the left child or the right child, left first.
    Alternate(u16, u16),
    /// Matches the child between `min` and `max` times.
    Repeat {
        /// The repeated child.
        child: u16,
        /// The minimum number of repetitions.
        min: u16,
        /// The maximum number of repetitions, `None` means unbounded.
        max: Option<u16>,
        /// Whether the repetition prefers to consume as much as possible.
        greedy: bool,
    },
    /// Matches the child and records it as a capture group.
    Capture {
        /// The captured child.
        child: u16,
        /// The index of the capture group.
        index: u16,
    },
}

/// A fixed-capacity arena of [`HirNode`]s.
#[derive(Clone, Copy)]
pub struct HirArena {
    nodes: [HirNode; MAX_HIR_NODES],
    len: u16,
}

impl HirArena {
    /// Creates an empty arena.
    pub fn new() -> Self {
        Self {
            nodes: [HirNode::Empty; MAX_HIR_NODES],
            len: 0,
        }
    }

    /// Appends a node, returning its index.
    pub fn push(&mut self, node: HirNode) -> Result<u16> {
        if self.len as usize >= MAX_HIR_NODES {
            return Err(RegexError::ProgramTooLarge {
                limit: MAX_HIR_NODES,
            });
        }
        let index = self.len;
        self.nodes[index as usize] = node;
        self.len += 1;
        Ok(index)
    }

    /// Returns the node stored at `index`.
    pub fn get(&self, index: u16) -> HirNode {
        self.nodes[index as usize]
    }

    /// The number of nodes stored.
    pub fn len(&self) -> usize {
        self.len as usize
    }

    /// Whether the arena holds no nodes.
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
}

impl Default for HirArena {
    fn default() -> Self {
        Self::new()
    }
}
