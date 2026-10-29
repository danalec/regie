//! The intent micro-model.
//!
//! The model is a linear classifier over the lexical features of a request.
//! Its weights are the log-odds ratios of a Bernoulli Naive Bayes that was
//! fitted offline by `tools/train_intent.py`; they are quantized to `i8` and
//! share a single scale, so scoring a request is a handful of integer
//! operations and gives the same answer on every platform.
//!
//! The model only ever chooses an enumerated class.  The remaining fields of
//! a [`CandidateSpec`] - the strictness, the charset, the filter terms, the
//! custom template - are derived by the small rule engine below, which is
//! deterministic as well.

use crate::features::{self, Features, Tokens};
use crate::weights::{BIASES, CLASS_COUNT, MARGIN_THRESHOLD, MIN_SCORE, VOCAB, WEIGHTS};
use regie_core::spec::{
    AmbiguityReason, BoundaryPolicy, CandidateSpec, CapturePolicy, CharsetPolicy, ExtractionKind,
    FilterAction, IntentSpec, PatternParams, PatternTemplate, PatternTerm, Strictness,
    ValidationKind, EXTRACTION_KINDS, VALIDATION_KINDS,
};
use regie_core::SmallVec;

/// The number of validation classes, in the order of [`VALIDATION_KINDS`].
const VALIDATION_CLASSES: usize = 31;
/// The number of extraction classes, in the order of [`EXTRACTION_KINDS`].
const EXTRACTION_CLASSES: usize = 10;
/// The class of the filter intent.
const FILTER_CLASS: usize = VALIDATION_CLASSES + EXTRACTION_CLASSES;
/// The class of the custom-template intent.
const CUSTOM_CLASS: usize = FILTER_CLASS + 1;

/// The number of candidates [`IntentModel::candidates`] returns.
const MAX_CANDIDATES: usize = 4;
/// The number of suggestions an ambiguous intent carries.
const MAX_SUGGESTIONS: usize = 3;

/// The local micro-model that maps a request onto a structured intent.
///
/// The model holds no state: the weights are compiled into the binary, so
/// building one costs nothing and interpreting a request allocates nothing.
///
/// ```
/// use regie_intent::IntentModel;
///
/// let model = IntentModel::embedded();
/// let spec = model.interpret("validar um CPF");
/// assert!(spec.is_confident());
/// ```
#[derive(Clone, Copy, Debug)]
pub struct IntentModel {
    _private: (),
}

impl IntentModel {
    /// Builds the model with the bundled, quantized weights.
    ///
    /// The weights are a `&'static` table in the binary: there is no I/O, no
    /// allocation and no platform dependent step.
    pub const fn embedded() -> IntentModel {
        IntentModel { _private: () }
    }

    /// The number of bytes the embedded weight table occupies.
    pub const fn weight_bytes() -> usize {
        crate::weights::weight_bytes()
    }

    /// The number of classes the model discriminates.
    pub const fn class_count() -> usize {
        CLASS_COUNT
    }

    /// The number of lexical features the model knows.
    pub const fn feature_count() -> usize {
        VOCAB.len()
    }

    /// Classifies one request.
    ///
    /// The result is [`IntentSpec::Confident`] only when the best class is
    /// far enough ahead of the runner-up.  A request that the model does not
    /// understand, or that two classes explain equally well, comes back as
    /// [`IntentSpec::Ambiguous`] - the layer never guesses.
    ///
    /// A request that shares no word with the model carries no evidence at
    /// all; it comes back ambiguous without any suggestion, because there is
    /// nothing worth suggesting.
    ///
    /// ```
    /// use regie_intent::IntentModel;
    ///
    /// let model = IntentModel::embedded();
    /// assert!(model.interpret("qwertyuiop zzzz").is_confident() == false);
    /// ```
    pub fn interpret(&self, request: &str) -> IntentSpec {
        let (tokens, features) = features::analyze(request);
        let scores = score_all(&features);
        let order = rank(&scores);

        // A request that shares no word with the model carries no evidence:
        // answering it would be a guess, not a classification.
        if features.lexical() == 0 {
            return IntentSpec::Ambiguous {
                reason: AmbiguityReason::LowConfidence,
                suggestions: SmallVec::new(),
            };
        }

        let best = order[0];
        let runner_up = order[1];
        if scores[best] < MIN_SCORE {
            return IntentSpec::Ambiguous {
                reason: AmbiguityReason::LowConfidence,
                suggestions: suggestions(&order, request, &tokens),
            };
        }
        if scores[best] - scores[runner_up] < MARGIN_THRESHOLD {
            return IntentSpec::Ambiguous {
                reason: AmbiguityReason::ConflictingKinds,
                suggestions: suggestions(&order, request, &tokens),
            };
        }
        IntentSpec::Confident(spec_for(best, request, &tokens))
    }

    /// The ranked candidates for a request, most promising first.
    ///
    /// At most four candidates are returned.  Unlike [`IntentModel::interpret`],
    /// this never refuses: the caller decides what to do with the ranking.
    pub fn candidates(&self, request: &str) -> SmallVec<CandidateSpec, 4> {
        let (tokens, features) = features::analyze(request);
        let scores = score_all(&features);
        let order = rank(&scores);
        let mut out = SmallVec::new();
        for &class in order.iter().take(MAX_CANDIDATES) {
            if !out.push(spec_for(class, request, &tokens)) {
                break;
            }
        }
        out
    }
}

impl Default for IntentModel {
    fn default() -> IntentModel {
        IntentModel::embedded()
    }
}

/// Scores every class with the quantized weights.
fn score_all(features: &Features) -> [i32; CLASS_COUNT] {
    let mut scores = [0i32; CLASS_COUNT];
    for class in 0..CLASS_COUNT {
        let mut total = BIASES[class];
        let row = &WEIGHTS[class];
        for &id in features.ids() {
            total += row[id as usize] as i32;
        }
        scores[class] = total;
    }
    scores
}

/// Whether `left` is a better class than `right`.
///
/// Ties are broken by the class order, so the ranking is total and stable.
fn better(scores: &[i32; CLASS_COUNT], left: usize, right: usize) -> bool {
    if scores[left] != scores[right] {
        scores[left] > scores[right]
    } else {
        left < right
    }
}

/// Orders the classes by score, best first.
fn rank(scores: &[i32; CLASS_COUNT]) -> [usize; CLASS_COUNT] {
    let mut order = [0usize; CLASS_COUNT];
    for (index, slot) in order.iter_mut().enumerate() {
        *slot = index;
    }
    for index in 1..CLASS_COUNT {
        let mut cursor = index;
        while cursor > 0 && better(scores, order[cursor], order[cursor - 1]) {
            order.swap(cursor, cursor - 1);
            cursor -= 1;
        }
    }
    order
}

/// Builds the suggestions of an ambiguous intent.
fn suggestions(
    order: &[usize; CLASS_COUNT],
    request: &str,
    tokens: &Tokens,
) -> SmallVec<CandidateSpec, 3> {
    let mut out = SmallVec::new();
    for &class in order.iter().take(MAX_SUGGESTIONS) {
        if !out.push(spec_for(class, request, tokens)) {
            break;
        }
    }
    out
}

/// Builds the specification of a class.
fn spec_for(class: usize, request: &str, tokens: &Tokens) -> CandidateSpec {
    if class < VALIDATION_CLASSES {
        let kind = VALIDATION_KINDS[class];
        CandidateSpec::Validate {
            kind,
            strictness: strictness_for(tokens),
            charset: charset_for(kind),
            max_len: max_len_for(kind),
        }
    } else if class < FILTER_CLASS {
        let kind = EXTRACTION_KINDS[class - VALIDATION_CLASSES];
        CandidateSpec::Extract {
            kind,
            boundary: boundary_for(tokens),
            capture: capture_for(kind, tokens),
        }
    } else if class == FILTER_CLASS {
        CandidateSpec::Filter {
            action: filter_action(tokens),
            terms: filter_terms(request, tokens),
        }
    } else {
        debug_assert_eq!(class, CUSTOM_CLASS);
        let template = custom_template(tokens);
        let params = custom_params(&template);
        CandidateSpec::Custom { template, params }
    }
}

// ---------------------------------------------------------------------------
// The parts of a specification that the classifier does not decide
// ---------------------------------------------------------------------------

/// The character repertoire of a validation family.
fn charset_for(kind: ValidationKind) -> CharsetPolicy {
    match kind {
        ValidationKind::Email
        | ValidationKind::Url
        | ValidationKind::Uri
        | ValidationKind::DomainName
        | ValidationKind::Ipv6
        | ValidationKind::MacAddress
        | ValidationKind::IsoDateTime
        | ValidationKind::Base64
        | ValidationKind::CurrencyAmount => CharsetPolicy::PrintableAscii,
        ValidationKind::Ipv4
        | ValidationKind::BrazilianCpf
        | ValidationKind::BrazilianCnpj
        | ValidationKind::BrazilianPostalCode
        | ValidationKind::BrazilianPhone
        | ValidationKind::PortugueseNif
        | ValidationKind::UsSsn
        | ValidationKind::IsoDate
        | ValidationKind::ClockTime
        | ValidationKind::CreditCard
        | ValidationKind::UsZipCode
        | ValidationKind::Integer
        | ValidationKind::Decimal => CharsetPolicy::Digits,
        ValidationKind::BrazilianLicensePlate
        | ValidationKind::Semver
        | ValidationKind::Uuid
        | ValidationKind::Hex
        | ValidationKind::Base32
        | ValidationKind::Iban => CharsetPolicy::Alphanumeric,
        ValidationKind::Slug | ValidationKind::Username => CharsetPolicy::Word,
        ValidationKind::Json => CharsetPolicy::Any,
    }
}

/// The maximum accepted length of a validation family.
fn max_len_for(kind: ValidationKind) -> Option<u16> {
    let value = match kind {
        ValidationKind::Email => 254,
        ValidationKind::Url | ValidationKind::Uri => 2048,
        ValidationKind::DomainName => 253,
        ValidationKind::Ipv4 => 15,
        ValidationKind::Ipv6 => 45,
        ValidationKind::MacAddress => 17,
        ValidationKind::BrazilianCpf => 14,
        ValidationKind::BrazilianCnpj => 18,
        ValidationKind::BrazilianPostalCode => 9,
        ValidationKind::BrazilianPhone => 17,
        ValidationKind::BrazilianLicensePlate => 8,
        ValidationKind::PortugueseNif => 9,
        ValidationKind::UsSsn => 11,
        ValidationKind::Semver => 64,
        ValidationKind::IsoDate => 10,
        ValidationKind::IsoDateTime => 32,
        ValidationKind::ClockTime => 12,
        ValidationKind::Uuid => 36,
        ValidationKind::Hex => 64,
        ValidationKind::Base64 => 256,
        ValidationKind::Base32 => 64,
        ValidationKind::Slug => 64,
        ValidationKind::Username => 32,
        ValidationKind::CreditCard => 19,
        ValidationKind::Iban => 34,
        ValidationKind::UsZipCode => 10,
        ValidationKind::Integer => 20,
        ValidationKind::Decimal => 32,
        ValidationKind::CurrencyAmount => 32,
        ValidationKind::Json => 4096,
    };
    Some(value)
}

/// The strictness a request asks for.
fn strictness_for(tokens: &Tokens) -> Strictness {
    const STRICT: &[&str] = &[
        "estritamente",
        "estrito",
        "estrita",
        "strict",
        "rigorosamente",
        "rigoroso",
        "exatamente",
        "exato",
        "canonical",
        "canonico",
    ];
    const LENIENT: &[&str] = &[
        "flexivel",
        "lenient",
        "permissivo",
        "qualquer",
        "loose",
        "relaxado",
        "frouxo",
    ];
    if tokens.first_index_of(STRICT).is_some() {
        Strictness::Strict
    } else if tokens.first_index_of(LENIENT).is_some() {
        Strictness::Lenient
    } else {
        Strictness::Standard
    }
}

/// The boundary policy a request asks for.
fn boundary_for(tokens: &Tokens) -> BoundaryPolicy {
    const EXCLUSIVE: &[&str] = &["exclusivo", "excluindo", "excluding", "exclusive"];
    const LAZY: &[&str] = &["lazy", "preguicoso", "guloso"];
    const AFTER_SEM: &[&str] = &[
        "delimitador",
        "delimitadores",
        "delimiter",
        "delimiters",
        "aspas",
        "quote",
        "quotes",
        "chave",
        "keyword",
        "marcador",
        "marker",
    ];
    if tokens.first_index_of(EXCLUSIVE).is_some() {
        return BoundaryPolicy::Exclusive;
    }
    for (index, token) in tokens.iter().enumerate() {
        if token == b"sem" {
            if let Some(next) = tokens.get(index + 1) {
                if AFTER_SEM.iter().any(|cue| cue.as_bytes() == next) {
                    return BoundaryPolicy::Exclusive;
                }
            }
        }
    }
    if tokens.first_index_of(LAZY).is_some() {
        BoundaryPolicy::Lazy
    } else {
        BoundaryPolicy::Greedy
    }
}

/// The capture policy a request asks for.
fn capture_for(kind: ExtractionKind, tokens: &Tokens) -> CapturePolicy {
    const WHOLE: &[&str] = &[
        "inteira",
        "inteiro",
        "completa",
        "completo",
        "whole",
        "full",
        "incluindo",
    ];
    const GROUP: &[&str] = &["grupo", "group", "interno", "internal"];
    if tokens.first_index_of(WHOLE).is_some() {
        return CapturePolicy::Whole;
    }
    if tokens.first_index_of(GROUP).is_some() {
        return CapturePolicy::Group;
    }
    // The families that have no inner group return the whole match.
    match kind {
        ExtractionKind::Numbers
        | ExtractionKind::Words
        | ExtractionKind::Urls
        | ExtractionKind::Emails
        | ExtractionKind::IpAddresses
        | ExtractionKind::Dates => CapturePolicy::Whole,
        _ => CapturePolicy::Group,
    }
}

/// The action a filter request asks for.
fn filter_action(tokens: &Tokens) -> FilterAction {
    const BLOCK: &[&str] = &[
        "bloquear",
        "bloqueie",
        "bloqueado",
        "block",
        "blocking",
        "reject",
        "rejeitar",
        "rejeitado",
        "remover",
        "remove",
        "proibir",
        "proibido",
        "ban",
        "banned",
        "deny",
        "negar",
        "recusar",
        "descartar",
        "drop",
    ];
    const ALLOW: &[&str] = &[
        "permitir",
        "permite",
        "permitido",
        "allow",
        "allowed",
        "aceitar",
        "aceito",
        "aceita",
        "liberar",
        "liberado",
    ];
    const HIGHLIGHT: &[&str] = &[
        "destacar",
        "destaque",
        "destacado",
        "highlight",
        "realcar",
        "marcar",
        "marcado",
        "mark",
        "marked",
        "assinalar",
    ];
    const COUNT: &[&str] = &["contar", "contagem", "count", "counting"];

    let mut best: Option<(usize, FilterAction)> = None;
    for (cues, action) in [
        (BLOCK, FilterAction::Block),
        (ALLOW, FilterAction::Allow),
        (HIGHLIGHT, FilterAction::Highlight),
        (COUNT, FilterAction::Count),
    ] {
        if let Some(index) = tokens.first_index_of(cues) {
            if best.map_or(true, |(best_index, _)| index < best_index) {
                best = Some((index, action));
            }
        }
    }
    // A filter without an explicit action blocks, which is the safe choice.
    best.map_or(FilterAction::Block, |(_, action)| action)
}

/// The terms a filter request asks for.
fn filter_terms(request: &str, tokens: &Tokens) -> SmallVec<PatternTerm, 4> {
    const SPECIFIC_MARKERS: &[&str] = &[
        "palavra",
        "palavras",
        "word",
        "words",
        "termo",
        "termos",
        "term",
        "terms",
        "frase",
        "frases",
        "expressao",
        "expressoes",
    ];
    const GENERIC_MARKERS: &[&str] = &[
        "mensagem",
        "mensagens",
        "message",
        "messages",
        "item",
        "itens",
        "linha",
        "linhas",
        "line",
        "lines",
        "entrada",
        "entradas",
        "input",
        "inputs",
    ];
    const STOPWORDS: &[&str] = &[
        "a",
        "as",
        "o",
        "os",
        "de",
        "da",
        "do",
        "das",
        "dos",
        "e",
        "ou",
        "com",
        "que",
        "para",
        "em",
        "no",
        "na",
        "nos",
        "nas",
        "the",
        "of",
        "to",
        "in",
        "on",
        "at",
        "any",
        "all",
        "only",
        "just",
        "out",
        "and",
        "or",
        "not",
        "but",
        "sem",
        "apenas",
        "como",
        "quantas",
        "quanto",
        "many",
        "how",
        "contem",
        "contain",
        "containing",
        "tenha",
        "tenham",
        "tiver",
        "tiverem",
        "tem",
        "sejam",
        "for",
        "from",
        "that",
        "this",
        "these",
        "those",
        "it",
        "is",
        "are",
        "be",
        "been",
        "pelo",
        "pela",
    ];

    let mut out = SmallVec::new();
    // A quoted term is taken literally.
    if push_quoted_terms(request, &mut out) {
        return out;
    }

    let marker = tokens
        .last_index_of(SPECIFIC_MARKERS)
        .or_else(|| tokens.last_index_of(GENERIC_MARKERS));
    let marker = match marker {
        Some(marker) => marker,
        None => return out,
    };
    for token in tokens.iter().skip(marker + 1) {
        if STOPWORDS.iter().any(|stop| stop.as_bytes() == token) {
            continue;
        }
        if !out.push(PatternTerm::word(token)) {
            break;
        }
    }
    out
}

/// Pushes the quoted spans of a request into `out`.
///
/// Returns whether a quoted span was found.
fn push_quoted_terms(request: &str, out: &mut SmallVec<PatternTerm, 4>) -> bool {
    let mut start: Option<usize> = None;
    let mut found = false;
    for (index, ch) in request.char_indices() {
        if ch == '"' || ch == '\u{201c}' || ch == '\u{201d}' {
            match start {
                Some(begin) => {
                    let span = &request[begin..index];
                    if !span.is_empty() {
                        found = true;
                        if !out.push(PatternTerm::literal(span.as_bytes())) {
                            return found;
                        }
                    }
                    start = None;
                }
                None => start = Some(index + ch.len_utf8()),
            }
        }
    }
    found
}

/// The shape of a custom pattern.
fn custom_template(tokens: &Tokens) -> PatternTemplate {
    let (count, charset) = custom_shape(tokens);
    match prefix_of(tokens) {
        Some(prefix) => {
            let mut literal: SmallVec<u8, 12> = SmallVec::new();
            for byte in prefix {
                if !literal.push(*byte) {
                    break;
                }
            }
            PatternTemplate::LiteralThenClass {
                literal,
                class: charset,
                count,
            }
        }
        None => PatternTemplate::Repeat {
            class: charset,
            count,
        },
    }
}

/// The bounds of a custom pattern.
fn custom_params(template: &PatternTemplate) -> PatternParams {
    let length = match template {
        PatternTemplate::Repeat { count, .. } => *count,
        PatternTemplate::LiteralThenClass { literal, count, .. } => literal.len() as u16 + *count,
        PatternTemplate::Sequence(pieces) => pieces.len() as u16,
    };
    PatternParams {
        min_len: Some(length),
        max_len: Some(length),
        ..PatternParams::default()
    }
}

/// The number of characters and the character class a custom request asks for.
fn custom_shape(tokens: &Tokens) -> (u16, CharsetPolicy) {
    let mut count: Option<u16> = None;
    let mut charset: Option<CharsetPolicy> = None;
    for (index, token) in tokens.iter().enumerate() {
        if token.is_empty() || !token.iter().all(|byte| byte.is_ascii_digit()) {
            continue;
        }
        let value: u32 = match core::str::from_utf8(token)
            .ok()
            .and_then(|text| text.parse().ok())
        {
            Some(value) => value,
            None => continue,
        };
        if !(1..=64).contains(&value) {
            continue;
        }
        match class_near(tokens, index) {
            Some(class) => {
                count = Some(value as u16);
                charset = Some(class);
                break;
            }
            None => {
                if count.is_none() {
                    count = Some(value as u16);
                }
            }
        }
    }
    match count {
        Some(count) => (count, charset.unwrap_or(CharsetPolicy::Alphanumeric)),
        None => (
            4,
            class_near(tokens, 0).unwrap_or(CharsetPolicy::Alphanumeric),
        ),
    }
}

/// The most specific character class mentioned around `center`.
///
/// The classes are ordered from the most to the least specific, so a
/// request that says "6 alphanumeric characters" is not read as a plain
/// character run.
fn class_near(tokens: &Tokens, center: usize) -> Option<CharsetPolicy> {
    const CLASSES: &[(&[&str], CharsetPolicy)] = &[
        (
            &[
                "alfanumerico",
                "alfanumericos",
                "alphanumeric",
                "alphanumerics",
            ],
            CharsetPolicy::Alphanumeric,
        ),
        (
            &[
                "digito", "digitos", "digit", "digits", "numero", "numeros", "number", "numbers",
            ],
            CharsetPolicy::Digits,
        ),
        (
            &["letra", "letras", "letter", "letters"],
            CharsetPolicy::Letters,
        ),
        (
            &[
                "caractere",
                "caracteres",
                "character",
                "characters",
                "chars",
                "char",
            ],
            CharsetPolicy::Alphanumeric,
        ),
        (
            &["simbolo", "simbolos", "symbol", "symbols"],
            CharsetPolicy::PrintableAscii,
        ),
        (
            &["palavra", "palavras", "word", "words"],
            CharsetPolicy::Letters,
        ),
    ];
    let mut best: Option<CharsetPolicy> = None;
    let mut best_rank = CLASSES.len();
    for offset in -3i32..=3 {
        if offset == 0 {
            continue;
        }
        let index = center as i32 + offset;
        if index < 0 || index as usize >= tokens.len() {
            continue;
        }
        let token = match tokens.get(index as usize) {
            Some(token) => token,
            None => continue,
        };
        for (rank, (words, charset)) in CLASSES.iter().enumerate() {
            if words.iter().any(|word| word.as_bytes() == token) && rank < best_rank {
                best_rank = rank;
                best = Some(*charset);
            }
        }
    }
    best
}

/// The literal prefix a custom request asks for.
fn prefix_of(tokens: &Tokens) -> Option<&[u8]> {
    const PREFIX_CUES: &[&str] = &[
        "prefixo",
        "prefix",
        "comecando",
        "comeca",
        "starting",
        "starts",
        "begins",
        "begin",
    ];
    const SKIP: &[&str] = &[
        "com", "de", "do", "da", "pelo", "pela", "the", "by", "with", "and", "e",
    ];
    let index = tokens.first_index_of(PREFIX_CUES)?;
    let mut probe = index + 1;
    while let Some(token) = tokens.get(probe) {
        if SKIP.iter().any(|cue| cue.as_bytes() == token) {
            probe += 1;
        } else {
            break;
        }
    }
    tokens.get(probe).filter(|token| !token.is_empty())
}
