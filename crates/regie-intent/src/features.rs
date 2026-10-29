//! Lexical feature extraction for the intent micro-model.
//!
//! The extractor turns a free-form request into a small set of binary
//! features: the tokens it contains, the pairs of adjacent tokens, and a few
//! coarse shape buckets (token count, text length, digits, accents).
//!
//! This module is a line-by-line mirror of the Python implementation in
//! `tools/train_intent.py`.  Both sides have to agree exactly, otherwise the
//! model would be scored with features it was never trained on.  When one
//! side changes, the other has to change with it.
//!
//! Everything here is allocation free and works on `no_std`.

use crate::weights::VOCAB;

/// The maximum number of bytes kept from a normalized request.
const TEXT_CAP: usize = 512;
/// The maximum number of tokens considered from one request.
const MAX_TOKENS: usize = 64;
/// The maximum length of a single token, in bytes.
const MAX_TOKEN_LEN: usize = 24;
/// The maximum number of distinct features extracted from one request.
const MAX_FEATURES: usize = 256;

/// The ASCII tokens of a request, after normalization.
///
/// The tokens are stored inline: the crate must not need an allocator.
pub(crate) struct Tokens {
    buf: [[u8; MAX_TOKEN_LEN]; MAX_TOKENS],
    lens: [u8; MAX_TOKENS],
    len: usize,
}

impl Tokens {
    /// Creates an empty token set.
    const fn new() -> Self {
        Self {
            buf: [[0; MAX_TOKEN_LEN]; MAX_TOKENS],
            lens: [0; MAX_TOKENS],
            len: 0,
        }
    }

    /// The number of tokens.
    pub(crate) fn len(&self) -> usize {
        self.len
    }

    /// The token at `index`, if any.
    pub(crate) fn get(&self, index: usize) -> Option<&[u8]> {
        if index < self.len {
            Some(&self.buf[index][..self.lens[index] as usize])
        } else {
            None
        }
    }

    /// Iterates over the tokens.
    pub(crate) fn iter(&self) -> impl Iterator<Item = &[u8]> + '_ {
        (0..self.len).map(move |index| &self.buf[index][..self.lens[index] as usize])
    }

    /// The index of the first token that is one of `cues`.
    pub(crate) fn first_index_of(&self, cues: &[&str]) -> Option<usize> {
        for (index, token) in self.iter().enumerate() {
            if cues.iter().any(|cue| cue.as_bytes() == token) {
                return Some(index);
            }
        }
        None
    }

    /// The index of the last token that is one of `cues`.
    pub(crate) fn last_index_of(&self, cues: &[&str]) -> Option<usize> {
        let mut found = None;
        for (index, token) in self.iter().enumerate() {
            if cues.iter().any(|cue| cue.as_bytes() == token) {
                found = Some(index);
            }
        }
        found
    }
}

/// The vocabulary indices of the features present in a request.
pub(crate) struct Features {
    ids: [u16; MAX_FEATURES],
    len: usize,
    lexical: usize,
}

impl Features {
    /// Creates an empty feature set.
    const fn new() -> Self {
        Self {
            ids: [0; MAX_FEATURES],
            len: 0,
            lexical: 0,
        }
    }

    /// Records a feature, ignoring duplicates.
    ///
    /// `lexical` marks the features that come from the words of the request;
    /// the shape buckets do not count, because a request that shares no word
    /// with the model carries no evidence at all.
    fn push(&mut self, id: u16, lexical: bool) {
        if self.len == MAX_FEATURES {
            return;
        }
        if self.ids[..self.len].contains(&id) {
            return;
        }
        self.ids[self.len] = id;
        self.len += 1;
        if lexical {
            self.lexical += 1;
        }
    }

    /// The vocabulary indices of the present features.
    pub(crate) fn ids(&self) -> &[u16] {
        &self.ids[..self.len]
    }

    /// The number of features that come from the words of the request.
    pub(crate) fn lexical(&self) -> usize {
        self.lexical
    }
}

/// Folds one character to a lowercase ASCII letter or digit.
///
/// Every other character returns `None` and becomes a token separator.  The
/// accented Latin letters are folded to their base letter, so that "validação"
/// and "validacao" produce the same tokens.
fn fold(ch: char) -> Option<u8> {
    match ch {
        'A'..='Z' => Some(ch as u8 + 32),
        'a'..='z' => Some(ch as u8),
        '0'..='9' => Some(ch as u8),
        '\u{e0}' | '\u{e1}' | '\u{e2}' | '\u{e3}' | '\u{e4}' | '\u{e5}' | '\u{c0}' | '\u{c1}'
        | '\u{c2}' | '\u{c3}' | '\u{c4}' | '\u{c5}' => Some(b'a'),
        '\u{e7}' | '\u{c7}' => Some(b'c'),
        '\u{e8}' | '\u{e9}' | '\u{ea}' | '\u{eb}' | '\u{c8}' | '\u{c9}' | '\u{ca}' | '\u{cb}' => {
            Some(b'e')
        }
        '\u{ec}' | '\u{ed}' | '\u{ee}' | '\u{ef}' | '\u{cc}' | '\u{cd}' | '\u{ce}' | '\u{cf}' => {
            Some(b'i')
        }
        '\u{f1}' | '\u{d1}' => Some(b'n'),
        '\u{f2}' | '\u{f3}' | '\u{f4}' | '\u{f5}' | '\u{f6}' | '\u{f8}' | '\u{d2}' | '\u{d3}'
        | '\u{d4}' | '\u{d5}' | '\u{d6}' | '\u{d8}' => Some(b'o'),
        '\u{f9}' | '\u{fa}' | '\u{fb}' | '\u{fc}' | '\u{d9}' | '\u{da}' | '\u{db}' | '\u{dc}' => {
            Some(b'u')
        }
        '\u{fd}' | '\u{ff}' | '\u{dd}' => Some(b'y'),
        _ => None,
    }
}

/// Normalizes a request into ASCII bytes.
///
/// Every character that is not part of a token becomes a single space and
/// runs of spaces are collapsed, so the result only depends on the words.
fn normalize(text: &str, buf: &mut [u8; TEXT_CAP]) -> usize {
    let mut out = 0;
    for ch in text.chars() {
        if out == buf.len() {
            break;
        }
        match fold(ch) {
            Some(byte) => {
                buf[out] = byte;
                out += 1;
            }
            None => {
                if out > 0 && buf[out - 1] != b' ' {
                    buf[out] = b' ';
                    out += 1;
                }
            }
        }
    }
    while out > 0 && buf[out - 1] == b' ' {
        out -= 1;
    }
    out
}

/// Splits a normalized request into tokens.
///
/// Tokens longer than [`MAX_TOKEN_LEN`] are dropped: they carry no useful
/// signal and would only make the vocabulary bigger.
fn tokenize(text: &str, buf: &mut [u8; TEXT_CAP], tokens: &mut Tokens) -> usize {
    let len = normalize(text, buf);
    let mut count = 0;
    let mut index = 0;
    while index < len && count < MAX_TOKENS {
        if buf[index] == b' ' {
            index += 1;
            continue;
        }
        let start = index;
        while index < len && buf[index] != b' ' {
            index += 1;
        }
        let width = index - start;
        if width <= MAX_TOKEN_LEN {
            tokens.buf[count][..width].copy_from_slice(&buf[start..index]);
            tokens.lens[count] = width as u8;
            count += 1;
        }
    }
    tokens.len = count;
    len
}

/// Looks a word up in the vocabulary.
fn lookup_word(word: &[u8]) -> Option<u16> {
    VOCAB
        .binary_search_by(|probe| probe.as_bytes().cmp(word))
        .ok()
        .map(|index| index as u16)
}

/// Compares a vocabulary entry with the virtual string `left + " " + right`.
///
/// The bigrams are stored as one string, but they are built from two separate
/// tokens, so the comparison is done without materializing the pair.
fn compare_pair(probe: &[u8], left: &[u8], right: &[u8]) -> core::cmp::Ordering {
    use core::cmp::Ordering;
    let mut index = 0;
    for &byte in left {
        if index == probe.len() {
            return Ordering::Less;
        }
        match probe[index].cmp(&byte) {
            Ordering::Equal => index += 1,
            other => return other,
        }
    }
    if index == probe.len() {
        return Ordering::Less;
    }
    match probe[index].cmp(&b' ') {
        Ordering::Equal => index += 1,
        other => return other,
    }
    for &byte in right {
        if index == probe.len() {
            return Ordering::Less;
        }
        match probe[index].cmp(&byte) {
            Ordering::Equal => index += 1,
            other => return other,
        }
    }
    if index < probe.len() {
        Ordering::Less
    } else {
        Ordering::Equal
    }
}

/// Looks a pair of adjacent tokens up in the vocabulary.
fn lookup_pair(left: &[u8], right: &[u8]) -> Option<u16> {
    VOCAB
        .binary_search_by(|probe| compare_pair(probe.as_bytes(), left, right))
        .ok()
        .map(|index| index as u16)
}

/// The coarse length bucket of a normalized request.
fn length_bucket(len: usize) -> &'static str {
    match len {
        0 => "0",
        1..=2 => "1-2",
        3..=5 => "3-5",
        6..=10 => "6-10",
        11..=20 => "11-20",
        21..=40 => "21-40",
        _ => "41+",
    }
}

/// Writes a small decimal number into `buf` and returns the new length.
fn write_number(buf: &mut [u8], at: usize, value: u8) -> usize {
    if value >= 10 {
        buf[at] = b'0' + value / 10;
        buf[at + 1] = b'0' + value % 10;
        at + 2
    } else {
        buf[at] = b'0' + value;
        at + 1
    }
}

/// Extracts the tokens and the features of a request.
pub(crate) fn analyze(request: &str) -> (Tokens, Features) {
    let mut buf = [0u8; TEXT_CAP];
    let mut tokens = Tokens::new();
    let mut features = Features::new();

    let len = tokenize(request, &mut buf, &mut tokens);

    for index in 0..tokens.len {
        let word = &tokens.buf[index][..tokens.lens[index] as usize];
        if let Some(id) = lookup_word(word) {
            features.push(id, true);
        }
    }
    for index in 0..tokens.len.saturating_sub(1) {
        let left = &tokens.buf[index][..tokens.lens[index] as usize];
        let right = &tokens.buf[index + 1][..tokens.lens[index + 1] as usize];
        if let Some(id) = lookup_pair(left, right) {
            features.push(id, true);
        }
    }

    // The shape buckets are marked as non lexical: they say how the request
    // looks, not what it is about.
    let mut name = [0u8; 16];
    name[..5].copy_from_slice(b"#tok=");
    let count = tokens.len.min(9) as u8;
    let end = write_number(&mut name, 5, count);
    if let Some(id) = lookup_word(&name[..end]) {
        features.push(id, false);
    }
    if let Some(id) = lookup_word(length_bucket(len).as_bytes()) {
        features.push(id, false);
    }
    if tokens
        .iter()
        .any(|token| token.iter().any(|byte| byte.is_ascii_digit()))
    {
        if let Some(id) = lookup_word(b"#digits") {
            features.push(id, false);
        }
    }
    if request.chars().any(|ch| ch as u32 > 127) {
        if let Some(id) = lookup_word(b"#accent") {
            features.push(id, false);
        }
    }

    (tokens, features)
}
