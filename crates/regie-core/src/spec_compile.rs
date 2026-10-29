//! Compilation of structured specifications into automata.
//!
//! This is the path the intent layer uses: the model chooses an enumerated
//! option, and this module turns that option into an automaton. No pattern
//! text is involved, so a model can never express a construct that was not
//! explicitly designed here.

use crate::class::{set_add, set_add_range, ByteSet, ClassTable, EMPTY_SET};
use crate::error::Result;
use crate::hir::{AssertKind, HirArena, HirNode};
use crate::smallvec::SmallVec;
use crate::spec::{
    BoundaryPolicy, CandidateSpec, CharsetPolicy, ExtractionKind, FilterAction, PatternTemplate,
    PatternTerm, Strictness, TermKind, ValidationKind,
};

/// Builds the syntax tree of a structured specification.
///
/// Returns the arena, the byte set table and the index of the root node.
pub(crate) fn build(spec: &CandidateSpec) -> Result<(HirArena, ClassTable, u16)> {
    let mut builder = Builder::new();
    builder.root = match spec {
        CandidateSpec::Validate {
            kind,
            strictness,
            charset,
            max_len,
        } => builder.validation(*kind, *strictness, *charset, *max_len)?,
        CandidateSpec::Extract {
            kind,
            boundary,
            capture,
        } => builder.extraction(*kind, *boundary, *capture)?,
        CandidateSpec::Filter { action, terms } => builder.filter(*action, terms)?,
        CandidateSpec::Custom { template, params } => builder.custom(template, params)?,
    };
    Ok((builder.arena, builder.classes, builder.root))
}

/// A builder for syntax tree nodes.
///
/// Every method takes `&mut self` and returns a node index, so a builder
/// call never nests inside another one.
struct Builder {
    arena: HirArena,
    classes: ClassTable,
    groups: u16,
    root: u16,
}

impl Builder {
    fn new() -> Builder {
        Builder {
            arena: HirArena::new(),
            classes: ClassTable::new(),
            groups: 0,
            root: 0,
        }
    }

    fn node(&mut self, node: HirNode) -> Result<u16> {
        self.arena.push(node)
    }

    fn class_node(&mut self, set: &ByteSet) -> Result<u16> {
        let id = self.classes.add(set)?;
        self.node(HirNode::Class(id))
    }

    /// A class holding exactly the given bytes.
    fn class_set(&mut self, bytes: &[u8]) -> Result<u16> {
        let mut set = EMPTY_SET;
        for byte in bytes {
            set_add(&mut set, *byte);
        }
        self.class_node(&set)
    }

    /// A class holding the given bytes and, when `fold_case` is set, their
    /// ASCII case counterparts.
    fn class_set_folded(&mut self, bytes: &[u8], fold_case: bool) -> Result<u16> {
        let mut set = EMPTY_SET;
        for byte in bytes {
            set_add(&mut set, *byte);
            if fold_case && byte.is_ascii_alphabetic() {
                set_add(&mut set, byte.to_ascii_lowercase());
                set_add(&mut set, byte.to_ascii_uppercase());
            }
        }
        self.class_node(&set)
    }

    /// A class holding every byte except the given ones.
    fn class_except(&mut self, excluded: &[u8]) -> Result<u16> {
        let mut set = EMPTY_SET;
        for byte in excluded {
            set_add(&mut set, *byte);
        }
        crate::class::set_negate(&mut set);
        self.class_node(&set)
    }

    fn range(&mut self, start: u8, end: u8) -> Result<u16> {
        let mut set = EMPTY_SET;
        set_add_range(&mut set, start, end);
        self.class_node(&set)
    }

    fn digits(&mut self) -> Result<u16> {
        self.range(b'0', b'9')
    }

    /// An octet of an IPv4 address: a number from 0 to 255.
    ///
    /// The alternatives are tried in order, and the matcher keeps the first
    /// one that leads to an overall match: `25[0-5]`, `2[0-4][0-9]`,
    /// `1[0-9][0-9]`, `[0-9][0-9]` and `[0-9]`.  An octet such as `999` is
    /// refused, because none of the alternatives covers it.
    fn ipv4_octet(&mut self) -> Result<u16> {
        let two_five = self.chars("25")?;
        let zero_five = self.range(b'0', b'5')?;
        let high = self.seq(&[two_five, zero_five])?;

        let two = self.chars("2")?;
        let zero_four = self.range(b'0', b'4')?;
        let digit = self.digits()?;
        let mid = self.seq(&[two, zero_four, digit])?;

        let one = self.chars("1")?;
        let digit_a = self.digits()?;
        let digit_b = self.digits()?;
        let low = self.seq(&[one, digit_a, digit_b])?;

        let digit_a = self.digits()?;
        let digit_b = self.digits()?;
        let two_digits = self.seq(&[digit_a, digit_b])?;

        let one_digit = self.digits()?;

        let mut node = self.alt(high, mid)?;
        node = self.alt(node, low)?;
        node = self.alt(node, two_digits)?;
        node = self.alt(node, one_digit)?;
        Ok(node)
    }

    /// A month of the calendar: `01` to `12`.
    fn month(&mut self) -> Result<u16> {
        let zero = self.chars("0")?;
        let one_to_nine = self.range(b'1', b'9')?;
        let first = self.seq(&[zero, one_to_nine])?;
        let one = self.chars("1")?;
        let zero_to_two = self.range(b'0', b'2')?;
        let second = self.seq(&[one, zero_to_two])?;
        self.alt(first, second)
    }

    /// A day of the month: `01` to `31`.
    ///
    /// The day of February is not known here: the family checks the shape of
    /// a date, not the calendar.
    fn day(&mut self) -> Result<u16> {
        let zero = self.chars("0")?;
        let one_to_nine = self.range(b'1', b'9')?;
        let first = self.seq(&[zero, one_to_nine])?;
        let one_or_two = self.class_set(b"12")?;
        let zero_to_nine = self.range(b'0', b'9')?;
        let second = self.seq(&[one_or_two, zero_to_nine])?;
        let mut node = self.alt(first, second)?;
        let three = self.chars("3")?;
        let zero_or_one = self.class_set(b"01")?;
        let third = self.seq(&[three, zero_or_one])?;
        node = self.alt(node, third)?;
        Ok(node)
    }

    /// An hour of the day: `00` to `23`.
    fn hour(&mut self) -> Result<u16> {
        let zero = self.chars("0")?;
        let digit = self.digits()?;
        let first = self.seq(&[zero, digit])?;
        let one = self.chars("1")?;
        let digit = self.digits()?;
        let second = self.seq(&[one, digit])?;
        let two = self.chars("2")?;
        let zero_to_three = self.range(b'0', b'3')?;
        let third = self.seq(&[two, zero_to_three])?;
        let mut node = self.alt(first, second)?;
        node = self.alt(node, third)?;
        Ok(node)
    }

    /// A minute or a second: `00` to `59`.
    fn minute(&mut self) -> Result<u16> {
        let zero_to_five = self.range(b'0', b'5')?;
        let digit = self.digits()?;
        self.seq(&[zero_to_five, digit])
    }

    fn letters(&mut self) -> Result<u16> {
        let mut set = EMPTY_SET;
        set_add_range(&mut set, b'A', b'Z');
        set_add_range(&mut set, b'a', b'z');
        self.class_node(&set)
    }

    fn alnum(&mut self) -> Result<u16> {
        let mut set = EMPTY_SET;
        set_add_range(&mut set, b'0', b'9');
        set_add_range(&mut set, b'A', b'Z');
        set_add_range(&mut set, b'a', b'z');
        self.class_node(&set)
    }

    fn word(&mut self) -> Result<u16> {
        let mut set = EMPTY_SET;
        set_add_range(&mut set, b'0', b'9');
        set_add_range(&mut set, b'A', b'Z');
        set_add_range(&mut set, b'a', b'z');
        set_add(&mut set, b'_');
        self.class_node(&set)
    }

    fn hex(&mut self) -> Result<u16> {
        let mut set = EMPTY_SET;
        set_add_range(&mut set, b'0', b'9');
        set_add_range(&mut set, b'A', b'F');
        set_add_range(&mut set, b'a', b'f');
        self.class_node(&set)
    }

    /// A sequence of literal bytes.
    fn chars(&mut self, text: &str) -> Result<u16> {
        let mut parts = [0u16; 32];
        let mut count = 0usize;
        for byte in text.bytes() {
            if count == parts.len() {
                break;
            }
            parts[count] = self.class_set(&[byte])?;
            count += 1;
        }
        self.seq(&parts[..count])
    }

    /// A sequence of nodes.
    fn seq(&mut self, parts: &[u16]) -> Result<u16> {
        let mut result = self.node(HirNode::Empty)?;
        for part in parts {
            result = self.node(HirNode::Concat(result, *part))?;
        }
        Ok(result)
    }

    fn alt(&mut self, left: u16, right: u16) -> Result<u16> {
        self.node(HirNode::Alternate(left, right))
    }

    fn rep(&mut self, child: u16, min: u16, max: Option<u16>) -> Result<u16> {
        self.node(HirNode::Repeat {
            child,
            min,
            max,
            greedy: true,
        })
    }

    fn rep_lazy(&mut self, child: u16, min: u16, max: Option<u16>) -> Result<u16> {
        self.node(HirNode::Repeat {
            child,
            min,
            max,
            greedy: false,
        })
    }

    fn optional(&mut self, child: u16) -> Result<u16> {
        self.rep(child, 0, Some(1))
    }

    fn group(&mut self, child: u16) -> Result<u16> {
        let index = self.groups;
        self.groups += 1;
        self.node(HirNode::Capture { child, index })
    }

    fn assert(&mut self, kind: AssertKind) -> Result<u16> {
        self.node(HirNode::Assert(kind))
    }

    /// A class for the given policy.
    fn policy(&mut self, policy: CharsetPolicy) -> Result<u16> {
        match policy {
            CharsetPolicy::Digits => self.digits(),
            CharsetPolicy::Letters => self.letters(),
            CharsetPolicy::Alphanumeric => self.alnum(),
            CharsetPolicy::Word => self.word(),
            CharsetPolicy::PrintableAscii => self.range(0x20, 0x7e),
            CharsetPolicy::Any => {
                let mut set = EMPTY_SET;
                set_add_range(&mut set, 0, 255);
                self.class_node(&set)
            }
        }
    }

    /// Builds the automaton of a validation family.
    fn validation(
        &mut self,
        kind: ValidationKind,
        strictness: Strictness,
        charset: CharsetPolicy,
        max_len: Option<u16>,
    ) -> Result<u16> {
        let body = self.validation_body(kind, strictness, charset, max_len)?;
        // A validation describes a whole value, so it is anchored.
        let start = self.assert(AssertKind::StartText)?;
        let end = self.assert(AssertKind::EndText)?;
        let parts = [start, body, end];
        self.seq(&parts)
    }

    fn validation_body(
        &mut self,
        kind: ValidationKind,
        strictness: Strictness,
        _charset: CharsetPolicy,
        _max_len: Option<u16>,
    ) -> Result<u16> {
        match kind {
            ValidationKind::Email => {
                let local = self.class_set(
                    b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789._%+-",
                )?;
                let local = self.rep(local, 1, None)?;
                let at = self.chars("@")?;
                let domain = self.class_set(
                    b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789.-",
                )?;
                let domain = self.rep(domain, 1, None)?;
                let dot = self.chars(".")?;
                let tld = self.letters()?;
                let tld = self.rep(tld, 2, None)?;
                let parts = [local, at, domain, dot, tld];
                self.seq(&parts)
            }
            ValidationKind::Url => {
                let scheme = self.chars("http")?;
                let secure = self.chars("s")?;
                let secure = self.optional(secure)?;
                let sep = self.chars("://")?;
                let host = self.class_set(
                    b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789.-",
                )?;
                let host = self.rep(host, 1, None)?;
                let port = self.digits()?;
                let port = self.rep(port, 1, None)?;
                let colon = self.chars(":")?;
                let port = self.seq(&[colon, port])?;
                let port = self.optional(port)?;
                let path = self.class_set(
                    b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789-._~:/?#[]@!$&'()*+,;=%",
                )?;
                let path = self.rep(path, 0, None)?;
                let path = self.optional(path)?;
                let parts = [scheme, secure, sep, host, port, path];
                self.seq(&parts)
            }
            ValidationKind::Uri => {
                let scheme = self.class_set(
                    b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789+-.",
                )?;
                let scheme = self.rep(scheme, 1, None)?;
                let colon = self.chars(":")?;
                let rest = self.class_set(
                    b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789-._~:/?#[]@!$&'()*+,;=%",
                )?;
                let rest = self.rep(rest, 1, None)?;
                let parts = [scheme, colon, rest];
                self.seq(&parts)
            }
            ValidationKind::DomainName => {
                // A label is letters and digits, with dashes between them: it
                // cannot start or end with a dash.  A domain has at least two
                // labels.
                let alnum = self
                    .class_set(b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789")?;
                let run = self.rep(alnum, 1, None)?;
                let dash = self.chars("-")?;
                let inner = self.seq(&[dash, run])?;
                let inner = self.rep(inner, 0, None)?;
                let label = self.seq(&[run, inner])?;

                // A domain is a label and at least one "dot label".
                let dot = self.chars(".")?;
                let part = self.seq(&[dot, label])?;
                let part = self.rep(part, 1, None)?;
                let parts = [label, part];
                self.seq(&parts)
            }
            ValidationKind::Ipv4 => {
                let octet = self.ipv4_octet()?;
                let dot = self.chars(".")?;
                let second = self.ipv4_octet()?;
                let third = self.ipv4_octet()?;
                let fourth = self.ipv4_octet()?;
                let parts = [octet, dot, second, dot, third, dot, fourth];
                self.seq(&parts)
            }
            ValidationKind::Ipv6 => {
                let hex = self.hex()?;
                let group = self.rep(hex, 1, Some(4))?;
                let colon = self.chars(":")?;
                let mut parts = [0u16; 8];
                parts[0] = group;
                for slot in parts.iter_mut().skip(1) {
                    let piece = self.seq(&[colon, group])?;
                    *slot = piece;
                }
                self.seq(&parts)
            }
            ValidationKind::MacAddress => {
                let hex = self.hex()?;
                let pair = self.rep(hex, 2, Some(2))?;
                let sep = self.class_set(b":-")?;
                let mut parts = [0u16; 6];
                parts[0] = pair;
                for slot in parts.iter_mut().skip(1) {
                    let piece = self.seq(&[sep, pair])?;
                    *slot = piece;
                }
                self.seq(&parts)
            }
            ValidationKind::BrazilianCpf => {
                let digit = self.digits()?;
                if strictness == Strictness::Lenient {
                    let digits = self.rep(digit, 11, Some(11))?;
                    return Ok(digits);
                }
                let first = self.rep(digit, 3, Some(3))?;
                let dot = self.chars(".")?;
                let second = self.rep(digit, 3, Some(3))?;
                let third = self.rep(digit, 3, Some(3))?;
                let dash = self.chars("-")?;
                let check = self.rep(digit, 2, Some(2))?;
                let parts = [first, dot, second, dot, third, dash, check];
                let dotted = self.seq(&parts)?;
                if strictness == Strictness::Strict {
                    // The punctuated form only.
                    return Ok(dotted);
                }
                let compact = self.rep(digit, 11, Some(11))?;
                self.alt(dotted, compact)
            }
            ValidationKind::BrazilianCnpj => {
                let digit = self.digits()?;
                let first = self.rep(digit, 2, Some(2))?;
                let dot = self.chars(".")?;
                let second = self.rep(digit, 3, Some(3))?;
                let third = self.rep(digit, 3, Some(3))?;
                let slash = self.chars("/")?;
                let branch = self.rep(digit, 4, Some(4))?;
                let dash = self.chars("-")?;
                let check = self.rep(digit, 2, Some(2))?;
                let parts = [first, dot, second, dot, third, slash, branch, dash, check];
                let dotted = self.seq(&parts)?;
                if strictness == Strictness::Strict {
                    // The punctuated form only.
                    return Ok(dotted);
                }
                let compact = self.rep(digit, 14, Some(14))?;
                self.alt(dotted, compact)
            }
            ValidationKind::BrazilianPostalCode => {
                let digit = self.digits()?;
                let first = self.rep(digit, 5, Some(5))?;
                let dash = self.chars("-")?;
                let dash = self.optional(dash)?;
                let second = self.rep(digit, 3, Some(3))?;
                let parts = [first, dash, second];
                self.seq(&parts)
            }
            ValidationKind::BrazilianPhone => {
                let digit = self.digits()?;
                let plus = self.chars("+55")?;
                let space = self.chars(" ")?;
                let space = self.optional(space)?;
                let prefix = self.seq(&[plus, space])?;
                let prefix = self.optional(prefix)?;
                let area = self.rep(digit, 2, Some(2))?;
                let space = self.chars(" ")?;
                let space = self.optional(space)?;
                let middle = self.rep(digit, 4, Some(5))?;
                let dash = self.chars("-")?;
                let dash = self.optional(dash)?;
                let last = self.rep(digit, 4, Some(4))?;
                let parts = [prefix, area, space, middle, dash, last];
                self.seq(&parts)
            }
            ValidationKind::BrazilianLicensePlate => {
                // The Mercosul plate, `ABC1D23`, and the older one,
                // `ABC1234`.
                let upper = self.range(b'A', b'Z')?;
                let upper3 = self.rep(upper, 3, Some(3))?;
                let digit = self.digits()?;
                let digit1 = self.rep(digit, 1, Some(1))?;
                let letter = self.rep(upper, 1, Some(1))?;
                let digit2 = self.rep(digit, 2, Some(2))?;
                let modern = self.seq(&[upper3, digit1, letter, digit2])?;

                let digit = self.digits()?;
                let old = self.rep(digit, 4, Some(4))?;
                let old = self.seq(&[upper3, old])?;

                self.alt(modern, old)
            }
            ValidationKind::PortugueseNif => {
                let digit = self.digits()?;
                let nif = self.rep(digit, 9, Some(9))?;
                Ok(nif)
            }
            ValidationKind::UsSsn => {
                // The dashes are the usual way of writing a number, but they
                // are not part of it.
                let digit = self.digits()?;
                let first = self.rep(digit, 3, Some(3))?;
                let dash = self.chars("-")?;
                let dash = self.optional(dash)?;
                let second = self.rep(digit, 2, Some(2))?;
                let dash2 = self.chars("-")?;
                let dash2 = self.optional(dash2)?;
                let third = self.rep(digit, 4, Some(4))?;
                let parts = [first, dash, second, dash2, third];
                self.seq(&parts)
            }
            ValidationKind::Semver => {
                let digit = self.digits()?;
                let major = self.rep(digit, 1, None)?;
                let dot = self.chars(".")?;
                let minor = self.rep(digit, 1, None)?;
                let dot2 = self.chars(".")?;
                let patch = self.rep(digit, 1, None)?;
                let pre = self.class_set(
                    b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789-.",
                )?;
                let pre = self.rep(pre, 1, None)?;
                let dash = self.chars("-")?;
                let pre = self.seq(&[dash, pre])?;
                let pre = self.optional(pre)?;
                let build = self.class_set(
                    b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789-.",
                )?;
                let build = self.rep(build, 1, None)?;
                let plus = self.chars("+")?;
                let build = self.seq(&[plus, build])?;
                let build = self.optional(build)?;
                let parts = [major, dot, minor, dot2, patch, pre, build];
                self.seq(&parts)
            }
            ValidationKind::IsoDate => {
                // The extended form of ISO 8601, `2026-10-29`, and the basic
                // one, `20261029`.
                let digit = self.digits()?;
                let year = self.rep(digit, 4, Some(4))?;
                let month = self.month()?;
                let day = self.day()?;
                let dash = self.chars("-")?;
                let extended = self.seq(&[year, dash, month, dash, day])?;
                let basic = self.seq(&[year, month, day])?;
                self.alt(extended, basic)
            }
            ValidationKind::IsoDateTime => {
                let digit = self.digits()?;
                let year = self.rep(digit, 4, Some(4))?;
                let month = self.month()?;
                let day = self.day()?;
                let dash = self.chars("-")?;
                let date = self.seq(&[year, dash, month, dash, day])?;
                let marker = self.chars("T")?;
                let hour = self.hour()?;
                let colon = self.chars(":")?;
                let minute = self.minute()?;
                let time = self.seq(&[marker, hour, colon, minute])?;
                let colon2 = self.chars(":")?;
                let second = self.minute()?;
                let seconds = self.seq(&[colon2, second])?;
                let seconds = self.optional(seconds)?;
                let time = self.seq(&[time, seconds])?;
                let zulu = self.chars("Z")?;
                let zulu = self.optional(zulu)?;
                let zone = self.class_set(b"+-")?;
                let zone_hour = self.hour()?;
                let zone_colon = self.chars(":")?;
                let zone_minute = self.minute()?;
                let zone = self.seq(&[zone, zone_hour, zone_colon, zone_minute])?;
                let zone = self.optional(zone)?;
                let parts = [date, time, zulu, zone];
                self.seq(&parts)
            }
            ValidationKind::ClockTime => {
                let hour = self.hour()?;
                let colon = self.chars(":")?;
                let minute = self.minute()?;
                let colon2 = self.chars(":")?;
                let second = self.minute()?;
                let seconds = self.seq(&[colon2, second])?;
                let seconds = self.optional(seconds)?;
                let parts = [hour, colon, minute, seconds];
                self.seq(&parts)
            }
            ValidationKind::Uuid => {
                let hex = self.hex()?;
                let a = self.rep(hex, 8, Some(8))?;
                let dash = self.chars("-")?;
                let b = self.rep(hex, 4, Some(4))?;
                let dash2 = self.chars("-")?;
                let c = self.rep(hex, 4, Some(4))?;
                let dash3 = self.chars("-")?;
                let d = self.rep(hex, 4, Some(4))?;
                let dash4 = self.chars("-")?;
                let e = self.rep(hex, 12, Some(12))?;
                let parts = [a, dash, b, dash2, c, dash3, d, dash4, e];
                self.seq(&parts)
            }
            ValidationKind::Hex => {
                let hex = self.hex()?;
                let lower = self.chars("0x")?;
                let upper = self.chars("0X")?;
                let prefix = self.alt(lower, upper)?;
                let prefix = self.optional(prefix)?;
                let digits = self.rep(hex, 1, None)?;
                let parts = [prefix, digits];
                self.seq(&parts)
            }
            ValidationKind::Base64 => {
                let alphabet = self.class_set(
                    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/",
                )?;
                let body = self.rep(alphabet, 1, None)?;
                let pad = self.chars("=")?;
                let pad = self.rep(pad, 0, Some(2))?;
                let parts = [body, pad];
                self.seq(&parts)
            }
            ValidationKind::Base32 => {
                let alphabet = self.class_set(b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567")?;
                let body = self.rep(alphabet, 1, None)?;
                let pad = self.chars("=")?;
                let pad = self.rep(pad, 0, Some(6))?;
                let parts = [body, pad];
                self.seq(&parts)
            }
            ValidationKind::Slug => {
                // A slug is a run of characters, with dashes between the
                // runs: it cannot end with a dash.
                let base = self.class_set(b"abcdefghijklmnopqrstuvwxyz0123456789_")?;
                let base = self.rep(base, 1, None)?;
                let dash = self.chars("-")?;
                let part = self.seq(&[dash, base])?;
                let part = self.rep(part, 0, None)?;
                let first_class = self.class_set(b"abcdefghijklmnopqrstuvwxyz0123456789_")?;
                let first = self.rep(first_class, 1, None)?;
                let parts = [first, part];
                self.seq(&parts)
            }
            ValidationKind::Username => {
                let name = self.class_set(
                    b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_.-",
                )?;
                let name = self.rep(name, 3, None)?;
                Ok(name)
            }
            ValidationKind::CreditCard => {
                // A card number is thirteen to nineteen digits, written
                // either straight or in groups of four.
                let digit = self.digits()?;
                let plain = self.rep(digit, 13, Some(19))?;

                let group = self.rep(digit, 4, Some(4))?;
                let separator = self.class_set(b" -")?;
                let grouped =
                    self.seq(&[group, separator, group, separator, group, separator, group])?;

                self.alt(plain, grouped)
            }
            ValidationKind::Iban => {
                let upper = self.range(b'A', b'Z')?;
                let country = self.rep(upper, 2, Some(2))?;
                let digit = self.digits()?;
                let check = self.rep(digit, 2, Some(2))?;
                let body = self.class_set(b"ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789")?;
                let body = self.rep(body, 10, Some(30))?;
                let parts = [country, check, body];
                self.seq(&parts)
            }
            ValidationKind::UsZipCode => {
                let digit = self.digits()?;
                let first = self.rep(digit, 5, Some(5))?;
                let dash = self.chars("-")?;
                let second = self.rep(digit, 4, Some(4))?;
                let plus = self.seq(&[dash, second])?;
                let plus = self.optional(plus)?;
                let parts = [first, plus];
                self.seq(&parts)
            }
            ValidationKind::Integer => {
                let digit = self.digits()?;
                let minus = self.chars("-")?;
                let minus = self.optional(minus)?;
                let body = self.rep(digit, 1, None)?;
                let parts = [minus, body];
                self.seq(&parts)
            }
            ValidationKind::Decimal => {
                let digit = self.digits()?;
                let minus = self.chars("-")?;
                let minus = self.optional(minus)?;
                let body = self.rep(digit, 1, None)?;
                let dot = self.chars(".")?;
                let fraction = self.rep(digit, 1, None)?;
                let fraction = self.seq(&[dot, fraction])?;
                let fraction = self.optional(fraction)?;
                let parts = [minus, body, fraction];
                self.seq(&parts)
            }
            ValidationKind::CurrencyAmount => {
                // A currency amount is a symbol followed by a number.  The
                // number may group its thousands with one separator and end
                // with two decimal digits with the other one, in either
                // convention.
                let dollar = self.chars("$")?;
                let real = self.chars("R$")?;
                let symbol = self.alt(dollar, real)?;
                let euro = self.chars("EUR")?;
                let symbol = self.alt(symbol, euro)?;
                let symbol = self.optional(symbol)?;

                let space = self.class_set(b" ")?;
                let space = self.rep(space, 0, None)?;

                // Thousands grouped with a comma, decimals with a dot.
                let digit = self.digits()?;
                let integer = self.rep(digit, 1, None)?;
                let comma = self.chars(",")?;
                let digit_a = self.digits()?;
                let digit_b = self.digits()?;
                let digit_c = self.digits()?;
                let group = self.seq(&[comma, digit_a, digit_b, digit_c])?;
                let group = self.rep(group, 0, None)?;
                let dot = self.chars(".")?;
                let digit_a = self.digits()?;
                let digit_b = self.digits()?;
                let cents = self.seq(&[dot, digit_a, digit_b])?;
                let cents = self.optional(cents)?;
                let western = self.seq(&[integer, group, cents])?;

                // Thousands grouped with a dot, decimals with a comma.
                let digit = self.digits()?;
                let integer = self.rep(digit, 1, None)?;
                let dot = self.chars(".")?;
                let digit_a = self.digits()?;
                let digit_b = self.digits()?;
                let digit_c = self.digits()?;
                let group = self.seq(&[dot, digit_a, digit_b, digit_c])?;
                let group = self.rep(group, 0, None)?;
                let comma = self.chars(",")?;
                let digit_a = self.digits()?;
                let digit_b = self.digits()?;
                let cents = self.seq(&[comma, digit_a, digit_b])?;
                let cents = self.optional(cents)?;
                let eastern = self.seq(&[integer, group, cents])?;

                let number = self.alt(western, eastern)?;
                self.seq(&[symbol, space, number])
            }
            ValidationKind::Json => {
                // A JSON document is an object or an array.  The keys of an
                // object are quoted strings, and the values are strings,
                // numbers or one of the three literals.  Nested objects and
                // arrays are not recognised: this family validates the shape
                // of a document, it is not a JSON parser.
                let quote = self.chars("\"")?;
                let string_inner = self.class_except(b"\"")?;
                let string_inner = self.rep(string_inner, 0, None)?;
                let string = self.seq(&[quote, string_inner, quote])?;

                let minus = self.chars("-")?;
                let minus = self.optional(minus)?;
                let digit = self.digits()?;
                let integer = self.rep(digit, 1, None)?;
                let dot = self.chars(".")?;
                let fraction = self.rep(digit, 1, None)?;
                let fraction = self.seq(&[dot, fraction])?;
                let fraction = self.optional(fraction)?;
                let number = self.seq(&[minus, integer, fraction])?;

                let true_word = self.chars("true")?;
                let false_word = self.chars("false")?;
                let null_word = self.chars("null")?;
                let boolean = self.alt(true_word, false_word)?;
                let literal = self.alt(boolean, null_word)?;

                let value = self.alt(string, number)?;
                let value = self.alt(value, literal)?;

                let space = self.class_set(b" \t\r\n")?;
                let space = self.rep(space, 0, None)?;
                let colon = self.chars(":")?;
                let comma = self.chars(",")?;

                // A key and its value.
                let entry = self.seq(&[string, space, colon, space, value])?;
                // The separator carries the next entry with it, so that a
                // trailing comma is not accepted.
                let separator = self.seq(&[space, comma, space, entry])?;
                let separator = self.rep(separator, 0, None)?;
                let entries = self.seq(&[entry, separator])?;
                let entries = self.optional(entries)?;

                let open = self.chars("{")?;
                let close = self.chars("}")?;
                let object = self.seq(&[open, space, entries, space, close])?;

                // The separator between two values.
                let separator = self.seq(&[space, comma, space, value])?;
                let separator = self.rep(separator, 0, None)?;
                let items = self.seq(&[value, separator])?;
                let items = self.optional(items)?;

                let open = self.chars("[")?;
                let close = self.chars("]")?;
                let array = self.seq(&[open, space, items, space, close])?;

                self.alt(object, array)
            }
        }
    }

    /// Builds the automaton of an extraction family.
    fn extraction(
        &mut self,
        kind: ExtractionKind,
        boundary: BoundaryPolicy,
        _capture: crate::spec::CapturePolicy,
    ) -> Result<u16> {
        let lazy = boundary == BoundaryPolicy::Lazy;
        match kind {
            ExtractionKind::BetweenDelimiters => {
                let inner = self.class_except(b"]")?;
                let inner = if lazy {
                    self.rep_lazy(inner, 0, None)?
                } else {
                    self.rep(inner, 0, None)?
                };
                let captured = self.group(inner)?;
                let open = self.chars("[")?;
                let close = self.chars("]")?;
                let parts = [open, captured, close];
                self.seq(&parts)
            }
            ExtractionKind::Quoted => {
                // A quoted text is delimited by double quotes or by single
                // ones: "aspas simples" are as common as "aspas duplas".
                let double_inner = self.class_except(b"\"")?;
                let double_inner = if lazy {
                    self.rep_lazy(double_inner, 0, None)?
                } else {
                    self.rep(double_inner, 0, None)?
                };
                let double_captured = self.group(double_inner)?;
                let double_open = self.chars("\"")?;
                let double_close = self.chars("\"")?;
                let double = self.seq(&[double_open, double_captured, double_close])?;

                let single_inner = self.class_except(b"'")?;
                let single_inner = if lazy {
                    self.rep_lazy(single_inner, 0, None)?
                } else {
                    self.rep(single_inner, 0, None)?
                };
                let single_captured = self.group(single_inner)?;
                let single_open = self.chars("'")?;
                let single_close = self.chars("'")?;
                let single = self.seq(&[single_open, single_captured, single_close])?;

                self.alt(double, single)
            }
            ExtractionKind::Numbers => {
                let digit = self.digits()?;
                let body = self.rep(digit, 1, None)?;
                let dot = self.chars(".")?;
                let fraction = self.rep(digit, 1, None)?;
                let fraction = self.seq(&[dot, fraction])?;
                let fraction = self.optional(fraction)?;
                let number = self.seq(&[body, fraction])?;
                let captured = self.group(number)?;
                Ok(captured)
            }
            ExtractionKind::Words => {
                let letter = self.letters()?;
                let body = self.rep(letter, 1, None)?;
                let captured = self.group(body)?;
                Ok(captured)
            }
            ExtractionKind::Urls => {
                let scheme = self.chars("http")?;
                let secure = self.chars("s")?;
                let secure = self.optional(secure)?;
                let sep = self.chars("://")?;
                let host = self.class_set(
                    b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789.-",
                )?;
                let host = self.rep(host, 1, None)?;
                let path = self.class_set(
                    b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789-._~:/?#[]@!$&'()*+,;=%",
                )?;
                let path = self.rep(path, 0, None)?;
                let url = self.seq(&[scheme, secure, sep, host, path])?;
                let captured = self.group(url)?;
                Ok(captured)
            }
            ExtractionKind::Emails => {
                let local = self.class_set(
                    b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789._%+-",
                )?;
                let local = self.rep(local, 1, None)?;
                let at = self.chars("@")?;
                let domain = self.class_set(
                    b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789.-",
                )?;
                let domain = self.rep(domain, 1, None)?;
                let dot = self.chars(".")?;
                let tld = self.letters()?;
                let tld = self.rep(tld, 2, None)?;
                let email = self.seq(&[local, at, domain, dot, tld])?;
                let captured = self.group(email)?;
                Ok(captured)
            }
            ExtractionKind::IpAddresses => {
                let octet = self.ipv4_octet()?;
                let dot = self.chars(".")?;
                let second = self.ipv4_octet()?;
                let third = self.ipv4_octet()?;
                let fourth = self.ipv4_octet()?;
                let ip = self.seq(&[octet, dot, second, dot, third, dot, fourth])?;
                let captured = self.group(ip)?;
                Ok(captured)
            }
            ExtractionKind::Dates => {
                let digit = self.digits()?;
                let year = self.rep(digit, 4, Some(4))?;
                let dash = self.chars("-")?;
                let month = self.rep(digit, 2, Some(2))?;
                let dash2 = self.chars("-")?;
                let day = self.rep(digit, 2, Some(2))?;
                let date = self.seq(&[year, dash, month, dash2, day])?;
                let captured = self.group(date)?;
                Ok(captured)
            }
            ExtractionKind::AfterKeyword => {
                let word = self.letters()?;
                let word = self.rep(word, 1, None)?;
                let space = self.chars(" ")?;
                let space = self.rep(space, 0, None)?;
                let separator = self.class_set(b":=")?;
                let value = self.class_except(b"\n")?;
                let value = if lazy {
                    self.rep_lazy(value, 1, None)?
                } else {
                    self.rep(value, 1, None)?
                };
                let captured = self.group(value)?;
                let prefix = self.seq(&[word, space, separator, space])?;
                let parts = [prefix, captured];
                self.seq(&parts)
            }
            ExtractionKind::BeforeKeyword => {
                let word = self.letters()?;
                let word = self.rep(word, 1, None)?;
                let space = self.chars(" ")?;
                let space = self.rep(space, 0, None)?;
                let separator = self.class_set(b":=")?;
                let value = self.class_except(b"\n")?;
                let value = if lazy {
                    self.rep_lazy(value, 1, None)?
                } else {
                    self.rep(value, 1, None)?
                };
                let captured = self.group(value)?;
                let suffix = self.seq(&[space, separator, space, word])?;
                let parts = [captured, suffix];
                self.seq(&parts)
            }
        }
    }

    /// Builds the automaton of a filter intent.
    fn filter(&mut self, _action: FilterAction, terms: &SmallVec<PatternTerm, 4>) -> Result<u16> {
        let mut result: Option<u16> = None;
        for term in terms.iter() {
            let fold = term.kind != TermKind::Literal;
            let mut parts = [0u16; 24];
            let mut count = 0usize;
            for byte in term.bytes.iter() {
                if count == parts.len() {
                    break;
                }
                parts[count] = self.class_set_folded(&[*byte], fold)?;
                count += 1;
            }
            let literal = self.seq(&parts[..count])?;
            result = Some(match result {
                None => literal,
                Some(previous) => self.alt(previous, literal)?,
            });
        }
        match result {
            Some(node) => Ok(node),
            // A filter without terms matches nothing: an empty class holds no
            // byte, so no input can trigger it.
            None => self.class_set(&[]),
        }
    }

    /// Builds the automaton of a custom template.
    fn custom(
        &mut self,
        template: &PatternTemplate,
        params: &crate::spec::PatternParams,
    ) -> Result<u16> {
        let node = match template {
            PatternTemplate::Sequence(pieces) => {
                let mut parts = [0u16; 8];
                let mut count = 0usize;
                for piece in pieces.iter() {
                    if count == parts.len() {
                        break;
                    }
                    parts[count] = self.piece(*piece)?;
                    count += 1;
                }
                self.seq(&parts[..count])?
            }
            PatternTemplate::Repeat { class, count } => {
                let class_node = self.policy(*class)?;

                self.rep(class_node, *count, Some(*count))?
            }
            PatternTemplate::LiteralThenClass {
                literal,
                class,
                count,
            } => {
                let mut parts = [0u16; 12];
                let mut count_parts = 0usize;
                for byte in literal.iter() {
                    if count_parts == parts.len() {
                        break;
                    }
                    parts[count_parts] = self.class_set(&[*byte])?;
                    count_parts += 1;
                }
                let class_node = self.policy(*class)?;
                let run = self.rep(class_node, *count, Some(*count))?;
                parts[count_parts] = run;
                count_parts += 1;
                self.seq(&parts[..count_parts])?
            }
        };
        if params.anchored_start || params.anchored_end {
            let start = if params.anchored_start {
                self.assert(AssertKind::StartText)?
            } else {
                self.node(HirNode::Empty)?
            };
            let end = if params.anchored_end {
                self.assert(AssertKind::EndText)?
            } else {
                self.node(HirNode::Empty)?
            };
            let parts = [start, node, end];
            self.seq(&parts)
        } else {
            Ok(node)
        }
    }

    /// Builds one element of a custom template.
    fn piece(&mut self, piece: crate::spec::TemplatePiece) -> Result<u16> {
        use crate::spec::TemplatePiece;
        match piece {
            TemplatePiece::Literal(byte) => self.class_set(&[byte]),
            TemplatePiece::LiteralRange(start, end) => self.range(start, end),
            TemplatePiece::Class(policy) => self.policy(policy),
            TemplatePiece::Any => self.class_except(b"\n"),
            TemplatePiece::Anchor(kind) => self.assert(kind),
            TemplatePiece::GroupStart(_) | TemplatePiece::GroupEnd(_) => {
                // Group markers are handled by the template walker; a lone
                // marker is a no-op.
                self.node(HirNode::Empty)
            }
        }
    }
}
