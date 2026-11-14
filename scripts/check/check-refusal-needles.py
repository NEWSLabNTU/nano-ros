#!/usr/bin/env python3
"""Every needle `check cpp` greps a refusal probe's output for is in a refusal TEXT.

WHY THIS EXISTS (phase-489 W9). `just check cpp` compiles freestanding probes
that must FAIL, then greps the compiler output for two needles: the shared
marker `REFUSED by nano-ros` and a phrase of that refusal. On 2026-11-14 a
commit took the design ids out of the refusal texts (ruling A) while three of
those needles were still `issue 2015` / `issue 2037` / `issue 2024`. Every
probe still refused correctly, every needle missed, and `check::build` (with
it `just ci gate`) was red on main for about three hours, because that lane is
~9 min and runs on no pull request. Comments in the same headers still said
`issue 2015`, so a grep of the HEADER would have passed; the needle has to be
in the refusal STRING.

This is the buildless half of that check: no compile, well under a second, on
the fast tier, so a pull request sees it. It reads

  * the needles: every `for _needle in "..." ...` loop in `just/` whose list
    names the marker `REFUSED by nano-ros`;
  * the refusal texts: the string literals of every
    `#define NROS_RCLCPP_REFUSE_*` macro in the nros-cpp headers, each joined
    the way the compiler concatenates them,

and fails if a needle is in no refusal text. It cannot say WHICH refusal a
probe must hit; the compile in `check cpp` still decides that.

Usage: check-refusal-needles.py [--self-test]
"""
from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
MARKER = "REFUSED by nano-ros"
LOOP = re.compile(r"for\s+_needle\s+in\s+((?:\"[^\"]*\"\s*)+);")
STR = re.compile(r'"((?:[^"\\]|\\.)*)"')
DEFINE = re.compile(r"^#define\s+(NROS_RCLCPP_REFUSE_\w+)\b(.*)$")


def needles(just_texts: dict[str, str]) -> list[tuple[str, str]]:
    out = []
    for rel, text in just_texts.items():
        for m in LOOP.finditer(text):
            items = re.findall(r'"([^"]*)"', m.group(1))
            if MARKER not in items:
                continue
            line = text.count("\n", 0, m.start()) + 1
            out += [(f"{rel}:{line}", n) for n in items if n != MARKER]
    return out


def refusal_texts(header_texts: dict[str, str]) -> dict[str, str]:
    """macro name -> its joined string literal."""
    out: dict[str, str] = {}
    for text in header_texts.values():
        lines = text.splitlines()
        i = 0
        while i < len(lines):
            m = DEFINE.match(lines[i])
            if not m:
                i += 1
                continue
            body = [m.group(2)]
            while lines[i].rstrip().endswith("\\") and i + 1 < len(lines):
                i += 1
                body.append(lines[i])
            out[m.group(1)] = "".join(STR.findall(" ".join(body)))
            i += 1
    return out


def check(just_texts, header_texts) -> list[str]:
    texts = refusal_texts(header_texts)
    if not texts:
        return ["found no NROS_RCLCPP_REFUSE_* macro: the harvest is broken, not the tree"]
    found = needles(just_texts)
    if not found:
        return ["found no needle loop naming the marker: the harvest is broken, not the tree"]
    problems = []
    for where, n in found:
        if not any(n in t for t in texts.values()):
            problems.append(f"{where}: needle {n!r} is in no NROS_RCLCPP_REFUSE_* text, "
                            "so every probe that greps for it fails however correctly it refuses")
    return problems


def _tracked(*pathspec: str) -> dict[str, str]:
    files = subprocess.run(["git", "ls-files", "--", *pathspec], cwd=REPO, check=True,
                           capture_output=True, text=True).stdout.split()
    return {f: (REPO / f).read_text(encoding="utf-8", errors="replace") for f in files}


def self_test() -> int:
    header = {"h.hpp": '// issue 2015 in a comment\n#define NROS_RCLCPP_REFUSE_X \\\n'
                       '    "goal-handle shapes are REFUSED by nano-ros. " \\\n    "Use the callbacks."\n'}
    cases = [
        ("a phrase of the text", 'for _needle in "REFUSED by nano-ros" "goal-handle shapes"; do', 0),
        ("an id only a comment has (the 2026-11-14 shape)", 'for _needle in "REFUSED by nano-ros" "issue 2015"; do', 1),
        ("a phrase split across two literals", 'for _needle in "REFUSED by nano-ros" "nano-ros. Use the"; do', 0),
        ("a loop without the marker is not a refusal loop", 'for _needle in "issue 2015"; do', None),
    ]
    fail = 0
    for name, loop, want in cases:
        got = check({"x.just": loop + "\n"}, header)
        if want is None:
            ok = len(got) == 1 and "no needle loop" in got[0]
        else:
            ok = len(got) == want
        fail |= not ok
        print(f"  {'ok  ' if ok else 'FAIL'} {name}: {got or 'clean'}")
    if fail:
        print("check-refusal-needles self-test: FAILED", file=sys.stderr)
        return 1
    print(f"check-refusal-needles self-test: OK ({len(cases)} cases)")
    return 0


def main(argv: list[str]) -> int:
    if argv[:1] == ["--self-test"]:
        return self_test()
    if self_test() != 0:
        return 1
    just_texts = _tracked("just/*.just", "just/**/*.just", "justfile")
    headers = _tracked("packages/api/nros-cpp/include/**")
    problems = check(just_texts, headers)
    if problems:
        print("check-refusal-needles: FAILED", file=sys.stderr)
        for p in problems:
            print(f"  {p}", file=sys.stderr)
        return 1
    print(f"check-refusal-needles: OK ({len(needles(just_texts))} needle(s) against "
          f"{len(refusal_texts(headers))} refusal text(s))")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
