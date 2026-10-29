//! Compilation: lowered syntax tree to executable program.
//!
//! The compiler builds a Thompson program: a flat array of instructions with
//! explicit jumps. There is no backtracking anywhere in the engine, so a
//! program is always safe to execute, no matter how the pattern looks.

use crate::class::ClassTable;
use crate::error::{RegexError, Result};
use crate::hir::{HirArena, HirNode};
use crate::spec::CandidateSpec;
use crate::Flags;

/// The maximum number of instructions a compiled program may hold.
pub const MAX_INST: usize = 512;

/// The operation of a program instruction.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Op {
    /// Consume one byte belonging to the byte set in `x`.
    Char,
    /// Assert a zero-width condition; `x` holds the assertion kind.
    Assert,
    /// Continue at `x`, or at `y` when the first branch fails.
    Split,
    /// Record the current position in the capture slot `x`, then continue at `y`.
    Save,
    /// The match ends here.
    Match,
}

/// One instruction of a compiled program.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Inst {
    pub op: Op,
    pub x: u16,
    pub y: u16,
}

/// A compiled program, executed by the Pike VM.
#[derive(Clone, Copy)]
pub(crate) struct Program {
    insts: [Inst; MAX_INST],
    len: u16,
    start: u16,
}

impl Program {
    fn new() -> Program {
        Program {
            insts: [Inst {
                op: Op::Match,
                x: 0,
                y: 0,
            }; MAX_INST],
            len: 0,
            start: 0,
        }
    }

    /// The instruction the simulation starts from.
    pub(crate) fn start(&self) -> u16 {
        self.start
    }

    /// The instruction at `pc`.
    pub(crate) fn inst(&self, pc: u16) -> Inst {
        self.insts[pc as usize]
    }

    /// The number of instructions.
    ///
    /// Only the determinizer needs this, so it is part of the `alloc` build.
    #[cfg(feature = "alloc")]
    pub(crate) fn len(&self) -> usize {
        self.len as usize
    }
}

/// Compiles a lowered syntax tree into an executable program.
pub(crate) fn compile(
    arena: &HirArena,
    root: u16,
    classes: &mut ClassTable,
    flags: Flags,
) -> Result<(Program, u16, bool)> {
    let mut compiler = Compiler {
        program: Program::new(),
        classes,
        flags,
        next: 0,
        group_count: 0,
        has_asserts: false,
    };
    // Instruction 0 is always the `Match` instruction; every compiled
    // fragment continues there when it is done.
    compiler.push(Op::Match, 0, 0)?;
    let start = compiler.emit_node(arena, root, 0)?;
    compiler.program.len = compiler.next;
    compiler.program.start = start;
    let group_count = compiler.group_count;
    let has_asserts = compiler.has_asserts;
    Ok((compiler.program, group_count, has_asserts))
}

/// Compiles a structured specification into an executable program.
pub(crate) fn compile_spec(spec: &CandidateSpec) -> Result<(Program, ClassTable, u16, bool)> {
    let (arena, mut classes, root) = crate::spec_compile::build(spec)?;
    let (program, group_count, has_asserts) =
        compile(&arena, root, &mut classes, Flags::default())?;
    Ok((program, classes, group_count, has_asserts))
}

struct Compiler<'a> {
    program: Program,
    classes: &'a mut ClassTable,
    flags: Flags,
    next: u16,
    group_count: u16,
    has_asserts: bool,
}

impl<'a> Compiler<'a> {
    fn push(&mut self, op: Op, x: u16, y: u16) -> Result<u16> {
        if self.next as usize >= MAX_INST {
            return Err(RegexError::ProgramTooLarge { limit: MAX_INST });
        }
        let pc = self.next;
        self.program.insts[pc as usize] = Inst { op, x, y };
        self.next += 1;
        Ok(pc)
    }

    fn set(&mut self, pc: u16, x: u16, y: u16) {
        self.program.insts[pc as usize] = Inst {
            op: self.program.insts[pc as usize].op,
            x,
            y,
        };
    }

    /// Emits the code for `node`, continuing at `next`.
    fn emit_node(&mut self, arena: &HirArena, node: u16, next: u16) -> Result<u16> {
        match arena.get(node) {
            HirNode::Empty => Ok(next),
            HirNode::LiteralByte(byte) => {
                let mut set = [0u64; 4];
                crate::class::set_add(&mut set, byte);
                if self.flags.case_insensitive && byte.is_ascii_alphabetic() {
                    crate::class::set_add(&mut set, byte.to_ascii_lowercase());
                    crate::class::set_add(&mut set, byte.to_ascii_uppercase());
                }
                let id = self.classes.add(&set)?;
                self.push(Op::Char, id, next)
            }
            HirNode::Class(id) => self.push(Op::Char, id, next),
            HirNode::Any => {
                let mut set = [0u64; 4];
                crate::class::set_add_range(&mut set, 0, 255);
                if !self.flags.dot_matches_newline {
                    crate::class::set_add(&mut set, b'\n');
                }
                let id = self.classes.add(&set)?;
                self.push(Op::Char, id, next)
            }
            HirNode::Assert(kind) => {
                self.has_asserts = true;
                self.push(Op::Assert, kind as u16, next)
            }
            HirNode::Concat(left, right) => {
                let right_pc = self.emit_node(arena, right, next)?;
                self.emit_node(arena, left, right_pc)
            }
            HirNode::Alternate(left, right) => {
                let split = self.push(Op::Split, 0, 0)?;
                let left_pc = self.emit_node(arena, left, next)?;
                let right_pc = self.emit_node(arena, right, next)?;
                self.set(split, left_pc, right_pc);
                Ok(split)
            }
            HirNode::Repeat {
                child,
                min,
                max,
                greedy,
            } => self.emit_repeat(arena, child, min, max, greedy, next),
            HirNode::Capture { child, index } => {
                if self.group_count as usize >= crate::MAX_CAPTURE_GROUPS {
                    return Err(RegexError::TooManyGroups {
                        limit: crate::MAX_CAPTURE_GROUPS,
                    });
                }
                self.group_count = self.group_count.max(index + 1);
                let close = self.push(Op::Save, 2 * index + 1, next)?;
                let inner = self.emit_node(arena, child, close)?;
                let open = self.push(Op::Save, 2 * index, inner)?;
                Ok(open)
            }
        }
    }

    fn emit_repeat(
        &mut self,
        arena: &HirArena,
        child: u16,
        min: u16,
        max: Option<u16>,
        greedy: bool,
        next: u16,
    ) -> Result<u16> {
        let mut cont = next;
        // Optional repetitions, built back to front so that the continuation
        // of each copy is the copy that follows it.
        let optional = match max {
            Some(max) => max.saturating_sub(min),
            None => 0,
        };
        for _ in 0..optional {
            let split = self.push(Op::Split, 0, 0)?;
            let body = self.emit_node(arena, child, cont)?;
            if greedy {
                self.set(split, body, cont);
            } else {
                self.set(split, cont, body);
            }
            cont = split;
        }
        // An unbounded tail becomes a loop.
        if max.is_none() {
            let split = self.push(Op::Split, 0, 0)?;
            let body = self.emit_node(arena, child, split)?;
            if greedy {
                self.set(split, body, cont);
            } else {
                self.set(split, cont, body);
            }
            cont = split;
        }
        // Mandatory repetitions.
        for _ in 0..min {
            let body = self.emit_node(arena, child, cont)?;
            cont = body;
        }
        Ok(cont)
    }
}

/// Builds a deterministic automaton for a compiled regex.
#[cfg(feature = "alloc")]
pub(crate) fn build_dfa(regex: &crate::Regex) -> Result<crate::Dfa> {
    crate::dfa_build::build(&regex.program, &regex.classes)
}
