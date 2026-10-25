#!/usr/bin/env python3
"""A correct build tells its user what happened, not which issue built it.

phase-517 W5. The installed-track book probe's first project -- a correct
hello-world -- printed paragraphs citing issues 0991, 1002, 1033, 1663 and
0740/1783, RFC-0100 D6 and RFC-0033 at cmake's default log level. That prose is
for whoever maintains nano-ros; the user reading it has nothing to look up and
nothing to do. The explanation still exists: `nros_message_detail()`
(`cmake/NanoRosDiagnostics.cmake`) prints it under `nros build -v` or
`cmake --log-level=VERBOSE`.

WHAT IS COUNTED

Text a default-level configure or build shows: the string arguments of
`message(STATUS|NOTICE|WARNING|AUTHOR_WARNING ...)` and a custom command's or
target's `COMMENT "..."`, in every tracked `.cmake` file outside tests and
examples plus the root `CMakeLists.txt`, that cite an issue (`issue 1234`,
`#1234`), an RFC (`RFC-0100`) or a phase (`phase-517`). `FATAL_ERROR` and
`SEND_ERROR` are NOT counted: a build that stops is being diagnosed, and the
issue that explains it is a fair thing to name there.

A RATCHET, keyed per file: the sites that predate this gate are recorded in
`.config/cmake-user-output-refs.json` and may only shrink. A rise fails; a fall
must be recorded in the change that made it (`scripts/lib/ratchet.py`).

Usage::

    check-cmake-user-output-refs.py [--update] [--list] [--selftest]
"""

import argparse
import json
import os
import re
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.join(ROOT, "scripts", "lib"))
from ratchet import fell_instructions, judge  # noqa: E402  phase-472 W9

BASELINE = os.path.join(ROOT, ".config", "cmake-user-output-refs.json")
REF = re.compile(r"\b(issues? \d{4}|RFC-\d{4}|phase-\d{3})|#\d{4}\b")
SHOWN = ("STATUS", "NOTICE", "WARNING", "AUTHOR_WARNING")
STRING = re.compile(r'"((?:[^"\\]|\\.)*)"')


def calls(text, name):
    """(line, argument text) of each `name(...)` call, quote- and comment-aware."""
    for m in re.finditer(r"(?<![A-Za-z0-9_])" + name + r"\s*\(", text, re.I):
        i, depth, quoted = m.end(), 1, False
        while i < len(text) and depth:
            c = text[i]
            if c == "\\":
                i += 2
                continue
            if c == '"':
                quoted = not quoted
            elif not quoted and c == "#":
                nl = text.find("\n", i)
                i = len(text) if nl < 0 else nl
                continue
            elif not quoted and c == "(":
                depth += 1
            elif not quoted and c == ")":
                depth -= 1
            i += 1
        yield text.count("\n", 0, m.start()) + 1, text[m.end():i - 1]


def sites(text):
    """(line, shown text) of each default-level output that cites a reference."""
    out = []
    for line, args in calls(text, "message"):
        words = args.split(None, 1)
        if not words or words[0] not in SHOWN:
            continue
        shown = "".join(STRING.findall(args))
        if REF.search(shown):
            out.append((line, shown))
    for name in ("add_custom_command", "add_custom_target"):
        for line, args in calls(text, name):
            m = re.search(r'\bCOMMENT\s+"((?:[^"\\]|\\.)*)"', args)
            if m and REF.search(m.group(1)):
                out.append((line, m.group(1)))
    return out


def tracked():
    out = subprocess.check_output(
        ["git", "-C", ROOT, "ls-files", "--", "*.cmake", "CMakeLists.txt"], text=True)
    return [p for p in out.split()
            if p == "CMakeLists.txt" or (p.endswith(".cmake")
                                         and not p.startswith(("tests/", "examples/"))
                                         and "/tests/" not in p)]


def measure():
    found = {}
    for path in tracked():
        try:
            with open(os.path.join(ROOT, path), encoding="utf8", errors="replace") as fh:
                hit = sites(fh.read())
        except FileNotFoundError:
            continue
        if hit:
            found[path] = hit
    return found


def selftest(verbose=False):
    good = 'message(STATUS "nros: sizes kept")\nnros_message_detail("why (issue 0991)")\n'
    bad = ('message(STATUS\n  "a (issue 0991)")\n'
           'add_custom_target(x COMMENT "stamp (issues 0740, 1783)" VERBATIM)\n'
           'message(WARNING "see RFC-0100")\n')
    fatal = 'message(FATAL_ERROR "stop (issue 1525)")\n# message(STATUS "issue 1")\n'
    ok = sites(good) == [] and len(sites(bad)) == 3 and sites(fatal) == []
    if verbose:
        print("check-cmake-user-output-refs: selftest " + ("ok" if ok else "FAILED"))
    if not ok:
        print("check-cmake-user-output-refs: selftest FAILED -- the scanner no "
              "longer sees a planted site, so a green run would mean nothing",
              file=sys.stderr)
        sys.exit(1)
    return 0


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--update", action="store_true",
                    help="rewrite the baseline from the tree (only ever downward)")
    ap.add_argument("--list", action="store_true", help="print every counted site")
    ap.add_argument("--selftest", action="store_true")
    args = ap.parse_args()
    if args.selftest:
        return selftest(verbose=True)
    selftest()

    found = measure()
    now = {p: len(v) for p, v in found.items()}
    if args.list:
        for p in sorted(found):
            for line, shown in found[p]:
                print(f"{p}:{line}: {shown[:120]}")
        return 0
    if args.update:
        with open(BASELINE, "w", encoding="utf8") as fh:
            json.dump({
                "_comment": (
                    "Default-level cmake output citing an issue, RFC or phase, "
                    "per file -- see scripts/check-cmake-user-output-refs.py. A "
                    "SHRINKING backlog: move the reference into "
                    "nros_message_detail() and lower the count."
                ),
                "counts": dict(sorted(now.items())),
            }, fh, indent=2)
            fh.write("\n")
        print(f"check-cmake-user-output-refs: baseline written -- {sum(now.values())} site(s).")
        return 0

    try:
        with open(BASELINE, encoding="utf8") as fh:
            base = json.load(fh)["counts"]
    except FileNotFoundError:
        print("check-cmake-user-output-refs: no baseline -- create it with --update.",
              file=sys.stderr)
        return 1

    rose, fell = judge(now, base)
    if rose:
        print("check-cmake-user-output-refs: a correct build would print maintainer "
              "references to its user:\n", file=sys.stderr)
        for m in rose:
            print(f"  {m.key}: {m.was} -> {m.now}", file=sys.stderr)
            for line, shown in found.get(m.key, []):
                print(f"      :{line}  {shown[:100]}", file=sys.stderr)
        print("\n  Say what happened in the STATUS line and move the reasoning, with its\n"
              "  issue or RFC, into nros_message_detail() (cmake/NanoRosDiagnostics.cmake),\n"
              "  which `nros build -v` shows. An error that stops the build may name it.",
              file=sys.stderr)
    if fell:
        for line in fell_instructions(
                fell, ".config/cmake-user-output-refs.json",
                lambda k, n: f'"{k}": {n}' if n else None,
                "python3 scripts/check-cmake-user-output-refs.py --update"):
            print(line, file=sys.stderr)
    if rose or fell:
        return 1
    print(f"check-cmake-user-output-refs: OK -- {sum(now.values())} recorded site(s), none new.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
