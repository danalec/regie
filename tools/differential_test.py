"""Differential test: compare `regie` against Python's `re`.

The two implementations follow the same leftmost-first semantics, so the
match spans must agree. The corpus avoids the places where the two
dialects differ on purpose:

* Python's `$` also matches before a trailing newline, `regie`'s `$` only
  matches at the very end (like Rust's regex crate);
* Python accepts `[^]]` (a `]` right after `^` is literal), `regie` requires
  it to be escaped;
* Python's `.` does not match a newline unless `re.DOTALL`, neither does
  `regie` unless the dot-all flag is set;
* Python supports possessive quantifiers (`a*+`), `regie` rejects them;
* Python reads `\01` as an octal escape, `regie` reads `\0` as NUL and
  treats the digit as a literal.

Usage:

    python tools/differential_test.py [--cases N] [--verbose]

The script compiles the `differential` example of `regie-core` once and
feeds it the corpus.
"""

from __future__ import annotations

import argparse
import os
import pathlib
import random
import re
import subprocess
import sys
import warnings

ROOT = pathlib.Path(__file__).resolve().parent.parent

PATTERNS = [
    r"a",
    r"ab",
    r"a|b",
    r"ab|a",
    r"a|ab",
    r"(a)(b)",
    r"(a|b)+",
    r"(ab|a)+",
    r"a*",
    r"a+",
    r"a?",
    r"a{2}",
    r"a{2,}",
    r"a{2,4}",
    r"a{2,4}?",
    r"[abc]",
    r"[a-c]+",
    r"[^abc]+",
    r"[a-zA-Z0-9_]+",
    r"\d+",
    r"\d{2,4}",
    r"\w+",
    r"\s+",
    r"\D+",
    r"\W+",
    r"\S+",
    r".",
    r"..",
    r"a.c",
    r"^a",
    r"a$",
    r"^ab$",
    r"\bab\b",
    r"foo|bar|baz",
    r"(foo|bar)+baz",
    r"x(y|z)?w",
    r"[0-9]{3}-[0-9]{2}",
    r"colou?r",
    r"(a|b|c)+d",
    r"a(b|c)*d",
    r"[abc]{2,3}",
    r"(?:ab)+",
    r"(a)(b)(c)",
    r"((a|b)c)+",
    r"\.",
    r"a\\b",
    r"[.]",
    r"[-a]+",
    r"[a-]+",
    # The flag directives.  A scoped one has to stay scoped: the flag must
    # not reach the pattern that follows the group.
    r"(?i)abc",
    r"(?i:abc)",
    r"(?s)a.b",
    r"(?s:a.b)",
    r"(?m)^b",
    r"(?m:^b)",
    r"(?is)a.c",
    r"(?i:a)b",
    r"(?i:a)B",
    r"(?-i:a)b",
    r"(?m:^a\n)",
    r"(?s:.)",
    r"(?i:a)(?i:b)c",
    r"a(?i:b)C",
    r"(?i)a(?-i:b)c",
    # The capture groups: the engine accepts as many as the limit allows.
    r"(a)(b)(c)(d)(e)(f)(g)(h)",
    r"((a)(b)(c)(d)(e)(f)(g))",
]

INPUTS = [
    "",
    "a",
    "b",
    "ab",
    "ba",
    "abc",
    "aab",
    "aaab",
    "xyz",
    "aaa",
    "aabbcc",
    "foo bar baz",
    "xxfooyybarzzbaz",
    "123-45",
    "1234-567",
    "color",
    "colour",
    "  spaces  ",
    "tab\tsep",
    "a.b",
    "a\\b",
    "line1\nline2",
    "concat",
    "a cat here",
    "the admin user",
    "CPF 123.456.789-09",
    "aaaaaaaaaaaaaaaaaaaaaaaaaaaaX",
    "ABC",
    "aBc",
    "a\nb",
]


def random_pattern(rng: random.Random, depth: int = 0) -> str:
    """Builds a random pattern that is usually valid in both dialects."""
    roll = rng.random()
    if depth >= 3 or roll < 0.30:
        return rng.choice("ab01x")
    if roll < 0.50:
        return random_pattern(rng, depth + 1) + random_pattern(rng, depth + 1)
    if roll < 0.65:
        return "(" + random_pattern(rng, depth + 1) + "|" + random_pattern(rng, depth + 1) + ")"
    if roll < 0.80:
        return "(" + random_pattern(rng, depth + 1) + ")" + rng.choice(["*", "+", "?", "{1,2}", "{2}", "{1,3}?"])
    if roll < 0.90:
        return "[" + rng.choice(["a-z", "0-9", "^a-c", "abc", "a-z0-9"]) + "]"
    return random_pattern(rng, depth + 1) + rng.choice(["\\d", "\\w", "\\s", "."])


def build_corpus(rng: random.Random, cases: int) -> list[tuple[str, str]]:
    corpus: list[tuple[str, str]] = []
    for pattern in PATTERNS:
        for text in INPUTS:
            corpus.append((pattern, text))
    # Random fuzzing over a small alphabet.
    alphabet = "abc01 .-"
    for _ in range(cases):
        pattern = random_pattern(rng)
        text = "".join(rng.choice(alphabet) for _ in range(rng.randint(0, 12)))
        corpus.append((pattern, text))
    return corpus


POSSESSIVE = re.compile(r"[*+?]\+")
OCTAL = re.compile(r"\\0[0-7]")
GLOBAL_FLAG = re.compile(r"\(\?[aiLmsux-]*\)")
CAPTURE_GROUP = re.compile(r"(?<!\\)\((?!\?)")


def dialect_difference(pattern: str) -> str | None:
    """Returns a reason when the two dialects differ on purpose."""
    if POSSESSIVE.search(pattern):
        return "possessive quantifier"
    if OCTAL.search(pattern):
        return "octal escape"
    # Python 3.11 refuses a global flag directive that is not at the start of
    # the expression.  The engine accepts one anywhere in a group.
    for match in GLOBAL_FLAG.finditer(pattern):
        if match.start() != 0:
            return "global flag directive"
    # The engine supports eight capture groups; Python supports a hundred.
    if len(CAPTURE_GROUP.findall(pattern)) > 8:
        return "capture group limit"
    return None


def python_span(pattern: str, text: str) -> tuple[int, int] | None:
    """Returns the span Python finds, or `None` when it finds nothing.

    Raises `re.error` when Python refuses the pattern.  The distinction
    matters: "Python finds no match" and "Python refuses the pattern" are
    two different answers, and the engine has to give the same one.
    """
    with warnings.catch_warnings():
        warnings.simplefilter("ignore")
        compiled = re.compile(pattern)
    match = compiled.search(text)
    if match is None:
        return None
    return match.start(), match.end()


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--cases", type=int, default=4000, help="random cases to add")
    parser.add_argument("--verbose", action="store_true")
    parser.add_argument("--seed", type=int, default=20261029)
    args = parser.parse_args()

    rng = random.Random(args.seed)
    corpus = build_corpus(rng, args.cases)

    payload = "\n".join(
        f"{pattern.encode('utf-8').hex()} {text.encode('utf-8').hex()}"
        for pattern, text in corpus
    )
    payload += "\n"

    binary = ROOT / "target-differential" / "debug" / "examples" / "differential.exe"
    if not binary.exists():
        binary = ROOT / "target-differential" / "debug" / "examples" / "differential"
    # The helper is always rebuilt: a stale binary would compare the current
    # patterns against an older engine and report differences that no longer
    # exist.
    print("building the differential helper...", file=sys.stderr)
    subprocess.run(
        [
            "cargo",
            "build",
            "--manifest-path",
            str(ROOT / "Cargo.toml"),
            "-p",
            "regie-core",
            "--example",
            "differential",
        ],
        check=True,
        env={**dict(__import__("os").environ), "CARGO_TARGET_DIR": str(ROOT / "target-differential")},
    )
    binary = ROOT / "target-differential" / "debug" / "examples" / "differential.exe"
    if not binary.exists():
        binary = ROOT / "target-differential" / "debug" / "examples" / "differential"

    result = subprocess.run(
        [str(binary)],
        input=payload,
        capture_output=True,
        text=True,
    )
    lines = result.stdout.strip().splitlines()
    if len(lines) != len(corpus):
        print(
            f"expected {len(corpus)} results, got {len(lines)}\n{result.stderr}",
            file=sys.stderr,
        )
        return 1

    mismatches = 0
    dialect = 0
    checked = 0
    for (pattern, text), line in zip(corpus, lines):
        reason = dialect_difference(pattern)
        if reason is not None:
            dialect += 1
            continue
        try:
            expected = python_span(pattern, text)
        except re.error:
            # Python refused the pattern, so `regie` has to refuse it too.
            # Accepting what the reference rejects is a divergence, not a
            # difference of opinion.
            if line != "ERR":
                mismatches += 1
                print(
                    f"MISMATCH (accept) pattern={pattern!r} text={text!r}: "
                    f"python refused the pattern, regie accepted it"
                )
            else:
                checked += 1
            continue
        if line == "ERR":
            mismatches += 1
            print(f"MISMATCH (compile) pattern={pattern!r} text={text!r}: regie refused to compile")
            continue
        if line == "-":
            actual = None
        else:
            start, end = line.split()
            actual = (int(start), int(end))
        if actual != expected:
            mismatches += 1
            if mismatches <= 20 or args.verbose:
                print(
                    f"MISMATCH pattern={pattern!r} text={text!r}: "
                    f"python={expected} regie={actual}"
                )
        checked += 1

    print(
        f"checked {checked} cases, dialect differences: {dialect}, mismatches: {mismatches}"
    )
    return 1 if mismatches else 0


if __name__ == "__main__":
    raise SystemExit(main())
