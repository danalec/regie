//! Structured intent specifications.
//!
//! The intent layer of `regie` never emits a pattern as text. It emits a
//! [`CandidateSpec`]: a closed, machine-checkable description of what the
//! user asked for. The deterministic compiler in `regie-core` turns that
//! description into an automaton directly, so a model can never smuggle
//! arbitrary regex syntax (or arbitrary code) into the engine.
//!
//! The types in this module are the contract between the intent model
//! (`regie-intent`), the verified search (`regie-search`) and the compiler
//! (`regie-core`).

use crate::smallvec::SmallVec;

/// How strictly a validation should accept near-misses.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Strictness {
    /// Only the canonical form is accepted (for example `2026-10-29`).
    Strict,
    /// The canonical form plus common, unambiguous variations.
    Standard,
    /// Any value that still carries the right shape.
    Lenient,
}

/// The character repertoire a value is allowed to use.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum CharsetPolicy {
    /// Digits only (`0-9`).
    Digits,
    /// ASCII letters only.
    Letters,
    /// ASCII letters and digits.
    Alphanumeric,
    /// ASCII letters, digits and underscore.
    Word,
    /// Printable ASCII, including punctuation.
    PrintableAscii,
    /// Any byte (ASCII mode) or any Unicode scalar (Unicode mode).
    Any,
}

/// The family of value a validation refers to.
///
/// The intent model maps free-form language onto this enumeration; the
/// compiler knows how to build an automaton for every variant.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum ValidationKind {
    /// An e-mail address (`local@domain.tld`).
    Email,
    /// An HTTP(S) URL.
    Url,
    /// A generic URI with a scheme.
    Uri,
    /// A domain name such as `example.com`.
    DomainName,
    /// An IPv4 address in dotted-quad notation.
    Ipv4,
    /// An IPv6 address.
    Ipv6,
    /// A MAC address, colon or dash separated.
    MacAddress,
    /// A Brazilian CPF (`123.456.789-09`).
    BrazilianCpf,
    /// A Brazilian CNPJ (`12.345.678/0001-95`).
    BrazilianCnpj,
    /// A Brazilian postal code (`01310-100`).
    BrazilianPostalCode,
    /// A Brazilian phone number (`+55 11 91234-5678`).
    BrazilianPhone,
    /// A Brazilian vehicle licence plate (`ABC1D23`).
    BrazilianLicensePlate,
    /// A Portuguese NIF (`123456789`).
    PortugueseNif,
    /// A US Social Security Number (`123-45-6789`).
    UsSsn,
    /// A semantic version (`1.2.3`, `1.2.3-rc.1+build`).
    Semver,
    /// An ISO-8601 calendar date (`2026-10-29`).
    IsoDate,
    /// An ISO-8601 date and time (`2026-10-29T12:00:00Z`).
    IsoDateTime,
    /// A wall-clock time (`12:34`, `12:34:56`).
    ClockTime,
    /// A UUID (`123e4567-e89b-12d3-a456-426614174000`).
    Uuid,
    /// A hexadecimal literal (`0x1f` or plain hex digits).
    Hex,
    /// A Base64 payload.
    Base64,
    /// A Base32 payload.
    Base32,
    /// A URL-safe slug (`my-page_title`).
    Slug,
    /// A login name (`user_name`, `user.name`).
    Username,
    /// A bank card number (digit groups).
    CreditCard,
    /// An IBAN (`DE89370400440532013000`).
    Iban,
    /// A US ZIP code (`94105` or `94105-1234`).
    UsZipCode,
    /// A signed decimal integer (`-42`).
    Integer,
    /// A decimal number with an optional fraction (`3.14`).
    Decimal,
    /// A monetary amount (`$1,234.56`).
    CurrencyAmount,
    /// A JSON document (balanced braces and strings).
    Json,
}

/// The shape of an extraction request.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum ExtractionKind {
    /// Text enclosed in a pair of delimiters, such as `[...]`.
    BetweenDelimiters,
    /// Text enclosed in quotes.
    Quoted,
    /// Numbers embedded in free text.
    Numbers,
    /// Words embedded in free text.
    Words,
    /// URLs embedded in free text.
    Urls,
    /// E-mail addresses embedded in free text.
    Emails,
    /// IP addresses embedded in free text.
    IpAddresses,
    /// Dates embedded in free text.
    Dates,
    /// The part following a keyword.
    AfterKeyword,
    /// The part preceding a keyword.
    BeforeKeyword,
}

/// How an extraction deals with its boundaries.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum BoundaryPolicy {
    /// Consume as much as possible.
    Greedy,
    /// Consume as little as possible.
    Lazy,
    /// Stop at the delimiter, excluding it.
    Exclusive,
}

/// Which part of the match an extraction returns.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum CapturePolicy {
    /// The whole match, delimiters included.
    Whole,
    /// Only the captured group.
    Group,
}

/// What to do with an input that matches a filter term.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum FilterAction {
    /// Reject the input.
    Block,
    /// Accept the input.
    Allow,
    /// Accept, but flag the input.
    Highlight,
    /// Accept and count the input.
    Count,
}

/// The nature of a filter term.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum TermKind {
    /// A literal string.
    Literal,
    /// A single word.
    Word,
    /// A phrase, matched case-insensitively.
    Phrase,
}

/// One term of a filter intent.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct PatternTerm {
    /// The nature of the term.
    pub kind: TermKind,
    /// The literal bytes of the term (ASCII, lower-cased for phrases).
    pub bytes: SmallVec<u8, 24>,
}

impl Default for PatternTerm {
    fn default() -> Self {
        Self {
            kind: TermKind::Literal,
            bytes: SmallVec::new(),
        }
    }
}

impl PatternTerm {
    /// Builds a literal term.
    pub fn literal(bytes: &[u8]) -> Self {
        let mut out = SmallVec::new();
        for b in bytes {
            if !out.push(*b) {
                break;
            }
        }
        Self {
            kind: TermKind::Literal,
            bytes: out,
        }
    }

    /// Builds a word term.
    pub fn word(bytes: &[u8]) -> Self {
        let mut out = SmallVec::new();
        for b in bytes {
            if !out.push(*b) {
                break;
            }
        }
        Self {
            kind: TermKind::Word,
            bytes: out,
        }
    }

    /// Builds a phrase term (matched case-insensitively).
    pub fn phrase(bytes: &[u8]) -> Self {
        let mut out = SmallVec::new();
        for b in bytes {
            if !out.push(b.to_ascii_lowercase()) {
                break;
            }
        }
        Self {
            kind: TermKind::Phrase,
            bytes: out,
        }
    }

    /// The term as a string slice, when the bytes are valid UTF-8.
    pub fn as_str(&self) -> Option<&str> {
        core::str::from_utf8(self.bytes.as_slice()).ok()
    }
}

/// One element of a custom pattern template.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum TemplatePiece {
    /// A literal byte.
    Literal(u8),
    /// An inclusive range of literal bytes.
    LiteralRange(u8, u8),
    /// A character class selected by policy.
    Class(CharsetPolicy),
    /// Any byte.
    Any,
    /// A zero-width assertion.
    Anchor(crate::hir::AssertKind),
    /// The opening of a capture group.
    GroupStart(u16),
    /// The closing of a capture group.
    GroupEnd(u16),
}

/// A structured pattern shape, expressed without pattern text.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum PatternTemplate {
    /// A sequence of pieces.
    Sequence(SmallVec<TemplatePiece, 8>),
    /// A run of `count` characters from a class.
    Repeat {
        /// The character class of the run.
        class: CharsetPolicy,
        /// How many characters the run holds.
        count: u16,
    },
    /// A literal prefix followed by a run of characters from a class.
    LiteralThenClass {
        /// The literal prefix.
        literal: SmallVec<u8, 12>,
        /// The character class of the run.
        class: CharsetPolicy,
        /// How many characters the run holds.
        count: u16,
    },
}

impl Default for TemplatePiece {
    fn default() -> Self {
        TemplatePiece::Literal(0)
    }
}

impl Default for PatternTemplate {
    fn default() -> Self {
        PatternTemplate::Sequence(SmallVec::new())
    }
}

/// Flags and bounds that apply to a compiled specification.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct PatternParams {
    /// Distinguish upper and lower case.
    pub case_sensitive: bool,
    /// Treat `^` and `$` as line anchors.
    pub multiline: bool,
    /// Let `.` match a newline.
    pub dot_all: bool,
    /// Anchor the match at the start of the input.
    pub anchored_start: bool,
    /// Anchor the match at the end of the input.
    pub anchored_end: bool,
    /// Minimum accepted length, when the kind defines one.
    pub min_len: Option<u16>,
    /// Maximum accepted length, when the kind defines one.
    pub max_len: Option<u16>,
}

impl Default for PatternParams {
    fn default() -> Self {
        Self {
            case_sensitive: true,
            multiline: false,
            dot_all: false,
            anchored_start: false,
            anchored_end: false,
            min_len: None,
            max_len: None,
        }
    }
}

/// Why the intent layer could not settle on a single specification.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum AmbiguityReason {
    /// The model was not confident enough about any candidate.
    LowConfidence,
    /// Several candidates describe the request equally well.
    ConflictingKinds,
    /// The request lacks a detail that changes the outcome.
    MissingDetail,
    /// The candidates disagree on the verification examples.
    InconsistentExamples,
}

/// A fully specified, unambiguous intent.
///
/// This is the only shape the compiler accepts. It contains no pattern text:
/// every field is an enumerated choice or a bounded inline value.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum CandidateSpec {
    /// Validate a value against a known family.
    Validate {
        /// The family of value.
        kind: ValidationKind,
        /// How strict the validation is.
        strictness: Strictness,
        /// The character repertoire.
        charset: CharsetPolicy,
        /// Maximum accepted length, when the family defines one.
        max_len: Option<u16>,
    },
    /// Extract a part out of a larger text.
    Extract {
        /// The shape of the extracted part.
        kind: ExtractionKind,
        /// How boundaries are handled.
        boundary: BoundaryPolicy,
        /// Which part is returned.
        capture: CapturePolicy,
    },
    /// Filter inputs by a set of terms.
    Filter {
        /// What to do with a matching input.
        action: FilterAction,
        /// The terms that trigger the action.
        terms: SmallVec<PatternTerm, 4>,
    },
    /// A user-shaped pattern built from a template.
    Custom {
        /// The shape of the pattern.
        template: PatternTemplate,
        /// Flags and bounds.
        params: PatternParams,
    },
}

/// The output of the intent layer.
///
/// The two variants are of a very different size, and boxing the larger one
/// would cost an allocation - and would take away the `Copy` that lets the
/// caller keep the answer in a register.
#[allow(clippy::large_enum_variant)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum IntentSpec {
    /// Exactly one candidate survived verification.
    Confident(CandidateSpec),
    /// Several candidates survived and the caller must choose.
    Ambiguous {
        /// Why the layer refused to pick one.
        reason: AmbiguityReason,
        /// The surviving candidates, most promising first.
        suggestions: SmallVec<CandidateSpec, 3>,
    },
}

impl IntentSpec {
    /// The candidate, when the intent is unambiguous.
    pub fn candidate(&self) -> Option<&CandidateSpec> {
        match self {
            IntentSpec::Confident(spec) => Some(spec),
            IntentSpec::Ambiguous { .. } => None,
        }
    }

    /// Whether the intent is unambiguous.
    pub fn is_confident(&self) -> bool {
        matches!(self, IntentSpec::Confident(_))
    }
}

impl Default for CandidateSpec {
    fn default() -> Self {
        CandidateSpec::Validate {
            kind: ValidationKind::Email,
            strictness: Strictness::Standard,
            charset: CharsetPolicy::Any,
            max_len: None,
        }
    }
}

impl CandidateSpec {
    /// A short, stable name of the candidate family, useful for reports.
    pub fn kind_name(&self) -> &'static str {
        match self {
            CandidateSpec::Validate { kind, .. } => validation_kind_name(*kind),
            CandidateSpec::Extract { kind, .. } => extraction_kind_name(*kind),
            CandidateSpec::Filter { .. } => "filter",
            CandidateSpec::Custom { .. } => "custom",
        }
    }
}

/// The stable name of a validation family.
pub fn validation_kind_name(kind: ValidationKind) -> &'static str {
    match kind {
        ValidationKind::Email => "email",
        ValidationKind::Url => "url",
        ValidationKind::Uri => "uri",
        ValidationKind::DomainName => "domain_name",
        ValidationKind::Ipv4 => "ipv4",
        ValidationKind::Ipv6 => "ipv6",
        ValidationKind::MacAddress => "mac_address",
        ValidationKind::BrazilianCpf => "brazilian_cpf",
        ValidationKind::BrazilianCnpj => "brazilian_cnpj",
        ValidationKind::BrazilianPostalCode => "brazilian_postal_code",
        ValidationKind::BrazilianPhone => "brazilian_phone",
        ValidationKind::BrazilianLicensePlate => "brazilian_license_plate",
        ValidationKind::PortugueseNif => "portuguese_nif",
        ValidationKind::UsSsn => "us_ssn",
        ValidationKind::Semver => "semver",
        ValidationKind::IsoDate => "iso_date",
        ValidationKind::IsoDateTime => "iso_date_time",
        ValidationKind::ClockTime => "clock_time",
        ValidationKind::Uuid => "uuid",
        ValidationKind::Hex => "hex",
        ValidationKind::Base64 => "base64",
        ValidationKind::Base32 => "base32",
        ValidationKind::Slug => "slug",
        ValidationKind::Username => "username",
        ValidationKind::CreditCard => "credit_card",
        ValidationKind::Iban => "iban",
        ValidationKind::UsZipCode => "us_zip_code",
        ValidationKind::Integer => "integer",
        ValidationKind::Decimal => "decimal",
        ValidationKind::CurrencyAmount => "currency_amount",
        ValidationKind::Json => "json",
    }
}

/// The stable name of an extraction family.
pub fn extraction_kind_name(kind: ExtractionKind) -> &'static str {
    match kind {
        ExtractionKind::BetweenDelimiters => "between_delimiters",
        ExtractionKind::Quoted => "quoted",
        ExtractionKind::Numbers => "numbers",
        ExtractionKind::Words => "words",
        ExtractionKind::Urls => "urls",
        ExtractionKind::Emails => "emails",
        ExtractionKind::IpAddresses => "ip_addresses",
        ExtractionKind::Dates => "dates",
        ExtractionKind::AfterKeyword => "after_keyword",
        ExtractionKind::BeforeKeyword => "before_keyword",
    }
}

/// The complete list of validation families, in a stable order.
///
/// The intent model uses this list to build its label set, and the dataset
/// generator uses it to guarantee coverage.
pub const VALIDATION_KINDS: [ValidationKind; 31] = [
    ValidationKind::Email,
    ValidationKind::Url,
    ValidationKind::Uri,
    ValidationKind::DomainName,
    ValidationKind::Ipv4,
    ValidationKind::Ipv6,
    ValidationKind::MacAddress,
    ValidationKind::BrazilianCpf,
    ValidationKind::BrazilianCnpj,
    ValidationKind::BrazilianPostalCode,
    ValidationKind::BrazilianPhone,
    ValidationKind::BrazilianLicensePlate,
    ValidationKind::PortugueseNif,
    ValidationKind::UsSsn,
    ValidationKind::Semver,
    ValidationKind::IsoDate,
    ValidationKind::IsoDateTime,
    ValidationKind::ClockTime,
    ValidationKind::Uuid,
    ValidationKind::Hex,
    ValidationKind::Base64,
    ValidationKind::Base32,
    ValidationKind::Slug,
    ValidationKind::Username,
    ValidationKind::CreditCard,
    ValidationKind::Iban,
    ValidationKind::UsZipCode,
    ValidationKind::Integer,
    ValidationKind::Decimal,
    ValidationKind::CurrencyAmount,
    ValidationKind::Json,
];

/// The complete list of extraction families, in a stable order.
pub const EXTRACTION_KINDS: [ExtractionKind; 10] = [
    ExtractionKind::BetweenDelimiters,
    ExtractionKind::Quoted,
    ExtractionKind::Numbers,
    ExtractionKind::Words,
    ExtractionKind::Urls,
    ExtractionKind::Emails,
    ExtractionKind::IpAddresses,
    ExtractionKind::Dates,
    ExtractionKind::AfterKeyword,
    ExtractionKind::BeforeKeyword,
];
