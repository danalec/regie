"""Counts the phrasings of the corpus per family and per language.

The corpus is rendered from templates, so the language of a record is known
from the template it came from: the Portuguese templates render over the
Portuguese nouns and the English templates over the English ones.  Counting
the rendered phrasings therefore measures the two languages exactly, with no
guesswork about which language a request is written in.

The numbers this prints are the ones
[`docs/LANGUAGES.md`](../docs/LANGUAGES.md) quotes.

Usage:

    python tools/measure_languages.py

The script is standard-library only and deterministic: it imports the
training script and counts what it would render.
"""

from __future__ import annotations

import pathlib
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT / "tools"))

import train_intent as corpus  # noqa: E402


def validation_families() -> list[tuple[str, int, int]]:
    """The phrasings of every validation family, in the two languages."""
    rows = []
    for entry in corpus.VALIDATION_CORPUS:
        kind = entry[0]
        pt_nouns = entry[4]
        en_nouns = entry[5]
        article = "o" if entry[3] == "m" else "a"
        portuguese = corpus._render(corpus.VALIDATION_TEMPLATES_PT, pt_nouns, article)
        english = corpus._render(corpus.VALIDATION_TEMPLATES_EN, en_nouns)
        rows.append((f"validate.{kind}", len(portuguese), len(english)))
    return rows


def extraction_families() -> list[tuple[str, int, int]]:
    """The phrasings of every extraction family, in the two languages."""
    rows = []
    for entry in corpus.EXTRACTION_CORPUS:
        kind = entry[0]
        portuguese = corpus._render(corpus.EXTRACTION_TEMPLATES_PT, entry[1])
        english = corpus._render(corpus.EXTRACTION_TEMPLATES_EN, entry[2])
        # The singular forms are two more phrasings in each language.
        portuguese_count = len(portuguese) + len(corpus.EXTRACTION_TEMPLATES_PT_EXTRA)
        english_count = len(english) + len(corpus.EXTRACTION_TEMPLATES_EN_EXTRA)
        rows.append((f"extract.{kind}", portuguese_count, english_count))
    return rows


def filter_families() -> list[tuple[str, int, int]]:
    """The phrasings of every filter family, in the two languages."""
    rows = []
    for entry in corpus.FILTER_CORPUS:
        action = entry[0]
        rows.append((f"filter.{action}", len(entry[1]), len(entry[2])))
    return rows


def custom_family() -> tuple[str, int, int]:
    """The phrasings of the custom shape, in the two languages.

    The requests of the custom family are written out by hand rather than
    rendered from a template, so the language is told by the words of the
    request itself.
    """
    portuguese = 0
    english = 0
    for entry in corpus.CUSTOM_CORPUS:
        for request in entry[3]:
            lowered = request.lower()
            if any(word in lowered for word in ("criar", "montar", "padrão", "padr")):
                portuguese += 1
            else:
                english += 1
    return ("custom", portuguese, english)


def main() -> int:
    rows = validation_families() + extraction_families() + filter_families()
    rows.append(custom_family())

    print(f'{"family":36} {"pt-BR":>6} {"en":>6}')
    for name, portuguese, english in sorted(rows):
        print(f"{name:36} {portuguese:>6} {english:>6}")

    portuguese_counts = [row[1] for row in rows]
    english_counts = [row[2] for row in rows]
    print()
    print(f"families measured: {len(rows)}")
    print(
        "portuguese per family : min "
        f"{min(portuguese_counts)} max {max(portuguese_counts)}"
    )
    print(
        "english per family    : min "
        f"{min(english_counts)} max {max(english_counts)}"
    )
    print(f"phrasings in total    : {sum(portuguese_counts) + sum(english_counts)}")

    # Every family has to have phrasings in both languages, or one of the two
    # would not really be supported.
    missing = [name for name, pt, en in rows if pt == 0 or en == 0]
    if missing:
        print()
        print("families without a phrasing in one of the languages:")
        for name in missing:
            print(f"  {name}")
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
