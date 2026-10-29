#!/usr/bin/env python3
"""Offline trainer for the `regie-intent` micro-model.

The model maps a free-form natural-language request onto one of 43 intent
classes (31 validation families, 10 extraction families, a filter family and a
custom-template family).  It is a quantized linear classifier over lexical
features; the trained weights are written to
`crates/regie-intent/src/weights.rs` as plain `const` tables.

Everything here is standard-library only and fully deterministic:

* the corpus is a fixed, hand-written table (no randomness, no downloads);
* the train/eval split is a fixed per-class rule;
* the shuffle uses an explicit LCG with a fixed seed;
* the optimizer runs a fixed number of rounds over a fixed order.

Running `python tools/train_intent.py` twice therefore produces byte-identical
output.

The feature extractor in this file is a line-by-line mirror of
`crates/regie-intent/src/features.rs`.  Any change to one of them must be
mirrored in the other, otherwise the Rust model stops agreeing with the
weights trained here.
"""

from __future__ import annotations

import argparse
import json
import math
import shutil
import subprocess
import sys
from pathlib import Path

# ---------------------------------------------------------------------------
# Layout
# ---------------------------------------------------------------------------

ROOT = Path(__file__).resolve().parent.parent
DATASET = ROOT / "datasets" / "intent_train.jsonl"
WEIGHTS = ROOT / "crates" / "regie-intent" / "src" / "weights.rs"

# ---------------------------------------------------------------------------
# Hyper-parameters (fixed: the trainer must be reproducible)
# ---------------------------------------------------------------------------

# Additive smoothing of the Bernoulli Naive Bayes.  Small values keep the
# log-odds of a rare word sharp while still bounding the rare words that do
# not occur in a class.
SMOOTHING = 0.1
# Largest magnitude of a quantized weight.  The weights are stored as `i8`,
# so the quantization step is `max|w| / QUANT_LEVELS`.
QUANT_LEVELS = 127
# Every EVAL_EVERY-th request of a class is held out.
EVAL_EVERY = 5

# ---------------------------------------------------------------------------
# Feature extraction: must stay in sync with crates/regie-intent/src/features.rs
# ---------------------------------------------------------------------------

TEXT_CAP = 512
MAX_TOKENS = 64
MAX_TOKEN_LEN = 24

_ACCENT_GROUPS = {
    "a": "\u00e0\u00e1\u00e2\u00e3\u00e4\u00e5\u00c0\u00c1\u00c2\u00c3\u00c4\u00c5",
    "c": "\u00e7\u00c7",
    "e": "\u00e8\u00e9\u00ea\u00eb\u00c8\u00c9\u00ca\u00cb",
    "i": "\u00ec\u00ed\u00ee\u00ef\u00cc\u00cd\u00ce\u00cf",
    "n": "\u00f1\u00d1",
    "o": "\u00f2\u00f3\u00f4\u00f5\u00f6\u00f8\u00d2\u00d3\u00d4\u00d5\u00d6\u00d8",
    "u": "\u00f9\u00fa\u00fb\u00fc\u00d9\u00da\u00db\u00dc",
    "y": "\u00fd\u00ff\u00dd",
}

ACCENT_MAP = {
    ord(ch): base for base, chars in _ACCENT_GROUPS.items() for ch in chars
}


def fold_char(ch: str) -> str | None:
    """Folds one character to a lowercase ASCII alphanumeric, or `None`."""
    code = ord(ch)
    if code < 128:
        if "A" <= ch <= "Z":
            return ch.lower()
        if ("a" <= ch <= "z") or ("0" <= ch <= "9"):
            return ch
        return None
    return ACCENT_MAP.get(code)


def tokenize(text: str) -> tuple[list[str], int]:
    """Normalizes `text` into ASCII tokens.

    Returns the token list and the length of the normalized text in bytes.
    """
    buf = bytearray()
    for ch in text:
        if len(buf) >= TEXT_CAP:
            break
        folded = fold_char(ch)
        if folded is None:
            if buf and buf[-1] != 0x20:
                buf.append(0x20)
        else:
            buf.append(ord(folded))
    while buf and buf[-1] == 0x20:
        buf.pop()
    tokens: list[str] = []
    for part in buf.split(b" "):
        if not part:
            continue
        if len(part) > MAX_TOKEN_LEN:
            continue
        if len(tokens) >= MAX_TOKENS:
            break
        tokens.append(part.decode("ascii"))
    return tokens, len(buf)


def length_bucket(length: int) -> str:
    """Maps a text length onto a coarse bucket name."""
    if length == 0:
        return "0"
    if length <= 2:
        return "1-2"
    if length <= 5:
        return "3-5"
    if length <= 10:
        return "6-10"
    if length <= 20:
        return "11-20"
    if length <= 40:
        return "21-40"
    return "41+"


def feature_names(text: str) -> list[str]:
    """Returns the distinct feature names of `text`, in a stable order."""
    tokens, norm_len = tokenize(text)
    names: list[str] = []
    seen: set[str] = set()

    def add(name: str) -> None:
        if name not in seen:
            seen.add(name)
            names.append(name)

    for token in tokens:
        add(token)
    for i in range(len(tokens) - 1):
        add(tokens[i] + " " + tokens[i + 1])
    add("#tok=%d" % min(len(tokens), 9))
    add("#len=%s" % length_bucket(norm_len))
    if any(any("0" <= c <= "9" for c in token) for token in tokens):
        add("#digits")
    if any(ord(c) > 127 for c in text):
        add("#accent")
    return names


# ---------------------------------------------------------------------------
# Rules that turn a request into the non-classified parts of a specification
# (mirrors crates/regie-intent/src/model.rs)
# ---------------------------------------------------------------------------

SPECIFIC_MARKERS = (
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
)

GENERIC_MARKERS = (
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
)

STOPWORDS = frozenset(
    """
    a as o os de da do das dos e ou com que para em no na nos nas the of to in on
    at any all only just out and or not but sem apenas como quantas quanto many how
    contem contem contain containing tenha tenham tiver tiverem tem sejam for
    from that this these those it is are be been com do pelo pela pelo pela
    """.split()
)

CONNECTORS = frozenset(("e", "ou", "and", "or", "a", "o"))

BLOCK_CUES = (
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
)

ALLOW_CUES = (
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
)

HIGHLIGHT_CUES = (
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
)

COUNT_CUES = ("contar", "contagem", "count", "counting")

# Generic filter verbs only decide the action when no specific cue is present.
FILTER_FALLBACK_CUES = ("filtrar", "filter", "filtro", "filtering")

STRICT_CUES = (
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
)

LENIENT_CUES = (
    "flexivel",
    "lenient",
    "permissivo",
    "qualquer",
    "loose",
    "relaxado",
    "frouxo",
)

LAZY_CUES = ("lazy", "preguicoso", "guloso")

EXCLUSIVE_CUES = ("exclusivo", "excluindo", "excluding", "exclusive")

WHOLE_CUES = (
    "inteira",
    "inteiro",
    "completa",
    "completo",
    "whole",
    "full",
    "incluindo",
)

GROUP_CUES = ("grupo", "group", "interno", "internal")

PREFIX_CUES = (
    "prefixo",
    "prefix",
    "comecando",
    "comeca",
    "starting",
    "starts",
    "begins",
    "begin",
)

PREFIX_SKIP = frozenset(
    ("com", "de", "do", "da", "pelo", "pela", "the", "by", "with", "and", "e")
)

# Class words, most specific first.
CLASS_WORDS: tuple[tuple[tuple[str, ...], str], ...] = (
    (
        ("alfanumerico", "alfanumericos", "alphanumeric", "alphanumerics"),
        "Alphanumeric",
    ),
    (
        (
            "digito",
            "digitos",
            "digit",
            "digits",
            "numero",
            "numeros",
            "number",
            "numbers",
        ),
        "Digits",
    ),
    (("letra", "letras", "letter", "letters"), "Letters"),
    (
        (
            "caractere",
            "caracteres",
            "character",
            "characters",
            "chars",
            "char",
        ),
        "Alphanumeric",
    ),
    (("simbolo", "simbolos", "symbol", "symbols"), "PrintableAscii"),
    (("palavra", "palavras", "word", "words"), "Letters"),
)


def _first_token(tokens: list[str], cues) -> int | None:
    for index, token in enumerate(tokens):
        if token in cues:
            return index
    return None


def _last_token(tokens: list[str], cues) -> int | None:
    found = None
    for index, token in enumerate(tokens):
        if token in cues:
            found = index
    return found


def filter_action(request: str) -> str:
    """Picks the filter action of a request."""
    tokens, _ = tokenize(request)
    best: tuple[int, str] | None = None
    for cues, action in (
        (BLOCK_CUES, "Block"),
        (ALLOW_CUES, "Allow"),
        (HIGHLIGHT_CUES, "Highlight"),
        (COUNT_CUES, "Count"),
    ):
        index = _first_token(tokens, cues)
        if index is not None and (best is None or index < best[0]):
            best = (index, action)
    if best is not None:
        return best[1]
    return "Block"


def quoted_spans(request: str) -> list[str]:
    """Returns the contents of the double-quoted spans of a request."""
    spans: list[str] = []
    current: list[str] | None = None
    for ch in request:
        if ch in ('"', "\u201c", "\u201d"):
            if current is None:
                current = []
            else:
                spans.append("".join(current))
                current = None
            continue
        if current is not None:
            current.append(ch)
    return [span for span in spans if span]


def filter_terms(request: str) -> list[tuple[str, str]]:
    """Extracts the terms of a filter request as (kind, text) pairs."""
    quoted = quoted_spans(request)
    if quoted:
        return [("Literal", span) for span in quoted[:4]]
    tokens, _ = tokenize(request)
    marker = _last_token(tokens, SPECIFIC_MARKERS)
    if marker is None:
        marker = _last_token(tokens, GENERIC_MARKERS)
    if marker is None:
        return []
    terms: list[tuple[str, str]] = []
    for token in tokens[marker + 1 :]:
        if token in CONNECTORS or token in STOPWORDS:
            continue
        terms.append(("Word", token))
        if len(terms) == 4:
            break
    return terms


def strictness_of(request: str) -> str:
    """Picks the strictness requested by a request."""
    tokens, _ = tokenize(request)
    if _first_token(tokens, STRICT_CUES) is not None:
        return "Strict"
    if _first_token(tokens, LENIENT_CUES) is not None:
        return "Lenient"
    return "Standard"


def boundary_of(request: str) -> str:
    """Picks the boundary policy requested by a request."""
    tokens, _ = tokenize(request)
    if _first_token(tokens, EXCLUSIVE_CUES) is not None:
        return "Exclusive"
    for index, token in enumerate(tokens):
        if token == "sem" and index + 1 < len(tokens):
            if tokens[index + 1] in (
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
            ):
                return "Exclusive"
    if _first_token(tokens, LAZY_CUES) is not None:
        return "Lazy"
    return "Greedy"


def capture_of(request: str, default: str) -> str:
    """Picks the capture policy requested by a request."""
    tokens, _ = tokenize(request)
    if _first_token(tokens, WHOLE_CUES) is not None:
        return "Whole"
    if _first_token(tokens, GROUP_CUES) is not None:
        return "Group"
    return default


def _class_of_window(tokens: list[str], center: int) -> str | None:
    """Picks the most specific class word around `center`."""
    best: str | None = None
    best_rank = len(CLASS_WORDS)
    for offset in range(-3, 4):
        index = center + offset
        if index < 0 or index >= len(tokens) or index == center:
            continue
        for rank, (words, charset) in enumerate(CLASS_WORDS):
            if tokens[index] in words and rank < best_rank:
                best_rank = rank
                best = charset
    return best


def custom_shape(request: str) -> tuple:
    """Builds the shape of a custom pattern from a request."""
    tokens, _ = tokenize(request)
    count: int | None = None
    charset: str | None = None
    for index, token in enumerate(tokens):
        if not token.isdigit() or not token.isascii():
            continue
        value = int(token)
        if value < 1 or value > 64:
            continue
        window = _class_of_window(tokens, index)
        if window is not None:
            count = value
            charset = window
            break
        if count is None:
            count = value
    if count is None:
        count = 4
        charset = _class_of_window(tokens, 0) or "Alphanumeric"

    prefix: str | None = None
    for index, token in enumerate(tokens):
        if token in PREFIX_CUES:
            probe = index + 1
            while probe < len(tokens) and tokens[probe] in PREFIX_SKIP:
                probe += 1
            if probe < len(tokens):
                prefix = tokens[probe]
            break

    if prefix is not None:
        return ("LiteralThenClass", prefix, charset or "Alphanumeric", count)
    return ("Repeat", charset or "Alphanumeric", count)


# ---------------------------------------------------------------------------
# The corpus
# ---------------------------------------------------------------------------
#
# Every class of a family is rendered with the *same* request templates, so
# the template words carry no information about the class.  Only the words
# that describe the value differ between the classes, and those are what the
# model has to learn.  Each class contributes four Portuguese and four
# English nouns, and the templates are rotated over them, so every class is
# seen in sixteen different sentence shapes.

VALIDATION_TEMPLATES_PT = [
    "validar {n}",
    "preciso validar {n}",
    "validação de {n}",
    "testar a validação de {n}",
    "checar se {n} está no formato correto",
    "{art} {n} está no formato errado",
    "rejeitar {n} com formato inválido",
    "como validar {n}",
]

VALIDATION_TEMPLATES_EN = [
    "validate {n}",
    "check if {n} is valid",
    "validation of {n}",
    "i need to validate {n}",
    "{n} must follow the correct format",
    "reject {n} with the wrong format",
    "how to validate {n}",
    "testing the validation of {n}",
]

EXTRACTION_TEMPLATES_PT = [
    "extrair {n}",
    "preciso extrair {n}",
    "extração de {n}",
    "no parágrafo, extrair {n}",
    "busca por {n}",
    "extrair apenas {n}",
    "extrair {n} do conteúdo",
    "encontrar {n} no texto",
]

EXTRACTION_TEMPLATES_EN = [
    "extract {n}",
    "get {n} from the text",
    "extraction of {n}",
    "i need to extract {n}",
    "{n} inside the text",
    "looking for {n} in the text",
    "extract only {n}",
    "pull {n} out of the text",
]

# The phrasings that ask for a single value.  They need the singular nouns
# below, because the tokenizer does not stem: "the url" and "the urls" are two
# different features.
EXTRACTION_TEMPLATES_PT_EXTRA = [
    "extrair {n} do texto",
    "preciso extrair {n} do texto",
]

EXTRACTION_TEMPLATES_EN_EXTRA = [
    "extract {n} from the text",
    "i want to extract {n} from the text",
]

# Each validation entry is
# (kind, charset, max_len, gender, pt_nouns, en_nouns, positives, negatives).
VALIDATION_CORPUS = [
    (
        "Email",
        "PrintableAscii",
        254,
        "m",
        ["email", "endereço de email", "e-mail", "endereço eletrônico"],
        ["email address", "e-mail address", "email", "mail address"],
        ["ana.silva@example.com", "contato@empresa.com.br"],
        ["ana.silva@", "@example.com"],
    ),
    (
        "Url",
        "PrintableAscii",
        2048,
        "f",
        ["url", "endereço de site", "link web", "endereço web"],
        ["url", "web address", "link", "site address"],
        ["https://example.com/path", "http://example.org"],
        ["example.com", "http://"],
    ),
    (
        "Uri",
        "PrintableAscii",
        2048,
        "f",
        ["uri", "uri genérica", "identificador uri", "endereço uri"],
        ["uri", "generic uri", "uri reference", "resource identifier"],
        ["urn:isbn:0451450523", "mailto:ana@example.com"],
        ["urn:", "://"],
    ),
    (
        "DomainName",
        "PrintableAscii",
        253,
        "m",
        ["domínio", "nome de domínio", "endereço de domínio", "domínio web"],
        ["domain name", "hostname", "domain", "web domain"],
        ["example.com", "sub.example.co.uk"],
        ["example", "-bad-.com"],
    ),
    (
        "Ipv4",
        "Digits",
        15,
        "m",
        ["ipv4", "endereço ipv4", "endereço ip versão 4", "endereço ip v4"],
        ["ipv4 address", "ipv4", "ip address version 4", "ipv4 host address"],
        ["192.168.0.1", "255.255.255.255"],
        ["192.168.0", "256.1.1.1"],
    ),
    (
        "Ipv6",
        "PrintableAscii",
        45,
        "m",
        ["ipv6", "endereço ipv6", "endereço ip versão 6", "endereço ip v6"],
        ["ipv6 address", "ipv6", "ip address version 6", "ipv6 host address"],
        # The compressed form of an address is not recognised: the family
        # expects the eight groups written out.
        ["2001:db8:0:0:0:0:2:1", "fe80:0:0:0:0:0:0:1"],
        ["fe80::", "gggg::1"],
    ),
    (
        "MacAddress",
        "PrintableAscii",
        17,
        "m",
        ["endereço mac", "mac address", "endereço de hardware", "endereço físico de rede"],
        ["mac address", "hardware address", "physical address", "ethernet address"],
        ["00:1A:2B:3C:4D:5E", "00-1a-2b-3c-4d-5e"],
        ["00:1A:2B:3C:4D", "zz:1a:2b:3c:4d:5e"],
    ),
    (
        "BrazilianCpf",
        "Digits",
        14,
        "m",
        ["cpf", "cpf de cliente", "cadastro de pessoa física", "cpf brasileiro"],
        ["cpf", "brazilian cpf", "cpf number", "person registry number"],
        # The check digits are part of the shape, but their value is not
        # checked: a regular expression cannot compute a checksum.
        ["123.456.789-09", "12345678909"],
        ["123.456.789", "123.456.789-0"],
    ),
    (
        "BrazilianCnpj",
        "Digits",
        18,
        "m",
        ["cnpj", "cnpj de empresa", "cadastro de empresa", "cnpj brasileiro"],
        ["cnpj", "brazilian cnpj", "cnpj number", "company registry number"],
        # The check digits are part of the shape, but their value is not
        # checked: a regular expression cannot compute a checksum.
        ["12.345.678/0001-95", "12345678000195"],
        ["12.345.678/0001", "12.345.678/0001-9"],
    ),
    (
        "BrazilianPostalCode",
        "Digits",
        9,
        "m",
        ["cep", "código postal", "cep brasileiro", "código postal brasileiro"],
        ["brazilian postal code", "cep", "brazilian postcode", "postal code of brazil"],
        ["01310-100", "01310100"],
        ["01310-10", "1234"],
    ),
    (
        "BrazilianPhone",
        "Digits",
        17,
        "m",
        ["telefone", "número de telefone", "celular", "telefone brasileiro"],
        ["brazilian phone number", "phone number", "cellphone number", "telephone number"],
        ["+55 11 91234-5678", "11912345678"],
        ["+55 11 912-5678", "12345"],
    ),
    (
        "BrazilianLicensePlate",
        "Alphanumeric",
        8,
        "f",
        ["placa de veículo", "placa de carro", "placa", "licença de veículo"],
        ["brazilian license plate", "vehicle plate", "license plate", "car plate"],
        ["ABC1D23", "ABC1234"],
        ["ABC1D2", "A1B2C3D"],
    ),
    (
        "PortugueseNif",
        "Digits",
        9,
        "m",
        ["nif", "nif português", "número de identificação fiscal", "número fiscal"],
        ["portuguese nif", "nif", "fiscal number", "tax number"],
        ["123456789", "501442600"],
        ["12345678", "1234567890"],
    ),
    (
        "UsSsn",
        "Digits",
        11,
        "m",
        ["ssn", "social security number", "número de segurança social", "ssn americano"],
        ["us ssn", "social security number", "ssn", "social security id"],
        # The area 000 and the group 00 do not exist, but a regular
        # expression cannot know the rules of the administration.
        ["123-45-6789", "123456789"],
        ["123-45-678", "123-456-789"],
    ),
    (
        "Semver",
        "Alphanumeric",
        64,
        "m",
        ["versão semver", "versão semântica", "semver", "versão de pacote"],
        ["semver version", "semantic version", "version string", "package version"],
        ["1.2.3", "1.2.3-rc.1+build"],
        ["1.2", "v1.2.3.4.5"],
    ),
    (
        "IsoDate",
        "Digits",
        10,
        "f",
        ["data iso", "data no formato iso", "data de calendário", "data em iso 8601"],
        ["iso date", "calendar date", "iso 8601 date", "date value"],
        ["2026-10-29", "19991231"],
        ["2026-13-01", "29/10/2026"],
    ),
    (
        "IsoDateTime",
        "PrintableAscii",
        32,
        "f",
        ["data e hora iso", "datetime", "timestamp", "momento em iso 8601"],
        ["iso datetime", "timestamp", "iso 8601 timestamp", "datetime with time"],
        ["2026-10-29T12:00:00Z", "2026-10-29T12:00:00+02:00"],
        ["2026-10-29 12:00", "2026-10-29T99:00:00Z"],
    ),
    (
        "ClockTime",
        "Digits",
        12,
        "m",
        ["horário", "hora do dia", "horário do relógio", "hora em hh:mm"],
        ["clock time", "wall clock time", "time of day", "time value"],
        ["12:34", "12:34:56"],
        ["25:00", "12"],
    ),
    (
        "Uuid",
        "Alphanumeric",
        36,
        "m",
        ["uuid", "uuid v4", "guid", "identificador único"],
        ["uuid", "uuid v4", "guid", "universally unique identifier"],
        ["123e4567-e89b-12d3-a456-426614174000", "00000000-0000-0000-0000-000000000000"],
        ["123e4567e89b12d3a456426614174000", "123e4567-e89b-12d3"],
    ),
    (
        "Hex",
        "Alphanumeric",
        64,
        "m",
        ["valor em hexadecimal", "hexadecimal", "hex", "número em base 16"],
        ["hex value", "hexadecimal literal", "hex number", "base 16 value"],
        ["0x1f", "deadbeef"],
        ["0xzz", "0x1g"],
    ),
    (
        "Base64",
        "PrintableAscii",
        256,
        "m",
        ["base64", "texto base64", "conteúdo base64", "payload base64"],
        ["base64 payload", "base64 string", "base64 content", "base64 text"],
        ["SGVsbG8=", "TWFu"],
        ["SGVsbG8!", "SGVs bG8="],
    ),
    (
        "Base32",
        "Alphanumeric",
        64,
        "m",
        ["base32", "texto base32", "conteúdo base32", "payload base32"],
        ["base32 payload", "base32 string", "base32 content", "base32 text"],
        ["JBSWY3DP", "MFRGG==="],
        ["jbswy3dp", "JBSWY3D1"],
    ),
    (
        "Slug",
        "Word",
        64,
        "m",
        ["slug", "slug de página", "identificador de página", "slug amigável"],
        ["slug", "permalink slug", "page slug", "friendly identifier"],
        ["my-page_title", "post-1"],
        ["My Page", "post-"],
    ),
    (
        "Username",
        "Word",
        32,
        "m",
        ["username", "nome de usuário", "login", "nome de login"],
        ["username", "login name", "user name", "screen name"],
        ["user_name", "user.name"],
        ["user name", "us#er"],
    ),
    (
        "CreditCard",
        "Digits",
        19,
        "m",
        ["número de cartão de crédito", "cartão de crédito", "cartão", "número de cartão"],
        ["credit card number", "debit card number", "card number", "payment card number"],
        ["4111 1111 1111 1111", "4111111111111111"],
        ["4111 1111 1111", "4111 1111 1111 111a"],
    ),
    (
        "Iban",
        "Alphanumeric",
        34,
        "m",
        ["iban", "conta internacional", "iban bancário", "conta bancária internacional"],
        ["iban", "international bank account number", "bank account iban", "iban code"],
        ["DE89370400440532013000", "GB82WEST12345698765432"],
        ["de89370400440532013000", "89370400440532013000"],
    ),
    (
        "UsZipCode",
        "Digits",
        10,
        "m",
        ["zip code", "código postal americano", "cep americano", "zip dos EUA"],
        ["us zip code", "united states zip code", "american zip code", "us zip"],
        ["94105", "94105-1234"],
        ["9410", "94105-123"],
    ),
    (
        "Integer",
        "Digits",
        20,
        "m",
        ["número inteiro", "inteiro", "valor inteiro", "número inteiro com sinal"],
        ["integer", "signed integer", "whole number", "integer value"],
        ["-42", "1234"],
        ["12.5", "abc"],
    ),
    (
        "Decimal",
        "Digits",
        32,
        "m",
        ["número decimal", "decimal", "valor decimal", "número com casas decimais"],
        ["decimal number", "decimal value", "fractional number", "decimal"],
        ["3.14", "-0.5"],
        ["3.14.1", "abc"],
    ),
    (
        "CurrencyAmount",
        "PrintableAscii",
        32,
        "m",
        ["valor monetário", "preço", "valor em dinheiro", "montante"],
        ["currency amount", "monetary value", "price amount", "money value"],
        ["$1,234.56", "R$ 10,00"],
        ["abc", "EUR"],
    ),
    (
        "Json",
        "Any",
        4096,
        "m",
        ["json", "documento json", "resposta json", "payload json"],
        ["json document", "json payload", "json text", "json"],
        ['{"a": 1}', "[1, 2, 3]"],
        ['{"a": 1', "{a: 1}"],
    ),
]

# Each extraction entry is (kind, pt_nouns, en_nouns, positives, negatives).
EXTRACTION_CORPUS = [
    (
        "BetweenDelimiters",
        [
            "texto entre colchetes",
            "conteúdo entre parênteses",
            "números entre colchetes",
            "trecho entre dois marcadores",
        ],
        [
            "the text between brackets",
            "the content between parentheses",
            "the values between the delimiters",
            "the part between the markers",
        ],
        ["o valor [interessante] fica aqui", "[abc] e [def]"],
        ["sem delimitador nenhum", "[abc"],
    ),
    (
        "Quoted",
        [
            "texto entre aspas",
            "frase entre aspas",
            "conteúdo entre aspas duplas",
            "citação entre aspas",
        ],
        [
            "the quoted text",
            "the string inside the quotes",
            "the text in quotation marks",
            "the quoted words",
        ],
        ['ele disse "ola" hoje', "'aspas simples' aqui"],
        ["sem aspas aqui", '"aberta'],
    ),
    (
        "Numbers",
        ["números", "valores numéricos", "quantidades", "cifras"],
        ["the numbers", "the numeric values", "the quantities", "the figures"],
        ["tenho 42 anos e 3 filhos", "notas 1 2 3"],
        ["nenhum numero aqui", "so letras"],
    ),
    (
        "Words",
        ["palavras", "termos", "vocabulário", "expressões"],
        ["the words", "the terms", "the vocabulary", "the expressions"],
        ["o rato roeu a roupa", "palavras soltas aqui"],
        ["12345", "???"],
    ),
    (
        "Urls",
        ["urls", "links", "endereços web", "endereços da internet"],
        ["the urls", "the links", "the web addresses", "the http addresses"],
        ["visite https://example.com agora", "http://a.org e https://b.org"],
        ["sem link nenhum", "example.com solto"],
    ),
    (
        "Emails",
        ["emails", "endereços de email", "contatos de email", "caixas de email"],
        ["the emails", "the email addresses", "the e-mail contacts", "the mail addresses"],
        ["fale com ana@example.com", "a@b.org e c@d.net"],
        ["sem email nenhum", "@example"],
    ),
    (
        "IpAddresses",
        ["endereços ip", "ips", "endereços de rede", "endereços ipv4 e ipv6"],
        ["the ip addresses", "the ips", "the network addresses", "the addresses of the hosts"],
        ["o host 192.168.0.1 respondeu", "10.0.0.1 e 10.0.0.2"],
        # An extractor finds an address anywhere, so a value with three
        # groups is the negative: there is no address in it at all.
        ["sem ip nenhum", "1.2.3"],
    ),
    (
        "Dates",
        ["datas", "dias e meses", "datas em formato iso", "datas citadas"],
        ["the dates", "the days and months", "the dates mentioned", "the dates written out"],
        ["a reuniao e 2026-10-29", "em 2026-10-29 as 10h"],
        ["sem data nenhuma", "30/02/2026"],
    ),
    (
        "AfterKeyword",
        [
            "trecho depois da palavra-chave",
            "conteúdo seguinte à palavra-chave",
            "valor depois do rótulo",
            "trecho posterior à palavra chave",
        ],
        [
            "what comes after the keyword",
            "the text following the keyword",
            "the value after the label",
            "the part after the marker",
        ],
        # The keyword and the value are separated by a colon, which is what
        # the engine expects: without it there is no way to tell where the
        # keyword ends and the value begins.
        ["total: 42 itens", "valor: 10 reais"],
        ["sem total", "total"],
    ),
    (
        "BeforeKeyword",
        [
            "trecho antes da palavra-chave",
            "conteúdo anterior à palavra-chave",
            "valor antes do rótulo",
            "trecho anterior à palavra chave",
        ],
        [
            "what comes before the keyword",
            "the text preceding the keyword",
            "the value before the label",
            "the part before the marker",
        ],
        ["42 itens: total", "10 reais: valor"],
        ["sem total", "total"],
    ),
]

# The singular form of every extraction noun.
#
# Asking for one value is the common case, and the tokenizer does not stem, so
# "the url" and "the urls" are different features.  Each class therefore gets
# its own phrasings for the singular form.
EXTRACTION_SINGULAR = {
    "BetweenDelimiters": ("o texto entre colchetes", "the text between brackets"),
    "Quoted": ("o texto entre aspas", "the quoted text"),
    "Numbers": ("o número", "the number"),
    "Words": ("a palavra", "the word"),
    "Urls": ("a url", "the url"),
    "Emails": ("o email", "the email address"),
    "IpAddresses": ("o endereço ip", "the ip address"),
    "Dates": ("a data", "the date"),
    "AfterKeyword": (
        "o trecho depois da palavra-chave",
        "the part after the keyword",
    ),
    "BeforeKeyword": (
        "o trecho antes da palavra-chave",
        "the part before the keyword",
    ),
}
# Each filter entry is (action, negatives, requests).  A request is a
# (text, terms, positives) triple: the terms are the ones the rule engine
# must extract, and the positives are the values that contain them.  The
# positives belong to the request, not to the action, because every request
# filters on its own words.
FILTER_CORPUS = [
    (
        "Block",
        ["uma mensagem normal", "nada suspeito"],
        [
            ("bloquear as palavras spam e phishing", ["spam", "phishing"],
             ["spam", "o teste de phishing"]),
            ("filtrar mensagens que contenham a palavra golpe", ["golpe"],
             ["golpe", "um golpe grande"]),
            ("rejeitar entradas que contenham a palavra erro", ["erro"],
             ["erro", "um erro grave"]),
            ("block messages containing the word spam", ["spam"],
             ["spam", "this is spam"]),
            ("filter out any line with the term forbidden", ["forbidden"],
             ["forbidden", "this is forbidden"]),
            ("ban any input with the word hack", ["hack"],
             ["hack", "a hack attempt"]),
            ("remover linhas que tenham a palavra segredo", ["segredo"],
             ["segredo", "o segredo"]),
        ],
    ),
    (
        "Allow",
        ["uma mensagem normal", "nada suspeito"],
        [
            ("permitir apenas mensagens sem a palavra propaganda", ["propaganda"],
             ["propaganda", "a propaganda de sempre"]),
            ("aceitar somente mensagens com a palavra parabens", ["parabens"],
             ["parabens", "um parabens aqui"]),
            ("allow any message containing the word notice", ["notice"],
             ["notice", "a notice here"]),
            ("permitir textos que contenham a palavra aviso", ["aviso"],
             ["aviso", "um aviso aqui"]),
        ],
    ),
    (
        "Highlight",
        ["uma mensagem normal", "nada suspeito"],
        [
            ("destacar as linhas com a palavra urgente", ["urgente"],
             ["urgente", "isso e urgente demais"]),
            ("highlight texts containing the word review", ["review"],
             ["review", "a review of the text"]),
            ("marcar as mensagens que contem a palavra pendencia", ["pendencia"],
             ["pendencia", "uma pendencia aberta"]),
            ("realcar qualquer texto com a palavra alerta", ["alerta"],
             ["alerta", "um alerta importante"]),
        ],
    ),
    (
        "Count",
        ["uma mensagem normal", "nada suspeito"],
        [
            ("contar quantas mensagens contem a palavra teste", ["teste"],
             ["teste", "um teste de contagem"]),
            ("count how many rows contain the word error", ["error"],
             ["error", "a counting error"]),
            ("contagem de mensagens com a palavra duplicada", ["duplicada"],
             ["duplicada", "uma duplicada aqui"]),
        ],
    ),
]

# Each custom entry is (shape, positives, negatives, requests).  The shape is
# ("Repeat", charset, count) or ("LiteralThenClass", prefix, charset, count).
CUSTOM_CORPUS = [
    (
        ("Repeat", "Alphanumeric", 6),
        ["a1b2c3", "XXXXXX"],
        ["a1b2c", "a1b2c3d4"],
        [
            "criar um padr\u00e3o de 6 caracteres alfanum\u00e9ricos",
            "build a custom pattern of 6 alphanumeric characters",
            "montar um padr\u00e3o de 6 caracteres",
        ],
    ),
    (
        ("Repeat", "Digits", 3),
        ["123", "007"],
        ["12", "1234"],
        [
            "montar um padr\u00e3o com 3 d\u00edgitos",
            "padr\u00e3o customizado de 3 n\u00fameros",
            "make a pattern with 3 digits",
        ],
    ),
    (
        ("Repeat", "Letters", 8),
        ["abcdefgh", "ABCDEFGH"],
        ["abcdefg", "abcdefghi"],
        [
            "um c\u00f3digo de 8 letras",
            "make a pattern with 8 letters",
            "padr\u00e3o de 8 letras",
        ],
    ),
    (
        ("Repeat", "Digits", 4),
        ["2026", "0000"],
        ["202", "20261"],
        [
            "padr\u00e3o customizado de 4 n\u00fameros",
            "criar um padr\u00e3o com 4 d\u00edgitos",
            "build a custom pattern of 4 digits",
        ],
    ),
    (
        ("Repeat", "Alphanumeric", 12),
        ["a1b2c3d4e5f6", "ABCDEFGHIJKL"],
        ["a1b2c3d4e5f", "a1b2c3d4e5f6g7"],
        [
            "um padr\u00e3o de 12 caracteres",
            "build a custom pattern of 12 alphanumeric characters",
            "criar um padr\u00e3o com 12 caracteres",
        ],
    ),
    (
        ("Repeat", "PrintableAscii", 2),
        ["!!", "??"],
        ["!", "!!!"],
        [
            "padr\u00e3o de 2 s\u00edmbolos",
            "build a custom pattern of 2 symbols",
            "criar um padr\u00e3o com 2 s\u00edmbolos",
        ],
    ),
    (
        ("Repeat", "Digits", 7),
        ["1234567", "7654321"],
        ["123456", "12345678"],
        [
            "criar um padr\u00e3o com 7 n\u00fameros",
            "make a pattern with 7 numbers",
            "padr\u00e3o de 7 d\u00edgitos",
        ],
    ),
    (
        ("LiteralThenClass", "abc", "Digits", 2),
        ["abc12", "abc34"],
        ["abc1", "abc123"],
        [
            "criar um padr\u00e3o com prefixo ABC e 2 d\u00edgitos",
            "custom pattern with prefix ABC and 2 digits",
            "montar um padr\u00e3o com prefixo abc e 2 d\u00edgitos",
        ],
    ),
    (
        ("LiteralThenClass", "sku", "Letters", 5),
        ["skuabcde", "skuxxxxx"],
        ["skuabcd", "skuabcdef"],
        [
            "padr\u00e3o que come\u00e7a com SKU seguido de 5 letras",
            "custom pattern starting with SKU and 5 letters",
            "montar um padr\u00e3o que come\u00e7a com sku e 5 letras",
        ],
    ),
    (
        ("LiteralThenClass", "ab", "Digits", 3),
        ["ab123", "ab456"],
        ["ab12", "ab1234"],
        [
            "custom template starting with AB and 3 digits",
            "custom pattern with prefix AB and 3 digits",
            "montar um padr\u00e3o com prefixo AB e 3 d\u00edgitos",
        ],
    ),
]

# ---------------------------------------------------------------------------
# Class table
# ---------------------------------------------------------------------------

VALIDATION_KINDS = [
    "Email",
    "Url",
    "Uri",
    "DomainName",
    "Ipv4",
    "Ipv6",
    "MacAddress",
    "BrazilianCpf",
    "BrazilianCnpj",
    "BrazilianPostalCode",
    "BrazilianPhone",
    "BrazilianLicensePlate",
    "PortugueseNif",
    "UsSsn",
    "Semver",
    "IsoDate",
    "IsoDateTime",
    "ClockTime",
    "Uuid",
    "Hex",
    "Base64",
    "Base32",
    "Slug",
    "Username",
    "CreditCard",
    "Iban",
    "UsZipCode",
    "Integer",
    "Decimal",
    "CurrencyAmount",
    "Json",
]

EXTRACTION_KINDS = [
    "BetweenDelimiters",
    "Quoted",
    "Numbers",
    "Words",
    "Urls",
    "Emails",
    "IpAddresses",
    "Dates",
    "AfterKeyword",
    "BeforeKeyword",
]

CLASS_NAMES = (
    ["validate:" + kind for kind in VALIDATION_KINDS]
    + ["extract:" + kind for kind in EXTRACTION_KINDS]
    + ["filter", "custom"]
)


# ---------------------------------------------------------------------------
# Corpus expansion
# ---------------------------------------------------------------------------


def _validate_spec(kind: str, charset: str, max_len: int, request: str) -> dict:
    return {
        "Validate": {
            "kind": kind,
            "strictness": strictness_of(request),
            "charset": charset,
            "max_len": max_len,
        }
    }


def _extract_spec(kind: str, request: str) -> dict:
    default_capture = "Whole" if kind in (
        "Numbers",
        "Words",
        "Urls",
        "Emails",
        "IpAddresses",
        "Dates",
    ) else "Group"
    return {
        "Extract": {
            "kind": kind,
            "boundary": boundary_of(request),
            "capture": capture_of(request, default_capture),
        }
    }


def _filter_spec(action: str, terms: list[tuple[str, str]], request: str) -> dict:
    # The bytes are written as the list of their values, which is the form
    # `serde` gives a `SmallVec<u8>`.  That way a record of the dataset can be
    # read back by the types of `regie-core` without any translation.
    return {
        "Filter": {
            "action": action,
            "terms": [
                {"kind": kind, "bytes": list(text.encode("utf-8"))}
                for kind, text in terms
            ],
        }
    }


def _custom_spec(shape: tuple, request: str) -> dict:
    resolved = custom_shape(request)
    if resolved[0] == "LiteralThenClass":
        _, prefix, charset, count = resolved
        template = {
            "LiteralThenClass": {
                "literal": list(prefix.encode("utf-8")),
                "class": charset,
                "count": count,
            }
        }
        length = len(prefix) + count
    else:
        _, charset, count = resolved
        template = {"Repeat": {"class": charset, "count": count}}
        length = count
    return {
        "Custom": {
            "template": template,
            "params": {
                "case_sensitive": True,
                "multiline": False,
                "dot_all": False,
                # The shape has an exact length, so the value has to have it
                # too: "a pattern of 6 characters" describes the whole value,
                # not a run of six characters somewhere inside it.
                "anchored_start": True,
                "anchored_end": True,
                "min_len": length,
                "max_len": length,
            },
        }
    }


def _render(templates, nouns, article: str | None = None) -> list[str]:
    """Renders every template against every noun, rotating the nouns."""
    out: list[str] = []
    for index, template in enumerate(templates):
        noun = nouns[index % len(nouns)]
        if article is None:
            out.append(template.format(n=noun))
        else:
            out.append(template.format(n=noun, art=article))
    return out


def build_examples() -> list[dict]:
    """Expands the corpus into dataset records."""
    records: list[dict] = []
    for (
        kind,
        charset,
        max_len,
        gender,
        pt_nouns,
        en_nouns,
        positives,
        negatives,
    ) in VALIDATION_CORPUS:
        article = "o" if gender == "m" else "a"
        requests = _render(VALIDATION_TEMPLATES_PT, pt_nouns, article)
        requests += _render(VALIDATION_TEMPLATES_EN, en_nouns)
        for request in requests:
            records.append(
                {
                    "request": request,
                    "spec": _validate_spec(kind, charset, max_len, request),
                    "positives": list(positives),
                    "negatives": list(negatives),
                }
            )
    for kind, pt_nouns, en_nouns, positives, negatives in EXTRACTION_CORPUS:
        requests = _render(EXTRACTION_TEMPLATES_PT, pt_nouns)
        requests += _render(EXTRACTION_TEMPLATES_EN, en_nouns)
        # The singular forms, in the phrasings that ask for a single value.
        pt_singular, en_singular = EXTRACTION_SINGULAR[kind]
        requests.append(EXTRACTION_TEMPLATES_PT_EXTRA[0].format(n=pt_singular))
        requests.append(EXTRACTION_TEMPLATES_PT_EXTRA[1].format(n=pt_singular))
        requests.append(EXTRACTION_TEMPLATES_EN_EXTRA[0].format(n=en_singular))
        requests.append(EXTRACTION_TEMPLATES_EN_EXTRA[1].format(n=en_singular))
        for request in requests:
            records.append(
                {
                    "request": request,
                    "spec": _extract_spec(kind, request),
                    "positives": list(positives),
                    "negatives": list(negatives),
                }
            )
    for action, negatives, requests in FILTER_CORPUS:
        for request, expected_terms, positives in requests:
            computed_terms = filter_terms(request)
            computed_action = filter_action(request)
            if computed_action != action:
                raise SystemExit(
                    "corpus/rule mismatch for %r: action=%r != %r"
                    % (request, computed_action, action)
                )
            if [text for _, text in computed_terms] != list(expected_terms):
                raise SystemExit(
                    "corpus/rule mismatch for %r: terms=%r != %r"
                    % (request, computed_terms, expected_terms)
                )
            records.append(
                {
                    "request": request,
                    "spec": _filter_spec(action, computed_terms, request),
                    "positives": list(positives),
                    "negatives": list(negatives),
                }
            )
    for shape, positives, negatives, requests in CUSTOM_CORPUS:
        for request in requests:
            computed = custom_shape(request)
            if computed != shape:
                raise SystemExit(
                    "corpus/rule mismatch for %r: %r != %r" % (request, computed, shape)
                )
            records.append(
                {
                    "request": request,
                    "spec": _custom_spec(shape, request),
                    "positives": list(positives),
                    "negatives": list(negatives),
                }
            )
    return records


def write_dataset(records: list[dict]) -> None:
    """Writes the dataset as JSON Lines."""
    DATASET.parent.mkdir(parents=True, exist_ok=True)
    with DATASET.open("w", encoding="utf-8", newline="\n") as handle:
        for record in records:
            handle.write(json.dumps(record, ensure_ascii=False, sort_keys=False))
            handle.write("\n")


def read_dataset() -> list[dict]:
    """Reads the dataset as JSON Lines."""
    records: list[dict] = []
    with DATASET.open("r", encoding="utf-8") as handle:
        for line in handle:
            line = line.strip()
            if not line:
                continue
            records.append(json.loads(line))
    return records


def class_of(record: dict) -> str:
    """Returns the class name of a dataset record."""
    spec = record["spec"]
    if "Validate" in spec:
        return "validate:" + spec["Validate"]["kind"]
    if "Extract" in spec:
        return "extract:" + spec["Extract"]["kind"]
    if "Filter" in spec:
        return "filter"
    if "Custom" in spec:
        return "custom"
    raise ValueError("unknown spec shape: %r" % (spec,))


# ---------------------------------------------------------------------------
# Training
# ---------------------------------------------------------------------------


def split_examples(records: list[dict]) -> tuple[list[int], list[int]]:
    """Splits the records into a training and a held-out index list.

    The split is per class and fully deterministic: the requests of a class
    keep their dataset order and every `EVAL_EVERY`-th one is held out.  The
    dataset is generated template by template, so this keeps every sentence
    shape in both halves instead of removing a whole shape from the training
    set (which would hide the words that identify the class).
    """
    by_class: dict[str, list[int]] = {}
    for index, record in enumerate(records):
        by_class.setdefault(class_of(record), []).append(index)
    train: list[int] = []
    eval_: list[int] = []
    for name in sorted(by_class):
        for position, index in enumerate(by_class[name]):
            if position % EVAL_EVERY == EVAL_EVERY - 1:
                eval_.append(index)
            else:
                train.append(index)
    return sorted(train), sorted(eval_)


def build_vocabulary(records: list[dict], indices: list[int]) -> list[str]:
    """Builds the feature vocabulary from the training split only.

    Every feature that occurs at least once is kept: the words that identify
    a class are rare by nature, and dropping them costs more accuracy than
    the extra dimensions cost in size.
    """
    names: set[str] = set()
    for index in indices:
        names.update(feature_names(records[index]["request"]))
    return sorted(names)


def fit_bernoulli_nb(
    records: list[dict],
    indices: list[int],
    labels: list[str],
    class_index: dict[str, int],
    vocab_index: dict[str, int],
    n_features: int,
    n_classes: int,
) -> tuple[list[list[float]], list[float]]:
    """Fits a Bernoulli Naive Bayes classifier over the binary features.

    The classifier is linear: `score(c) = bias(c) + sum(weight(c, f))` over
    the features that are present, with `weight(c, f)` the log-odds ratio of
    the feature for the class.  The fit is closed-form, so it needs no
    iteration, no seed and no shuffling: the same data always yields the
    same weights, bit for bit.
    """
    df = [[0] * n_features for _ in range(n_classes)]
    df_other = [0] * n_features
    class_count = [0] * n_classes
    for index in indices:
        c = class_index[labels[index]]
        class_count[c] += 1
        for f in set(
            vocab_index[name]
            for name in feature_names(records[index]["request"])
            if name in vocab_index
        ):
            df[c][f] += 1
            df_other[f] += 1
    total = len(indices)
    weights = [[0.0] * n_classes for _ in range(n_features)]
    bias = [0.0] * n_classes
    for c in range(n_classes):
        own = class_count[c]
        rest = total - own
        bias[c] = math.log(own / total)
        for f in range(n_features):
            p_own = (df[c][f] + SMOOTHING) / (own + 2 * SMOOTHING)
            p_rest = (df_other[f] - df[c][f] + SMOOTHING) / (rest + 2 * SMOOTHING)
            weights[f][c] = math.log(p_own / p_rest)
    # The tables are class-major from here on.
    class_major = [[weights[f][c] for f in range(n_features)] for c in range(n_classes)]
    return class_major, bias


def stratified_folds(records: list[dict]) -> list[list[int]]:
    """Splits the records into `EVAL_EVERY` stratified folds.

    Every class contributes one request to each fold, so a fold always holds
    the same mix of classes as the whole dataset.
    """
    by_class: dict[str, list[int]] = {}
    for index, record in enumerate(records):
        by_class.setdefault(class_of(record), []).append(index)
    folds: list[list[int]] = [[] for _ in range(EVAL_EVERY)]
    for name in sorted(by_class):
        for position, index in enumerate(by_class[name]):
            folds[position % EVAL_EVERY].append(index)
    return folds


def calibrate_thresholds(
    records: list[dict],
    labels: list[str],
    class_index: dict[str, int],
    n_classes: int,
) -> tuple[int, int]:
    """Estimates the decision thresholds with a stratified cross-validation.

    The thresholds have to hold for requests the model has never seen, so
    they are measured on held-out folds instead of on the data the weights
    were fitted on.  The folds are rebuilt from scratch - including the
    vocabulary - so the estimate matches what the shipped model sees.
    """
    folds = stratified_folds(records)
    margins: list[float] = []
    tops: list[float] = []
    for fold in range(EVAL_EVERY):
        held_out = folds[fold]
        kept = set(held_out)
        fit_ids = [index for index in range(len(records)) if index not in kept]
        vocab = build_vocabulary(records, fit_ids)
        vocab_index = {name: index for index, name in enumerate(vocab)}
        weights, bias = fit_bernoulli_nb(
            records, fit_ids, labels, class_index, vocab_index, len(vocab), n_classes
        )
        qw, qb, _ = quantize(weights, bias)
        for index in held_out:
            ids = []
            for name in feature_names(records[index]["request"]):
                position = vocab_index.get(name)
                if position is not None:
                    ids.append(position)
            scores = score(qw, qb, ids)
            order = rank(scores)
            if order[0] != class_index[labels[index]]:
                continue
            margins.append(float(scores[order[0]] - scores[order[1]]))
            tops.append(float(scores[order[0]]))
    margin_threshold = int(round(percentile(margins, 0.05)))
    min_score = int(round(percentile(tops, 0.01)))
    return margin_threshold, min_score


def quantize(
    weights: list[list[float]], bias: list[float]
) -> tuple[list[list[int]], list[int], float]:
    """Quantizes the weights and the biases to integers.

    All classes share one scale, so the quantization preserves the order of
    the scores exactly: the argmax and the margin comparison stay valid.
    """
    largest = 0.0
    for row in weights:
        for value in row:
            magnitude = abs(value)
            if magnitude > largest:
                largest = magnitude
    scale = largest / QUANT_LEVELS if largest > 0.0 else 1.0
    limit = QUANT_LEVELS
    qw = []
    for row in weights:
        qw.append(
            [max(-limit - 1, min(limit, int(round(v / scale)))) for v in row]
        )
    qb = [int(round(v / scale)) for v in bias]
    return qw, qb, scale


def score(qw: list[list[int]], qb: list[int], feature_ids: list[int]) -> list[int]:
    """Scores every class with the quantized model."""
    scores = []
    for c in range(len(qb)):
        acc = qb[c]
        row = qw[c]
        for f in feature_ids:
            acc += row[f]
        scores.append(acc)
    return scores


def rank(scores: list[int]) -> list[int]:
    """Returns the class indices ordered by score, best first."""
    return sorted(range(len(scores)), key=lambda c: (-scores[c], c))


def percentile(values: list[float], fraction: float) -> float:
    """Returns the `fraction`-th percentile of `values` (nearest rank)."""
    if not values:
        return 0.0
    ordered = sorted(values)
    position = int(len(ordered) * fraction)
    if position >= len(ordered):
        position = len(ordered) - 1
    return ordered[position]


# ---------------------------------------------------------------------------
# Rust code generation
# ---------------------------------------------------------------------------


def _rust_string(value: str) -> str:
    escaped = value.replace("\\", "\\\\").replace('"', '\\"')
    return '"%s"' % escaped


def format_generated(path: Path) -> None:
    """Runs rustfmt over a generated file so that it matches the repository.

    The tables are written as plain text and rustfmt reflows the long rows of
    the weight matrix.  Formatting here keeps a fresh run byte-identical to the
    file that is checked in, and keeps the format check of the CI green.  When
    rustfmt is not installed the file is left exactly as it was written.
    """
    rustfmt = shutil.which("rustfmt")
    if rustfmt is None:
        print("rustfmt not found: %s was left unformatted" % path.name)
        return
    result = subprocess.run(
        [rustfmt, "--edition", "2021", str(path)],
        capture_output=True,
        text=True,
    )
    if result.returncode != 0:
        print("rustfmt failed on %s: %s" % (path.name, result.stderr.strip()))


def render_weights_rs(
    vocab: list[str],
    qw: list[list[int]],
    qb: list[int],
    margin_threshold: int,
    min_score: int,
    scale: float,
) -> str:
    """Renders the quantized tables as a Rust source file."""
    out: list[str] = []
    out.append("//! @generated by `tools/train_intent.py` - do not edit by hand.")
    out.append("//!")
    out.append("//! Regenerate with `python tools/train_intent.py`.")
    out.append("//!")
    out.append("//! The classifier is a Bernoulli Naive Bayes over binary lexical")
    out.append("//! features.  Every weight and bias is an integer in the same unit, so")
    out.append("//! the argmax and the confidence margin are exact integer operations")
    out.append("//! and give the same answer on every platform.")
    out.append("//!")
    out.append("//! Vocabulary: %d entries, sorted by UTF-8 byte order." % len(vocab))
    out.append("")
    out.append("/// The number of intent classes the model discriminates.")
    out.append("pub const CLASS_COUNT: usize = %d;" % len(qb))
    out.append("")
    out.append("/// The number of lexical features in the vocabulary.")
    out.append("pub const FEATURE_COUNT: usize = %d;" % len(vocab))
    out.append("")
    out.append("/// The stable name of every class, in weight-table order.")
    out.append("pub static CLASS_NAMES: [&str; CLASS_COUNT] = [")
    for name in CLASS_NAMES:
        out.append("    %s," % _rust_string(name))
    out.append("];")
    out.append("")
    out.append("/// The lexical feature vocabulary, sorted by UTF-8 byte order.")
    out.append("pub static VOCAB: [&str; FEATURE_COUNT] = [")
    for name in vocab:
        out.append("    %s," % _rust_string(name))
    out.append("];")
    out.append("")
    out.append("/// The quantized weight of every (class, feature) pair.")
    out.append("pub static WEIGHTS: [[i8; FEATURE_COUNT]; CLASS_COUNT] = [")
    for row in qw:
        out.append("    [")
        for start in range(0, len(row), 32):
            chunk = row[start : start + 32]
            out.append("        " + ", ".join("%d" % value for value in chunk) + ",")
        out.append("    ],")
    out.append("];")
    out.append("")
    out.append("/// The quantized bias of every class.")
    out.append("pub static BIASES: [i32; CLASS_COUNT] = [")
    for start in range(0, len(qb), 8):
        chunk = qb[start : start + 8]
        out.append("    " + ", ".join("%d" % value for value in chunk) + ",")
    out.append("];")
    out.append("")
    out.append("/// The minimum score gap between the best and the runner-up class.")
    out.append("pub const MARGIN_THRESHOLD: i32 = %d;" % margin_threshold)
    out.append("")
    out.append("/// The minimum score the best class must reach to be accepted.")
    out.append("pub const MIN_SCORE: i32 = %d;" % min_score)
    out.append("")
    out.append("/// The number of bytes occupied by the embedded weight tables.")
    out.append("pub const fn weight_bytes() -> usize {")
    out.append("    CLASS_COUNT * FEATURE_COUNT * core::mem::size_of::<i8>()")
    out.append("        + CLASS_COUNT * core::mem::size_of::<i32>()")
    out.append("}")
    out.append("")
    out.append("/// The quantization step the integer tables were produced with.")
    # The shortest form that reads back as the same number, so that the
    # literal carries no more digits than the value needs.
    out.append("pub const QUANT_SCALE: f64 = %r;" % scale)
    out.append("")
    return "\n".join(out)


# ---------------------------------------------------------------------------
# Entry point
# ---------------------------------------------------------------------------


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--emit-dataset",
        action="store_true",
        help="regenerate datasets/intent_train.jsonl from the built-in corpus",
    )
    args = parser.parse_args(argv)

    if args.emit_dataset:
        records = build_examples()
        write_dataset(records)
        print("wrote %d records to %s" % (len(records), DATASET))
        return 0

    records = read_dataset()
    if not records:
        print("empty dataset: %s" % DATASET, file=sys.stderr)
        return 1

    labels = [class_of(record) for record in records]
    class_index = {name: index for index, name in enumerate(CLASS_NAMES)}
    for name in labels:
        if name not in class_index:
            print("unknown class in dataset: %s" % name, file=sys.stderr)
            return 1

    train_ids, eval_ids = split_examples(records)
    vocab = build_vocabulary(records, train_ids)
    vocab_index = {name: index for index, name in enumerate(vocab)}

    def feature_ids(index: int) -> list[int]:
        ids = []
        for name in feature_names(records[index]["request"]):
            position = vocab_index.get(name)
            if position is not None:
                ids.append(position)
        return ids

    all_features = [feature_ids(index) for index in range(len(records))]

    weights, bias = fit_bernoulli_nb(
        records,
        train_ids,
        labels,
        class_index,
        vocab_index,
        len(vocab),
        len(CLASS_NAMES),
    )
    qw, qb, scale = quantize(weights, bias)

    # Decision thresholds: measured on held-out folds, so they hold for
    # requests the weights were not fitted on.
    margin_threshold, min_score = calibrate_thresholds(
        records, labels, class_index, len(CLASS_NAMES)
    )

    # Report.
    correct = 0
    resolved = 0
    per_class: dict[str, list[int]] = {name: [0, 0] for name in CLASS_NAMES}
    for index in eval_ids:
        scores = score(qw, qb, all_features[index])
        order = rank(scores)
        label = class_index[labels[index]]
        per_class[CLASS_NAMES[label]][1] += 1
        if order[0] == label:
            correct += 1
            per_class[CLASS_NAMES[label]][0] += 1
            if (
                scores[order[0]] - scores[order[1]] >= margin_threshold
                and scores[order[0]] >= min_score
            ):
                resolved += 1

    table_bytes = len(CLASS_NAMES) * len(vocab) + len(CLASS_NAMES) * 4
    # The linked footprint of the model: the weight table, the biases, the
    # class names and the vocabulary, with the fat pointers of the `&str`
    # arrays included, because they are linked just like the strings.
    class_name_bytes = sum(len(name.encode("utf-8")) for name in CLASS_NAMES)
    vocab_bytes = sum(len(name.encode("utf-8")) for name in vocab)
    footprint = (
        table_bytes
        + len(CLASS_NAMES) * 16
        + class_name_bytes
        + len(vocab) * 16
        + vocab_bytes
    )
    print("regie-intent training report")
    print("  examples            : %d" % len(records))
    print("  classes             : %d" % len(CLASS_NAMES))
    print("  train / eval split  : %d / %d" % (len(train_ids), len(eval_ids)))
    print("  vocabulary          : %d features" % len(vocab))
    print("  quantization        : i8, %d levels, step %.6e" % (QUANT_LEVELS, scale))
    print("  margin threshold    : %d" % margin_threshold)
    print("  minimum score       : %d" % min_score)
    print("  weight table        : %d bytes" % table_bytes)
    print("  linked footprint    : %d bytes" % footprint)
    print("  held-out accuracy   : %.4f (%d/%d)" % (
        correct / len(eval_ids), correct, len(eval_ids)))
    print("  held-out resolved   : %.4f (%d/%d)" % (
        resolved / len(eval_ids), resolved, len(eval_ids)))
    print("")
    print("  per-class accuracy on the held-out split:")
    for name in CLASS_NAMES:
        hits, total = per_class[name]
        if total == 0:
            continue
        flag = "" if hits == total else "   <-"
        print("    %-28s %2d/%2d%s" % (name, hits, total, flag))

    WEIGHTS.parent.mkdir(parents=True, exist_ok=True)
    WEIGHTS.write_text(
        render_weights_rs(vocab, qw, qb, margin_threshold, min_score, scale),
        encoding="utf-8",
        newline="\n",
    )
    format_generated(WEIGHTS)
    size = WEIGHTS.stat().st_size
    print("")
    print("wrote %s (%d bytes)" % (WEIGHTS, size))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
