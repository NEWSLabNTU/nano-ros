#!/usr/bin/env python3
"""A process substitution feeding a redirect drops its producer's exit status.  Issue 2424.

    while read -r row; do ...; done < <(python3 fixtures-manifest.py west-overrides)
    mapfile -t rows < <(producer)

`set -e` and `pipefail` see neither producer: bash runs it asynchronously and
the redirect's status is the CONSUMER's.  So a producer that crashes hands the
loop an empty or truncated stream, and the loop treats that as "no rows".
Issue 2424 is that shape: #3449 made `fixtures-manifest.py west-overrides`
raise `UnboundLocalError`, `zephyr-fixture-leaves.sh` read it through
`done < <(...)`, and every Zephyr row built with NO west overrides, silently,
until issue 2363's fix made that runner fail loudly.  It is the sibling of
issue 1249 (`check-set-e-bare-assignment`, a bare assignment) and issue 1466
(a `$(...)` in argument position), and neither of those gates can see it.

THE FIX.  Bash (>= 4.4) sets `$!` to the last process substitution's pid, so
`wait $!` returns the producer's status.  Write it as the NEXT statement:

    mapfile -t rows < <(producer)
    wait $! || { echo "producer failed" >&2; exit 1; }

    while read -r row; do ...; done < <(producer)
    wait $! || exit

Under `set -e` a bare `wait $!` is enough: it fails the script itself.

WHAT IS EXEMPT.  A producer whose head is `printf` or `echo`: a shell builtin
writing literal text, which has no failure to lose.

THE RATCHET.  The tree held 187 unchecked sites when this gate landed, most of
them `git`/`grep`/`find` heads and local functions.  Rewriting them all at once
is the mass rewrite issue 1249's gate refused to be.  So the per-file counts in
`.config/process-substitution-status-baseline.txt` may only FALL: a new
unchecked site fails, a removed one asks for `--update` so the baseline
follows it down, and a file not in the baseline may hold none.
"""

from __future__ import annotations

import importlib.util
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
BASELINE = ROOT / ".config" / "process-substitution-status-baseline.txt"

# `< <(` — input redirected from a process substitution.  `<(` as a plain
# argument (`diff <(a) <(b)`) is a different shape and is not counted here.
REDIRECT = re.compile(r"<\s*<\(")
WAIT = re.compile(r"^\s*(?:[;&|]+\s*)?wait\s+\"?\$!\"?(?:\s|;|$|\|\|)")
EXEMPT_HEADS = {"printf", "echo"}


def tracked_shell() -> list[str]:
    """The same file set issue 1249's gate reads, from its one spelling."""
    spec = importlib.util.spec_from_file_location(
        "set_e_gate", ROOT / "scripts" / "check-set-e-bare-assignment.py")
    mod = importlib.util.module_from_spec(spec)
    assert spec.loader is not None
    spec.loader.exec_module(mod)
    return mod.tracked_shell()


def substitution_end(lines: list[str], i: int, col: int) -> tuple[int, int]:
    """The (line, column) just past the `)` closing the substitution opened at
    `lines[i][col]` (the `(`).  Quotes are honoured; nested parens counted."""
    depth = 0
    quote = ""
    j = i
    k = col
    while j < len(lines):
        s = lines[j]
        while k < len(s):
            c = s[k]
            if quote:
                if c == "\\" and quote == '"':
                    k += 2
                    continue
                if c == quote:
                    quote = ""
            elif c in "'\"":
                quote = c
            elif c == "(":
                depth += 1
            elif c == ")":
                depth -= 1
                if depth == 0:
                    return j, k + 1
            k += 1
        j += 1
        k = 0
    return len(lines) - 1, len(lines[-1]) if lines else 0


def producer_head(text: str) -> str:
    words = text.strip().split()
    return words[0] if words else ""


def scan_text(text: str) -> list[tuple[int, str]]:
    """Every unchecked `< <(...)` site: (1-based line, the line's text)."""
    lines = text.splitlines()
    found: list[tuple[int, str]] = []
    for i, line in enumerate(lines):
        if line.lstrip().startswith("#"):
            continue
        for m in REDIRECT.finditer(line):
            paren = m.end() - 1
            end_line, end_col = substitution_end(lines, i, paren)
            body = line[m.end():] if end_line != i else line[m.end():end_col - 1]
            if producer_head(body) in EXEMPT_HEADS:
                continue
            rest = lines[end_line][end_col:]
            if WAIT.match(rest):
                continue
            nxt = end_line + 1
            while nxt < len(lines) and (not lines[nxt].strip()
                                        or lines[nxt].lstrip().startswith("#")):
                nxt += 1
            if nxt < len(lines) and WAIT.match(lines[nxt]):
                continue
            found.append((i + 1, line.strip()))
    return found


def read_baseline() -> dict[str, int]:
    counts: dict[str, int] = {}
    if not BASELINE.exists():
        return counts
    for raw in BASELINE.read_text().splitlines():
        if not raw.strip() or raw.startswith("#"):
            continue
        rel, _, n = raw.partition("\t")
        counts[rel] = int(n)
    return counts


def write_baseline(counts: dict[str, int]) -> None:
    head = (
        "# Unchecked `< <(...)` sites per file (issue 2424,\n"
        "# scripts/check-process-substitution-status.py). May only FALL: fix a\n"
        "# site with `wait $!` as its next statement, then `--update`.\n"
    )
    body = "".join(f"{rel}\t{n}\n" for rel, n in sorted(counts.items()) if n)
    BASELINE.write_text(head + body)


def self_test() -> list[str]:
    cases = [
        ("done < <(python3 m.py rows)\n", 1, "unchecked loop"),
        ("mapfile -t a < <(producer)\nwait $!\n", 0, "wait on the next line"),
        ("mapfile -t a < <(producer)\n\n# why\nwait $! || exit 1\n", 0,
         "wait after a blank line and a comment"),
        ("mapfile -t a < <(producer); wait $!\n", 0, "wait on the same line"),
        ("mapfile -t a < <(printf '%s\\n' a b)\n", 0, "printf head is exempt"),
        ("done < <(python3 m.py \\\n  --flag \"a)b\")\nwait $!\n", 0,
         "multi-line producer with a quoted paren"),
        ("done < <(python3 m.py \\\n  --flag)\necho next\n", 1,
         "multi-line producer, unchecked"),
        ("diff <(a) <(b)\n", 0, "argument process substitution is out of scope"),
        ("# done < <(producer)\n", 0, "a comment"),
        ("done < <(p)\nwaiting=1\n", 1, "a word starting with wait is not wait"),
    ]
    problems = []
    for text, want, what in cases:
        got = len(scan_text(text))
        if got != want:
            problems.append(f"{what}: expected {want} site(s), found {got}")
    return problems


def main() -> int:
    problems = self_test()
    if problems:
        print("check-process-substitution-status: SELF-TEST FAILED — the detector "
              "no longer recognises the shape it exists to find:", file=sys.stderr)
        for p in problems:
            print(f"  {p}", file=sys.stderr)
        return 1

    me = Path(__file__).resolve()
    sites: dict[str, list[tuple[int, str]]] = {}
    for rel in tracked_shell():
        path = ROOT / rel
        if path.resolve() == me:
            continue
        try:
            text = path.read_text(encoding="utf-8", errors="replace")
        except OSError:
            continue
        found = scan_text(text)
        if found:
            sites[rel] = found
    counts = {rel: len(v) for rel, v in sites.items()}
    total = sum(counts.values())

    if "--update" in sys.argv[1:]:
        old = read_baseline()
        grown = {r: n for r, n in counts.items() if n > old.get(r, 0)}
        if grown and "--allow-growth" not in sys.argv[1:]:
            print("check-process-substitution-status: --update refuses to RAISE the "
                  "baseline; these files gained unchecked sites:", file=sys.stderr)
            for r, n in sorted(grown.items()):
                print(f"  {r}: {old.get(r, 0)} -> {n}", file=sys.stderr)
            return 1
        write_baseline(counts)
        print(f"check-process-substitution-status: baseline written, {total} site(s)")
        return 0

    base = read_baseline()
    grew, fell = [], []
    for rel in sorted(set(base) | set(counts)):
        have, allowed = counts.get(rel, 0), base.get(rel, 0)
        if have > allowed:
            grew.append((rel, allowed, have))
        elif have < allowed:
            fell.append((rel, allowed, have))

    if grew:
        print("check-process-substitution-status: FAILED (issue 2424) — a process "
              "substitution feeding a redirect drops its producer's exit status, so "
              "a crashing producer reads as an empty stream.", file=sys.stderr)
        for rel, allowed, have in grew:
            print(f"  {rel}: {have} unchecked site(s), baseline {allowed}", file=sys.stderr)
            for lineno, text in sites.get(rel, []):
                print(f"    {rel}:{lineno}  {text}", file=sys.stderr)
        print("\n  Fix: make `wait $!` the NEXT statement, which returns the "
              "producer's status (bash >= 4.4):\n"
              "      mapfile -t rows < <(producer)\n"
              "      wait $! || { echo \"producer failed\" >&2; exit 1; }",
              file=sys.stderr)
        return 1
    if fell:
        print("check-process-substitution-status: the baseline is higher than the "
              "tree — the ratchet only falls, so lower it:\n"
              "      python3 scripts/check-process-substitution-status.py --update",
              file=sys.stderr)
        for rel, allowed, have in fell:
            print(f"  {rel}: baseline {allowed}, now {have}", file=sys.stderr)
        return 1
    print(f"check-process-substitution-status: OK ({total} unchecked site(s) in "
          f"{len(counts)} file(s), none above the baseline)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
