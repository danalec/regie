//! Matching: the Pike VM.
//!
//! The engine simulates the Thompson program with an NFA simulation in the
//! style of Russ Cox's Pike VM. Two properties come out of that choice:
//!
//! * The engine never backtracks, so no pattern can trigger catastrophic
//!   backtracking. The worst case is `O(m * n)` for a program of `m`
//!   instructions over an input of `n` bytes.
//! * Threads are kept in priority order, so the match that is reported is
//!   the leftmost one, and among matches with the same start the branch the
//!   pattern prefers.
//!
//! Captures are tracked per thread. That costs memory proportional to the
//! number of live threads, which is why the capture path is the only place
//! with a noticeable scratch requirement.

use crate::error::{RegexError, Result};
use crate::hir::AssertKind;
use crate::program::{Op, Program, MAX_INST};
use crate::{Captures, Match, Regex, MAX_CAPTURE_GROUPS, MAX_CAPTURE_SLOTS};

/// The maximum number of live threads.
const MAX_THREADS: usize = 256;

/// The number of 64-bit words used to track visited instructions.
const VISIT_WORDS: usize = MAX_INST.div_ceil(64);

/// The maximum depth of the epsilon closure.
const MAX_CLOSURE_DEPTH: usize = 64;

/// One live thread of the simulation.
#[derive(Clone, Copy)]
struct Thread<const CAPS: usize> {
    pc: u16,
    start: u32,
    caps: [u32; CAPS],
}

impl<const CAPS: usize> Thread<CAPS> {
    const EMPTY: Thread<CAPS> = Thread {
        pc: 0,
        start: 0,
        caps: [u32::MAX; CAPS],
    };
}

/// Whether the pattern matches anywhere in `haystack`.
pub(crate) fn is_match(regex: &Regex, haystack: &[u8]) -> bool {
    run::<0>(regex, haystack).is_some()
}

/// Returns the leftmost match in `haystack`.
pub(crate) fn find(regex: &Regex, haystack: &[u8]) -> Option<Match> {
    run::<0>(regex, haystack).map(|(start, end, _)| Match { start, end })
}

/// Returns the leftmost match and its capture groups.
pub(crate) fn captures(regex: &Regex, haystack: &[u8]) -> Option<Captures> {
    let (start, end, caps) = run::<MAX_CAPTURE_SLOTS>(regex, haystack)?;
    let mut groups = [None; MAX_CAPTURE_GROUPS + 1];
    groups[0] = Some(Match { start, end });
    let group_count = regex.group_count();
    for group in 1..=group_count {
        let open = caps[2 * (group - 1)];
        let close = caps[2 * (group - 1) + 1];
        if open != u32::MAX && close != u32::MAX && close >= open {
            groups[group] = Some(Match {
                start: open as usize,
                end: close as usize,
            });
        }
    }
    Some(Captures {
        groups,
        len: group_count + 1,
    })
}

/// Runs the simulation.
///
/// Returns the match bounds and the capture slots.
fn run<const CAPS: usize>(regex: &Regex, haystack: &[u8]) -> Option<(usize, usize, [u32; CAPS])> {
    let program = &regex.program;
    let classes = &regex.classes;

    let mut clist = [Thread::<CAPS>::EMPTY; MAX_THREADS];
    let mut nlist = [Thread::<CAPS>::EMPTY; MAX_THREADS];
    let mut clist_len = 0usize;
    let mut nlist_len = 0usize;
    let mut cvisited = [0u64; VISIT_WORDS];
    let mut nvisited = [0u64; VISIT_WORDS];

    let mut matched = false;
    let mut match_start = 0usize;
    let mut match_end = 0usize;
    let mut match_caps = [u32::MAX; CAPS];

    let mut pos = 0usize;
    {
        let caps = [u32::MAX; CAPS];
        add_thread(
            program,
            haystack,
            &mut clist,
            &mut clist_len,
            &mut cvisited,
            program.start(),
            pos,
            pos as u32,
            caps,
            0,
        )
        .ok()?;
    }

    loop {
        let mut index = 0usize;
        while index < clist_len {
            let thread = clist[index];
            let inst = program.inst(thread.pc);
            match inst.op {
                Op::Match => {
                    // The match is provisional: every thread that survives
                    // this point descends from a thread that the depth-first
                    // order prefers, so a later match always replaces this
                    // one. The threads that the order does *not* prefer are
                    // dropped below, which is what keeps the match leftmost
                    // and the branch preferred.
                    matched = true;
                    match_start = thread.start as usize;
                    match_end = pos;
                    match_caps = thread.caps;
                    // The remaining threads are worse than this match.
                    break;
                }
                Op::Char if pos < haystack.len() && classes.contains(inst.x, haystack[pos]) => {
                    let next = Thread {
                        pc: inst.y,
                        start: thread.start,
                        caps: thread.caps,
                    };
                    add_thread(
                        program,
                        haystack,
                        &mut nlist,
                        &mut nlist_len,
                        &mut nvisited,
                        next.pc,
                        pos + 1,
                        next.start,
                        next.caps,
                        0,
                    )
                    .ok()?;
                }
                _ => {
                    // Epsilon instructions are resolved while the thread is
                    // added to a list, so they never reach this loop.
                }
            }
            index += 1;
        }

        if pos >= haystack.len() {
            break;
        }
        pos += 1;

        core::mem::swap(&mut clist, &mut nlist);
        core::mem::swap(&mut cvisited, &mut nvisited);
        clist_len = nlist_len;
        nlist_len = 0;
        nvisited = [0u64; VISIT_WORDS];

        if !matched {
            // A new attempt may start at this position. It is appended last,
            // so every attempt that started earlier keeps priority.
            let caps = [u32::MAX; CAPS];
            add_thread(
                program,
                haystack,
                &mut clist,
                &mut clist_len,
                &mut cvisited,
                program.start(),
                pos,
                pos as u32,
                caps,
                0,
            )
            .ok()?;
        }
    }

    if matched {
        Some((match_start, match_end, match_caps))
    } else {
        None
    }
}

/// Adds the epsilon closure of `pc` to `list`.
///
/// The closure follows jumps, splits, capture markers and assertions. It is
/// recursive, but the depth is bounded by the number of instructions and by
/// the visited set, which stops a loop from being followed twice.
#[allow(clippy::too_many_arguments)]
fn add_thread<const CAPS: usize>(
    program: &Program,
    haystack: &[u8],
    list: &mut [Thread<CAPS>; MAX_THREADS],
    len: &mut usize,
    visited: &mut [u64; VISIT_WORDS],
    pc: u16,
    pos: usize,
    start: u32,
    caps: [u32; CAPS],
    depth: usize,
) -> Result<()> {
    if depth > MAX_CLOSURE_DEPTH {
        return Err(RegexError::ProgramTooLarge { limit: MAX_INST });
    }
    if visited[pc as usize / 64] & (1u64 << (pc as usize % 64)) != 0 {
        return Ok(());
    }
    visited[pc as usize / 64] |= 1u64 << (pc as usize % 64);

    let inst = program.inst(pc);
    match inst.op {
        Op::Char | Op::Match => {
            if *len == MAX_THREADS {
                return Err(RegexError::ThreadLimitExceeded { limit: MAX_THREADS });
            }
            list[*len] = Thread { pc, start, caps };
            *len += 1;
        }
        Op::Assert => {
            if assert_holds(inst.x, haystack, pos) {
                add_thread(
                    program,
                    haystack,
                    list,
                    len,
                    visited,
                    inst.y,
                    pos,
                    start,
                    caps,
                    depth + 1,
                )?;
            }
        }
        Op::Save => {
            let mut updated = caps;
            let slot = inst.x as usize;
            if slot < CAPS {
                updated[slot] = pos as u32;
            }
            add_thread(
                program,
                haystack,
                list,
                len,
                visited,
                inst.y,
                pos,
                start,
                updated,
                depth + 1,
            )?;
        }
        Op::Split => {
            // The preferred branch is explored first, so it keeps priority.
            add_thread(
                program,
                haystack,
                list,
                len,
                visited,
                inst.x,
                pos,
                start,
                caps,
                depth + 1,
            )?;
            add_thread(
                program,
                haystack,
                list,
                len,
                visited,
                inst.y,
                pos,
                start,
                caps,
                depth + 1,
            )?;
        }
    }
    Ok(())
}

/// Whether the zero-width assertion `kind` holds at `pos`.
fn assert_holds(kind: u16, haystack: &[u8], pos: usize) -> bool {
    let kind = match kind {
        0 => AssertKind::StartText,
        1 => AssertKind::EndText,
        2 => AssertKind::StartLine,
        3 => AssertKind::EndLine,
        4 => AssertKind::WordBoundary,
        _ => AssertKind::NotWordBoundary,
    };
    match kind {
        AssertKind::StartText => pos == 0,
        AssertKind::EndText => pos == haystack.len(),
        AssertKind::StartLine => pos == 0 || haystack[pos - 1] == b'\n',
        AssertKind::EndLine => pos == haystack.len() || haystack[pos] == b'\n',
        AssertKind::WordBoundary => {
            is_word(prev_byte(haystack, pos)) != is_word(next_byte(haystack, pos))
        }
        AssertKind::NotWordBoundary => {
            is_word(prev_byte(haystack, pos)) == is_word(next_byte(haystack, pos))
        }
    }
}

fn prev_byte(haystack: &[u8], pos: usize) -> u8 {
    if pos == 0 {
        0
    } else {
        haystack[pos - 1]
    }
}

fn next_byte(haystack: &[u8], pos: usize) -> u8 {
    if pos < haystack.len() {
        haystack[pos]
    } else {
        0
    }
}

/// Whether `byte` is a word byte (ASCII letters, digits and underscore).
fn is_word(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}
