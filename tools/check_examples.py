#!/usr/bin/env python3
"""Checks that every example in the documentation still behaves as documented.

The galleries under `crates/*/EXAMPLES.md` and `docs/EXAMPLES.md` quote the
output of the programs they document.  A quoted output goes stale the moment
the code changes, and a stale quote is worse than no quote at all, because a
reader trusts it.

This tool closes that gap.  It reads the galleries, runs every command it
finds, and compares what it observed with what the gallery claims.  A
mismatch is a failure.

What it understands is deliberately small:

* a fenced block tagged `console` whose first line starts with `$ ` is a
  command with its expected output;
* a command of the form `cargo run -p <crate> --example <name>` runs the
  example program that was built;
* a command of the form `cargo run -p <crate> --bin <name> -- <args>` runs
  that binary;
* a command of the form `python <script> <args>` runs the script;
* anything else is reported as "not checked" and never fails the run.

The observed output is the standard output followed by the standard error,
because the command line tool writes its diagnostics to the standard error
and a gallery quotes both.  Paths inside the repository are compared in their
repository-relative form, so a gallery that quotes an absolute path still
matches on another machine.

Usage:
    python tools/check_examples.py            # check every gallery
    python tools/check_examples.py --list     # only list what would be checked
"""

from __future__ import annotations

import argparse
import os
import re
import shlex
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
GALLERIES = (
    [ROOT / "README.md"]
    + sorted(ROOT.glob("docs/*.md"))
    + sorted(ROOT.glob("crates/*/README.md"))
    + sorted(ROOT.glob("crates/*/EXAMPLES.md"))
)

BLOCK = re.compile(r"^```console\s*$(.*?)^```\s*$", re.MULTILINE | re.DOTALL)


class Case:
    """One command and the output the gallery claims for it."""

    def __init__(self, gallery: Path, line: int, command: str, expected: list[str]):
        self.gallery = gallery
        self.line = line
        self.command = command
        self.expected = expected

    def __str__(self) -> str:
        return "%s:%d  %s" % (self.gallery.relative_to(ROOT), self.line, self.command)


def parse_gallery(path: Path) -> list[Case]:
    """Extracts the verifiable cases from one gallery."""
    text = path.read_text(encoding="utf-8")
    cases: list[Case] = []
    for match in BLOCK.finditer(text):
        body = match.group(1)
        line = text[: match.start()].count("\n") + 1
        current: list[str] | None = None
        for raw in body.splitlines():
            if raw.startswith("$ "):
                if current is not None:
                    cases.append(Case(path, line, current[0], current[1]))
                current = [raw[2:].strip(), []]
            elif current is not None:
                if raw.strip() == "":
                    current[1].append("")
                else:
                    current[1].append(raw)
        if current is not None:
            cases.append(Case(path, line, current[0], current[1]))
    return cases


def normalize(text: str) -> list[str]:
    """Puts an output into the form the comparison uses."""
    root = str(ROOT).replace("\\", "/")
    text = text.replace("\\", "/")
    text = text.replace(root + "/", "").replace(root, "")
    lines = [line.rstrip() for line in text.splitlines()]
    while lines and lines[-1] == "":
        lines.pop()
    return lines


def example_owners() -> dict[str, list[str]]:
    """Maps the name of every example to the crates that declare it.

    Cargo writes the examples of every crate of a workspace into one
    directory, so two crates that declare an example with the same name
    silently overwrite each other.  The checker has to know about that
    before it compares a documented output with the wrong program.
    """
    owners: dict[str, list[str]] = {}
    for directory in sorted(ROOT.glob("crates/*/examples")):
        crate = directory.parent.name
        for source in sorted(directory.glob("*.rs")):
            owners.setdefault(source.stem, []).append(crate)
    return owners


def build(target: Path) -> tuple[Path, str]:
    """Builds the examples and the binaries, and returns the target directory."""
    target.mkdir(parents=True, exist_ok=True)
    environment = dict(os.environ)
    environment["CARGO_TARGET_DIR"] = str(target)

    for arguments in (
        ["build", "--workspace", "--examples", "--all-features"],
        ["build", "--workspace", "--bins", "--all-features"],
    ):
        result = subprocess.run(
            ["cargo", *arguments],
            cwd=str(ROOT),
            env=environment,
            capture_output=True,
            text=True,
        )
        if result.returncode != 0:
            return target, result.stderr.strip()

    return target, ""


PLACEHOLDER = re.compile(r"^<[^>]*>$")


def resolve(command: str, target: Path) -> tuple[list[str], str] | None:
    """Turns a documented command into the program to run.

    Returns the argument vector and the working directory to run it in, or
    `None` when the command is not one this tool knows how to check.
    """
    parts = shlex.split(command)
    if not parts:
        return None

    # A token that is a placeholder marks a template, not a command.
    if any(PLACEHOLDER.match(part) for part in parts):
        return None

    if parts[0] == "cargo" and parts[1] == "run":
        rest = parts[2:]
        if "--" in rest:
            rest, arguments = rest[: rest.index("--")], rest[rest.index("--") + 1 :]
        else:
            arguments = []

        package = ""
        example = ""
        binary = ""
        index = 0
        while index < len(rest):
            item = rest[index]
            if item == "-p" and index + 1 < len(rest):
                package = rest[index + 1]
                index += 2
                continue
            if item == "--example" and index + 1 < len(rest):
                example = rest[index + 1]
                index += 2
                continue
            if item == "--bin" and index + 1 < len(rest):
                binary = rest[index + 1]
                index += 2
                continue
            index += 1

        if example:
            name = example + (".exe" if os.name == "nt" else "")
            path = target / "debug" / "examples" / name
            if not path.exists():
                return None
            # An example program may read files that live in the repository.
            return [str(path)], str(ROOT)
        if binary:
            name = binary + (".exe" if os.name == "nt" else "")
            path = target / "debug" / name
            if not path.exists():
                return None
            # A command that writes a file must not write it into the tree.
            return [str(path), *arguments], ""
        return None

    if parts[0] == "python" and len(parts) > 1:
        script = ROOT / parts[1]
        if not script.exists():
            return None
        return ["python", str(script), *parts[2:]], ""

    if parts[0] == "regie":
        name = "regie" + (".exe" if os.name == "nt" else "")
        path = target / "debug" / name
        if not path.exists():
            return None
        # A command that writes a file must not write it into the tree.
        return [str(path), *parts[1:]], ""

    return None


def run_producer(command: str) -> str:
    """Runs the left side of a pipeline and returns what it wrote.

    The documentation is written for a POSIX shell, where `echo "text"`
    writes `text`.  The Windows shell keeps the quotes, so the two would
    disagree; the difference is handled here.
    """
    parts = shlex.split(command)
    if parts and parts[0] == "echo":
        return " ".join(parts[1:]) + "\n"
    result = subprocess.run(
        command,
        shell=True,
        cwd=str(ROOT),
        capture_output=True,
        text=True,
    )
    return result.stdout


def observe(case: Case, target: Path, workspace: Path) -> tuple[list[str], int, str]:
    """Runs one case and returns what it printed."""
    command = case.command

    # A pipeline is split in two: the left side produces the input and the
    # right side is the program that is checked.
    producer = None
    if "|" in command:
        left, right = command.split("|", 1)
        producer = run_producer(left.strip())
        command = right.strip()

    resolved = resolve(command, target)
    if resolved is None:
        return [], 0, "not checked"

    argv, directory = resolved
    if directory == "":
        directory = str(workspace)

    result = subprocess.run(
        argv,
        cwd=directory,
        capture_output=True,
        text=True,
        # An example that reads the standard input has to see the end of it
        # at once, or the check waits for a key press that never comes.
        stdin=subprocess.DEVNULL if producer is None else None,
        input=producer,
    )
    combined = result.stdout + result.stderr
    return normalize(combined), result.returncode, ""


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--list",
        action="store_true",
        help="list the cases without running them",
    )
    parser.add_argument(
        "--target",
        help="the cargo target directory to build into",
    )
    args = parser.parse_args(argv)

    cases: list[Case] = []
    for gallery in GALLERIES:
        if gallery.exists():
            cases.extend(parse_gallery(gallery))

    if not cases:
        print("no gallery found", file=sys.stderr)
        return 1

    if args.list:
        for case in cases:
            print(case)
        print("%d cases" % len(cases))
        return 0

    target = Path(args.target) if args.target else ROOT / "target-examples"

    collisions = {
        name: crates for name, crates in example_owners().items() if len(crates) > 1
    }
    if collisions:
        print("two crates declare an example with the same name:", file=sys.stderr)
        for name, crates in sorted(collisions.items()):
            print("  %s: %s" % (name, ", ".join(crates)), file=sys.stderr)
        print(
            "cargo writes every example of a workspace into one directory, so "
            "one of the two silently replaces the other.",
            file=sys.stderr,
        )
        return 1

    target, failure = build(target)
    if failure:
        print("the build failed:", file=sys.stderr)
        print(failure, file=sys.stderr)
        return 1

    failures = 0
    unchecked = 0
    with tempfile.TemporaryDirectory() as temporary:
        workspace = Path(temporary)
        for case in cases:
            observed, code, note = observe(case, target, workspace)
            if note:
                unchecked += 1
                print("skip  %s (%s)" % (case, note))
                continue
            expected = normalize("\n".join(case.expected))
            # A gallery may shorten a long output with a trailing ellipsis.
            # The part that is shown still has to be exactly right.
            elided = bool(expected) and expected[-1].strip() == "..."
            if elided:
                expected = expected[:-1]
                matched = observed[: len(expected)] == expected
            else:
                matched = observed == expected
            if matched:
                print("ok    %s" % case)
            else:
                failures += 1
                print("FAIL  %s" % case)
                print("  expected:")
                for line in expected:
                    print("    | %s" % line)
                print("  observed:")
                for line in observed:
                    print("    | %s" % line)

    print("")
    print(
        "%d cases, %d failed, %d not checked"
        % (len(cases), failures, unchecked)
    )
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
