//! Pattern parsing: text dialect to lowered syntax tree.
//!
//! The dialect is deliberately small. Every construct that makes matching
//! unpredictable — look-around, backreferences, recursion, nested character
//! class operations — is rejected with an explicit error instead of being
//! silently reinterpreted.
//!
//! Grammar (informal):
//!
//! ```text
//! alternation := concat ('|' concat)*
//! concat      := repeat*
//! repeat      := atom quantifier?
//! quantifier  := '*' | '+' | '?' | '{n}' | '{n,}' | '{n,m}' ('?')?
//! atom        := '(' group ')' | '[' class ']' | '.' | '^' | '$' | escape | literal
//! ```

use crate::class::{set_add, set_add_range, set_negate, ByteSet, ClassTable, EMPTY_SET};
use crate::error::{RegexError, Result};
use crate::hir::{AssertKind, HirArena, HirNode};
use crate::Flags;

/// The largest repetition bound a pattern may use.
pub const MAX_REPEAT: u16 = 255;

/// The deepest group nesting a pattern may use.
pub const MAX_DEPTH: u32 = 32;

/// The error for an escape sequence that names nothing.
///
/// The engine refuses to reinterpret an unknown escape of a letter as a
/// literal: a typo must be reported, not silently matched.
fn unknown_escape(position: usize) -> RegexError {
    RegexError::Syntax {
        position,
        message: "unknown escape sequence",
    }
}

/// Parses `pattern` into a lowered syntax tree and its byte set table.
///
/// Returns the arena, the byte set table and the index of the root node.
pub(crate) fn parse(pattern: &str, flags: Flags) -> Result<(HirArena, ClassTable, u16)> {
    let mut parser = Parser {
        input: pattern.as_bytes(),
        pos: 0,
        arena: HirArena::new(),
        classes: ClassTable::new(),
        group_count: 0,
        depth: 0,
    };
    let root = parser.parse_alternation(flags)?;
    if parser.pos != parser.input.len() {
        return Err(RegexError::Syntax {
            position: parser.pos,
            message: "unbalanced ')'",
        });
    }
    Ok((parser.arena, parser.classes, root))
}

struct Parser<'a> {
    input: &'a [u8],
    pos: usize,
    arena: HirArena,
    classes: ClassTable,
    group_count: u16,
    depth: u32,
}

impl<'a> Parser<'a> {
    fn peek(&self) -> Option<u8> {
        self.input.get(self.pos).copied()
    }

    fn bump(&mut self) -> Option<u8> {
        let byte = self.peek();
        if byte.is_some() {
            self.pos += 1;
        }
        byte
    }

    fn eat(&mut self, byte: u8) -> bool {
        if self.peek() == Some(byte) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn syntax<T>(&self, message: &'static str) -> Result<T> {
        Err(RegexError::Syntax {
            position: self.pos,
            message,
        })
    }

    fn push(&mut self, node: HirNode) -> Result<u16> {
        self.arena.push(node)
    }

    fn empty(&mut self) -> Result<u16> {
        self.push(HirNode::Empty)
    }

    fn literal(&mut self, byte: u8, flags: Flags) -> Result<u16> {
        if flags.case_insensitive && byte.is_ascii_alphabetic() {
            let mut set = EMPTY_SET;
            set_add(&mut set, byte.to_ascii_lowercase());
            set_add(&mut set, byte.to_ascii_uppercase());
            let id = self.classes.add(&set)?;
            return self.push(HirNode::Class(id));
        }
        self.push(HirNode::LiteralByte(byte))
    }

    /// Parses `concat ('|' concat)*`.
    fn parse_alternation(&mut self, flags: Flags) -> Result<u16> {
        let mut left = self.parse_concat(flags)?;
        while self.eat(b'|') {
            let right = self.parse_concat(flags)?;
            left = self.push(HirNode::Alternate(left, right))?;
        }
        Ok(left)
    }

    /// Parses `repeat*`.
    fn parse_concat(&mut self, mut flags: Flags) -> Result<u16> {
        let mut result: Option<u16> = None;
        loop {
            // `(?flags)` switches the flags for the rest of this group.
            if self.try_parse_flag_directive(&mut flags)? {
                continue;
            }
            match self.peek() {
                None | Some(b'|') | Some(b')') => break,
                _ => {}
            }
            let piece = self.parse_repeat(flags)?;
            result = Some(match result {
                None => piece,
                Some(previous) => self.push(HirNode::Concat(previous, piece))?,
            });
        }
        Ok(match result {
            Some(node) => node,
            None => self.empty()?,
        })
    }

    /// Consumes a `(?flags)` directive when one is present.
    ///
    /// Returns `true` when a directive was consumed.
    fn try_parse_flag_directive(&mut self, flags: &mut Flags) -> Result<bool> {
        if self.input.get(self.pos) != Some(&b'(') || self.input.get(self.pos + 1) != Some(&b'?') {
            return Ok(false);
        }
        let mut cursor = self.pos + 2;
        let mut negate = false;
        let mut touched = false;
        while let Some(&byte) = self.input.get(cursor) {
            match byte {
                b'-' if !negate => {
                    negate = true;
                    cursor += 1;
                }
                b'i' => {
                    flags.case_insensitive = !negate;
                    cursor += 1;
                    touched = true;
                }
                b'm' => {
                    flags.multi_line = !negate;
                    cursor += 1;
                    touched = true;
                }
                b's' => {
                    flags.dot_matches_newline = !negate;
                    cursor += 1;
                    touched = true;
                }
                _ => break,
            }
        }
        if !touched || self.input.get(cursor) != Some(&b')') {
            return Ok(false);
        }
        self.pos = cursor + 1;
        Ok(true)
    }

    /// Parses `atom quantifier?`.
    fn parse_repeat(&mut self, flags: Flags) -> Result<u16> {
        let start = self.pos;
        let atom = self.parse_atom(flags)?;
        let (min, max) = match self.peek() {
            Some(b'*') => {
                self.pos += 1;
                (0u16, None)
            }
            Some(b'+') => {
                self.pos += 1;
                (1u16, None)
            }
            Some(b'?') => {
                self.pos += 1;
                (0u16, Some(1u16))
            }
            Some(b'{') => match self.parse_counted()? {
                Some(bounds) => bounds,
                None => {
                    // A '{' that does not open a valid bound is a literal.
                    self.pos = start;
                    return self.parse_atom(flags);
                }
            },
            _ => return Ok(atom),
        };
        if self.arena.get(atom) == HirNode::Empty {
            return Err(RegexError::Syntax {
                position: start,
                message: "nothing to repeat",
            });
        }
        let greedy = !self.eat(b'?');
        if self.matches_quantifier_start() {
            return Err(RegexError::Syntax {
                position: self.pos,
                message: "repeated quantifier",
            });
        }
        self.push(HirNode::Repeat {
            child: atom,
            min,
            max,
            greedy,
        })
    }

    fn matches_quantifier_start(&self) -> bool {
        matches!(self.peek(), Some(b'*') | Some(b'+') | Some(b'?'))
    }

    /// Parses `{n}`, `{n,}` or `{n,m}`.
    ///
    /// Returns `Ok(None)` when the brace does not open a valid bound, which
    /// makes the caller treat the `{` as a literal.
    fn parse_counted(&mut self) -> Result<Option<(u16, Option<u16>)>> {
        let start = self.pos;
        debug_assert_eq!(self.peek(), Some(b'{'));
        self.pos += 1;
        let min = match self.parse_number() {
            Some(value) => value,
            None => {
                self.pos = start;
                return Ok(None);
            }
        };
        let max = if self.eat(b',') {
            // `{n,}` has no upper bound.
            self.parse_number()
        } else {
            Some(min)
        };
        if !self.eat(b'}') {
            self.pos = start;
            return Ok(None);
        }
        if min > MAX_REPEAT {
            return Err(RegexError::RepetitionTooLarge {
                limit: MAX_REPEAT as usize,
            });
        }
        if let Some(max) = max {
            if max > MAX_REPEAT {
                return Err(RegexError::RepetitionTooLarge {
                    limit: MAX_REPEAT as usize,
                });
            }
            if max < min {
                return Err(RegexError::InvalidClass {
                    position: start,
                    message: "repetition upper bound is smaller than the lower bound",
                });
            }
        }
        Ok(Some((min, max)))
    }

    fn parse_number(&mut self) -> Option<u16> {
        let start = self.pos;
        let mut value: u32 = 0;
        while let Some(byte) = self.peek() {
            if !byte.is_ascii_digit() {
                break;
            }
            value = value
                .saturating_mul(10)
                .saturating_add((byte - b'0') as u32);
            self.pos += 1;
        }
        if self.pos == start {
            None
        } else {
            Some(value.min(u16::MAX as u32) as u16)
        }
    }

    /// Parses a single atom.
    fn parse_atom(&mut self, flags: Flags) -> Result<u16> {
        let byte = match self.bump() {
            Some(byte) => byte,
            None => return self.syntax("unexpected end of pattern"),
        };
        match byte {
            b'(' => self.parse_group(flags),
            b'[' => self.parse_class(flags),
            b'.' => {
                // `.` matches every byte, except a newline unless the
                // dot-all flag is set.
                let mut set = EMPTY_SET;
                for byte in 0u16..=255 {
                    if flags.dot_matches_newline || byte != b'\n' as u16 {
                        set_add(&mut set, byte as u8);
                    }
                }
                let id = self.classes.add(&set)?;
                self.push(HirNode::Class(id))
            }
            b'^' => {
                let kind = if flags.multi_line {
                    AssertKind::StartLine
                } else {
                    AssertKind::StartText
                };
                self.push(HirNode::Assert(kind))
            }
            b'$' => {
                let kind = if flags.multi_line {
                    AssertKind::EndLine
                } else {
                    AssertKind::EndText
                };
                self.push(HirNode::Assert(kind))
            }
            b'\\' => self.parse_escape(flags, false),
            b'*' | b'+' | b'?' => Err(RegexError::Syntax {
                position: self.pos - 1,
                message: "nothing to repeat",
            }),
            b')' => Err(RegexError::Syntax {
                position: self.pos - 1,
                message: "unbalanced ')'",
            }),
            b']' | b'}' => self.literal(byte, flags),
            other => self.literal(other, flags),
        }
    }

    /// Parses a group: `(...)`, `(?:...)`, `(?flags)` or `(?flags:...)`.
    fn parse_group(&mut self, flags: Flags) -> Result<u16> {
        self.depth += 1;
        if self.depth > MAX_DEPTH {
            return Err(RegexError::NestingTooDeep {
                limit: MAX_DEPTH as usize,
            });
        }
        let result = self.parse_group_inner(flags);
        self.depth -= 1;
        result
    }

    fn parse_group_inner(&mut self, flags: Flags) -> Result<u16> {
        if self.peek() == Some(b'?') {
            self.pos += 1;
            match self.peek() {
                Some(b':') => {
                    self.pos += 1;
                    let inner = self.parse_alternation(flags)?;
                    self.expect_close()?;
                    return Ok(inner);
                }
                Some(b'=') | Some(b'!') => {
                    return Err(RegexError::Unsupported {
                        position: self.pos,
                        feature: "look-ahead",
                    })
                }
                Some(b'<') => {
                    return Err(RegexError::Unsupported {
                        position: self.pos,
                        feature: "look-behind",
                    })
                }
                Some(b'#') => {
                    return Err(RegexError::Unsupported {
                        position: self.pos,
                        feature: "comment group",
                    })
                }
                Some(b'P') => {
                    return Err(RegexError::Unsupported {
                        position: self.pos,
                        feature: "named group",
                    })
                }
                _ => {}
            }
            // Flag setting: `(?flags:...)`.
            let mut new_flags = flags;
            let mut negate = false;
            loop {
                match self.peek() {
                    Some(b'-') => {
                        negate = true;
                        self.pos += 1;
                    }
                    Some(b'i') => {
                        self.pos += 1;
                        new_flags.case_insensitive = !negate;
                    }
                    Some(b'm') => {
                        self.pos += 1;
                        new_flags.multi_line = !negate;
                    }
                    Some(b's') => {
                        self.pos += 1;
                        new_flags.dot_matches_newline = !negate;
                    }
                    Some(b'U') | Some(b'x') => {
                        return Err(RegexError::Unsupported {
                            position: self.pos,
                            feature: "flag",
                        })
                    }
                    _ => break,
                }
            }
            if self.eat(b':') {
                let inner = self.parse_alternation(new_flags)?;
                self.expect_close()?;
                return Ok(inner);
            }
            return self.syntax("unrecognized group syntax");
        }
        // Capturing group.
        if self.group_count as usize >= crate::MAX_CAPTURE_GROUPS {
            return Err(RegexError::TooManyGroups {
                limit: crate::MAX_CAPTURE_GROUPS,
            });
        }
        let index = self.group_count;
        self.group_count += 1;
        let inner = self.parse_alternation(flags)?;
        self.expect_close()?;
        self.push(HirNode::Capture {
            child: inner,
            index,
        })
    }

    fn expect_close(&mut self) -> Result<()> {
        if self.eat(b')') {
            Ok(())
        } else {
            Err(RegexError::Syntax {
                position: self.pos,
                message: "unclosed group",
            })
        }
    }

    /// Parses a character class.
    fn parse_class(&mut self, flags: Flags) -> Result<u16> {
        let start = self.pos - 1;
        let negated = self.eat(b'^');
        let mut set = EMPTY_SET;
        let mut first = true;
        loop {
            let byte = match self.peek() {
                Some(byte) => byte,
                None => {
                    return Err(RegexError::InvalidClass {
                        position: start,
                        message: "unclosed character class",
                    })
                }
            };
            if byte == b']' && !first {
                self.pos += 1;
                break;
            }
            first = false;
            if byte == b']' {
                return Err(RegexError::InvalidClass {
                    position: start,
                    message: "empty character class",
                });
            }
            // A shorthand class inside a class (`[\d\s]`) contributes its
            // whole set.
            if byte == b'\\' {
                let escape_pos = self.pos;
                self.pos += 1;
                match self.peek() {
                    Some(b'd') | Some(b'D') | Some(b'w') | Some(b'W') | Some(b's') | Some(b'S') => {
                        let shorthand = self.parse_shorthand(escape_pos)?;
                        let mut merged = set;
                        merge(&mut merged, &shorthand);
                        set = merged;
                        continue;
                    }
                    _ => {}
                }
                let value = self.parse_escape_value(escape_pos, flags)?;
                self.add_class_byte(&mut set, value, flags);
                continue;
            }
            self.pos += 1;
            // A range?
            if self.peek() == Some(b'-') && self.input.get(self.pos + 1) != Some(&b']') {
                if self.input.get(self.pos + 1).is_none() {
                    return Err(RegexError::InvalidClass {
                        position: start,
                        message: "unclosed character class",
                    });
                }
                self.pos += 1;
                let range_start = byte;
                let range_end = match self.peek() {
                    Some(b'\\') => {
                        let escape_pos = self.pos;
                        self.pos += 1;
                        self.parse_escape_value(escape_pos, flags)?
                    }
                    Some(other) => {
                        self.pos += 1;
                        other
                    }
                    None => {
                        return Err(RegexError::InvalidClass {
                            position: start,
                            message: "unclosed character class",
                        })
                    }
                };
                if range_end < range_start {
                    return Err(RegexError::InvalidClass {
                        position: start,
                        message: "character range is reversed",
                    });
                }
                for value in range_start..=range_end {
                    self.add_class_byte(&mut set, value, flags);
                }
                continue;
            }
            self.add_class_byte(&mut set, byte, flags);
        }
        if negated {
            set_negate(&mut set);
        }
        let id = self.classes.add(&set)?;
        self.push(HirNode::Class(id))
    }

    fn add_class_byte(&self, set: &mut ByteSet, byte: u8, flags: Flags) {
        set_add(set, byte);
        if flags.case_insensitive && byte.is_ascii_alphabetic() {
            set_add(set, byte.to_ascii_lowercase());
            set_add(set, byte.to_ascii_uppercase());
        }
    }

    /// Parses an escape sequence that stands for a single byte.
    fn parse_escape(&mut self, flags: Flags, in_class: bool) -> Result<u16> {
        let escape_pos = self.pos - 1;
        match self.peek() {
            Some(b'd') | Some(b'D') | Some(b'w') | Some(b'W') | Some(b's') | Some(b'S') => {
                let set = self.parse_shorthand(escape_pos)?;
                let id = self.classes.add(&set)?;
                self.push(HirNode::Class(id))
            }
            Some(b'b') | Some(b'B') if !in_class => {
                self.pos += 1;
                let kind = if self.input[escape_pos + 1] == b'b' {
                    AssertKind::WordBoundary
                } else {
                    AssertKind::NotWordBoundary
                };
                self.push(HirNode::Assert(kind))
            }
            _ => {
                let value = self.parse_escape_value(escape_pos, flags)?;
                self.literal(value, flags)
            }
        }
    }

    /// Parses the body of a shorthand class (`\d`, `\w`, `\s`).
    fn parse_shorthand(&mut self, escape_pos: usize) -> Result<ByteSet> {
        let byte = self.bump().ok_or(RegexError::Syntax {
            position: escape_pos,
            message: "trailing backslash",
        })?;
        let mut set = EMPTY_SET;
        match byte {
            b'd' => set_add_range(&mut set, b'0', b'9'),
            b'D' => {
                set_add_range(&mut set, b'0', b'9');
                set_negate(&mut set);
            }
            b'w' => {
                set_add_range(&mut set, b'0', b'9');
                set_add_range(&mut set, b'A', b'Z');
                set_add_range(&mut set, b'a', b'z');
                set_add(&mut set, b'_');
            }
            b'W' => {
                set_add_range(&mut set, b'0', b'9');
                set_add_range(&mut set, b'A', b'Z');
                set_add_range(&mut set, b'a', b'z');
                set_add(&mut set, b'_');
                set_negate(&mut set);
            }
            b's' => {
                set_add(&mut set, b' ');
                set_add(&mut set, b'\t');
                set_add(&mut set, b'\n');
                set_add(&mut set, b'\r');
                set_add(&mut set, 0x0b);
                set_add(&mut set, 0x0c);
            }
            b'S' => {
                set_add(&mut set, b' ');
                set_add(&mut set, b'\t');
                set_add(&mut set, b'\n');
                set_add(&mut set, b'\r');
                set_add(&mut set, 0x0b);
                set_add(&mut set, 0x0c);
                set_negate(&mut set);
            }
            _ => return Err(unknown_escape(escape_pos)),
        }
        Ok(set)
    }

    /// Parses the body of an escape sequence that denotes a single byte.
    fn parse_escape_value(&mut self, escape_pos: usize, _flags: Flags) -> Result<u8> {
        let byte = self.bump().ok_or(RegexError::Syntax {
            position: escape_pos,
            message: "trailing backslash",
        })?;
        let value = match byte {
            b'n' => b'\n',
            b'r' => b'\r',
            b't' => b'\t',
            b'f' => 0x0c,
            b'v' => 0x0b,
            b'a' => 0x07,
            b'e' => 0x1b,
            b'b' => 0x08,
            b'0' => 0,
            b'x' => {
                let high = self.parse_hex_digit(escape_pos)?;
                let low = self.parse_hex_digit(escape_pos)?;
                high << 4 | low
            }
            b'u' => {
                return Err(RegexError::Unsupported {
                    position: escape_pos,
                    feature: "\\u escape",
                })
            }
            b'p' | b'P' => {
                return Err(RegexError::Unsupported {
                    position: escape_pos,
                    feature: "Unicode property class",
                })
            }
            b'1'..=b'9' => {
                return Err(RegexError::Unsupported {
                    position: escape_pos,
                    feature: "backreference",
                })
            }
            // An unknown escape of a letter is a mistake, not a literal. The
            // engine refuses to reinterpret it silently; an escape of a
            // punctuation mark (`\.`, `\-`, `\/`) still stands for itself.
            other if other.is_ascii_alphanumeric() => return Err(unknown_escape(escape_pos)),
            other => other,
        };
        Ok(value)
    }

    fn parse_hex_digit(&mut self, escape_pos: usize) -> Result<u8> {
        let byte = self.bump().ok_or(RegexError::Syntax {
            position: escape_pos,
            message: "truncated hexadecimal escape",
        })?;
        match byte {
            b'0'..=b'9' => Ok(byte - b'0'),
            b'a'..=b'f' => Ok(byte - b'a' + 10),
            b'A'..=b'F' => Ok(byte - b'A' + 10),
            _ => Err(RegexError::Syntax {
                position: escape_pos,
                message: "invalid hexadecimal escape",
            }),
        }
    }
}

/// Merges `source` into `target`.
fn merge(target: &mut ByteSet, source: &ByteSet) {
    for (index, word) in source.iter().enumerate() {
        target[index] |= *word;
    }
}
