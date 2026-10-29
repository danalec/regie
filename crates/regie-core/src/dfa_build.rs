//! Determinization: Thompson program to a compact DFA.
//!
//! The construction is a subset construction over byte equivalence classes.
//! Two details matter for the embedded use case:
//!
//! * **Unanchored by default.** Every state includes the program start, so a
//!   single left-to-right pass finds a match anywhere in the input.
//! * **Anchors are handled explicitly.** A pattern that starts with `^` is
//!   anchored (no restart), and a pattern that ends with `$` only accepts at
//!   the end of the input. Every other zero-width assertion is rejected:
//!   it cannot be expressed by a deterministic automaton without look-around.

use alloc::vec::Vec;

use crate::class::ClassTable;
use crate::dfa::{Dfa, MAX_CLASSES, MAX_DFA_STATES};
use crate::error::{RegexError, Result};
use crate::program::{Op, Program};

/// The result of an epsilon closure.
#[derive(Clone)]
struct Closure {
    /// The consuming instructions in the closure.
    chars: Vec<u16>,
    /// Whether the match instruction is reachable.
    matched: bool,
    /// Whether the closure needs the start of the text.
    start_assert: bool,
    /// Whether the closure needs the end of the text.
    end_assert: bool,
}

impl Closure {
    fn new() -> Closure {
        Closure {
            chars: Vec::new(),
            matched: false,
            start_assert: false,
            end_assert: false,
        }
    }

    /// The identity of a DFA state.
    fn key(&self) -> Vec<u16> {
        let mut key = self.chars.clone();
        key.sort_unstable();
        key
    }
}

/// Builds a deterministic automaton for `program`.
pub(crate) fn build(program: &Program, classes: &ClassTable) -> Result<Dfa> {
    let (class_count, byte_class) = classes.partition();
    if class_count as usize > MAX_CLASSES {
        return Err(RegexError::TooManyClasses { limit: MAX_CLASSES });
    }

    // The start state tells us whether the pattern is anchored.
    let start_closure = closure_of(program, &[program.start()])?;
    let anchored = start_closure.start_assert;

    // Every byte of an equivalence class belongs to exactly the same byte
    // sets, so one representative byte per class is enough to test whether a
    // program instruction accepts that class.
    let mut representative = [0u8; 256];
    let mut seen = [false; 256];
    for (byte, class) in byte_class.iter().enumerate() {
        let class = *class as usize;
        if !seen[class] {
            seen[class] = true;
            representative[class] = byte as u8;
        }
    }

    let mut states: Vec<Closure> = Vec::new();
    let mut transitions: Vec<Vec<u16>> = Vec::new();

    // State 0 is the dead state, state 1 is the start state.
    states.push(Closure::new());
    transitions.push(Vec::new());
    let start_id = 1usize;
    states.push(start_closure);
    transitions.push(Vec::new());

    let mut pending: Vec<usize> = alloc::vec![start_id];
    let mut processed: Vec<bool> = alloc::vec![false; states.len()];
    while let Some(state_id) = pending.pop() {
        if processed[state_id] {
            continue;
        }
        processed[state_id] = true;
        let state = states[state_id].clone();
        let mut row = alloc::vec![0u16; class_count as usize];
        for class in 0..class_count as usize {
            // The instructions that can consume a byte of this class.
            let mut seeds: Vec<u16> = Vec::new();
            for pc in state.chars.iter() {
                let inst = program.inst(*pc);
                if classes.contains(inst.x, representative[class]) {
                    seeds.push(inst.y);
                }
            }
            if !anchored {
                // A match may start at any position.
                seeds.push(program.start());
            }
            if seeds.is_empty() {
                row[class] = 0;
                continue;
            }
            let target = closure_of(program, &seeds)?;
            if target.start_assert && !anchored {
                return Err(RegexError::Unsupported {
                    position: 0,
                    feature: "start-of-text assertion inside the pattern",
                });
            }
            let target_id = intern(&mut states, &mut transitions, target)?;
            row[class] = target_id as u16;
        }
        transitions[state_id] = row;
        while processed.len() < states.len() {
            processed.push(false);
        }
        for target in transitions[state_id].iter().copied() {
            let target = target as usize;
            if target != 0 && !processed[target] && !pending.contains(&target) {
                pending.push(target);
            }
        }
    }

    if states.len() > MAX_DFA_STATES {
        return Err(RegexError::DfaTooLarge {
            limit: MAX_DFA_STATES,
        });
    }

    let mut dfa = Dfa::new();
    dfa.set_class_count(class_count);
    dfa.set_state_count(states.len() as u16);
    for (byte, class) in byte_class.iter().enumerate() {
        dfa.set_byte_class(byte as u8, *class);
    }
    // Every accepting state has to agree on whether the match needs the
    // end of the input; otherwise the automaton would need per-state
    // information that does not fit the fixed layout.
    let mut accept_at_end: Option<bool> = None;
    for (state_id, state) in states.iter().enumerate() {
        if state.matched {
            match accept_at_end {
                None => accept_at_end = Some(state.end_assert),
                Some(flag) if flag == state.end_assert => {}
                Some(_) => {
                    return Err(RegexError::Unsupported {
                        position: 0,
                        feature: "end-of-text assertion in only some alternatives",
                    })
                }
            }
            dfa.set_accept(state_id as u16, true);
        }
    }
    let accept_at_end = accept_at_end.unwrap_or(false);
    for (state_id, row) in transitions.iter().enumerate() {
        for (class, target) in row.iter().enumerate() {
            dfa.set_transition(state_id as u16, class as u8, *target);
        }
    }
    dfa.set_accept_at_end(accept_at_end);
    Ok(dfa)
}

/// Finds or creates the DFA state for `closure`.
fn intern(
    states: &mut Vec<Closure>,
    transitions: &mut Vec<Vec<u16>>,
    closure: Closure,
) -> Result<usize> {
    let key = closure.key();
    for (index, state) in states.iter().enumerate() {
        if state.key() == key && state.matched == closure.matched {
            return Ok(index);
        }
    }
    if states.len() >= MAX_DFA_STATES {
        return Err(RegexError::DfaTooLarge {
            limit: MAX_DFA_STATES,
        });
    }
    states.push(closure);
    transitions.push(Vec::new());
    Ok(states.len() - 1)
}

/// Computes the epsilon closure of `seeds`.
fn closure_of(program: &Program, seeds: &[u16]) -> Result<Closure> {
    let mut out = Closure::new();
    let mut visited = alloc::vec![false; program.len()];
    for seed in seeds {
        visit(program, *seed, &mut visited, &mut out)?;
    }
    Ok(out)
}

/// Adds `pc` and everything reachable without consuming input to `out`.
fn visit(program: &Program, pc: u16, visited: &mut [bool], out: &mut Closure) -> Result<()> {
    if visited[pc as usize] {
        return Ok(());
    }
    visited[pc as usize] = true;
    let inst = program.inst(pc);
    match inst.op {
        Op::Char => {
            out.chars.push(pc);
        }
        Op::Match => {
            out.matched = true;
        }
        Op::Split => {
            visit(program, inst.x, visited, out)?;
            visit(program, inst.y, visited, out)?;
        }
        Op::Save => visit(program, inst.y, visited, out)?,
        Op::Assert => match inst.x {
            0 => {
                // Start of text: only valid as the very first thing.
                if !out.chars.is_empty() {
                    return Err(RegexError::Unsupported {
                        position: 0,
                        feature: "start-of-text assertion inside the pattern",
                    });
                }
                out.start_assert = true;
                visit(program, inst.y, visited, out)?;
            }
            1 => {
                // End of text: only valid when the match follows directly.
                let mut tail = Closure::new();
                let mut tail_visited = alloc::vec![false; program.len()];
                visit(program, inst.y, &mut tail_visited, &mut tail)?;
                if !tail.chars.is_empty() || !tail.matched {
                    return Err(RegexError::Unsupported {
                        position: 0,
                        feature: "end-of-text assertion inside the pattern",
                    });
                }
                out.end_assert = true;
                out.matched = true;
            }
            _ => {
                return Err(RegexError::Unsupported {
                    position: 0,
                    feature: "zero-width assertion in a deterministic automaton",
                })
            }
        },
    }
    Ok(())
}
