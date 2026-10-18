#!/usr/bin/env python3
"""Every parallel `make` the build runs is under the stall watchdog — issue 1403.

`scripts/build/make-stall-watchdog.py` turns a jobserver make that sits idle
with no child (the 1403 deadlock) into a dump plus `NO VERDICT` (rc 75). It only
does that for the makes it WRAPS, and the sweep that resolved 1403 found four
`make -j` invocations it did not wrap: the top-level `build-test-fixtures`
fan-out, `linux build-examples`' fan-out, and the two NuttX kernel makes (which
run inside fixture builds, where a stall keeps the OUTER make's child alive, so
no outer watchdog can see it). A make added tomorrow is the same gap, so the
rule is checked rather than remembered.

The rule: in `justfile`, `just/**.just` and `scripts/**.sh`, a shell command
that runs `make` with `-j`, `--jobserver-style` or a `make_args` array carries
`make-stall-watchdog.py` earlier in the same logical command (backslash
continuations joined). `echo`/`printf` lines only mention a make.

`--self-test` (also run on every normal invocation) is the negative control: an
unwrapped `make -j` must be reported, a wrapped one must not.
"""

import os
import re
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
WATCHDOG = "make-stall-watchdog.py"

# A make binary as the build spells it: bare `make`/`gmake`, or a variable /
# command substitution that resolves to the pinned one.
MAKE = re.compile(
    r'(?:(?<![\w./$-])g?make(?![\w.-])'
    r'|"\$\{?(?:make_bin|make|pinned_make|MAKE)\}?"'
    r'|"\$\(nros_pinned_make\)")'
)
PARALLEL = re.compile(r'(?:^|\s)(?:-j|--jobserver)|make_args\[@\]')
# Where one shell command ends inside a logical line.
END = re.compile(r'&&|\|\||;|\||\)')


def logical_lines(text):
    """(first line number, joined text) per logical line; comments dropped."""
    out, buf, start = [], "", None
    for n, raw in enumerate(text.splitlines(), 1):
        line = raw.rstrip()
        if not buf and line.lstrip().startswith("#"):
            continue
        if start is None:
            start = n
        if line.endswith("\\"):
            buf += line[:-1] + " "
            continue
        out.append((start, buf + line))
        buf, start = "", None
    if buf:
        out.append((start, buf))
    return out


def scan(text):
    """[(line, command)] for every unwatched parallel make in `text`."""
    bad = []
    for n, line in logical_lines(text):
        body = line.strip()
        if body.startswith(("echo ", "printf ")):
            continue
        for m in MAKE.finditer(line):
            tail = line[m.end():]
            e = END.search(tail)
            seg = tail[: e.start()] if e else tail
            if not PARALLEL.search(seg):
                continue
            if WATCHDOG in line[: m.start()]:
                continue
            bad.append((n, body))
    return bad


def self_test():
    errs = []
    cases = (
        ("make -j4 -f x.mk\n", 1, "a bare make -j"),
        ('( cd d && make olddefconfig && make -j"$(nproc)" )\n', 1, "make -j after &&"),
        ('"$make_bin" "${make_args[@]}" -f "$makefile"\n', 1, "a make_args array"),
        ("python3 scripts/build/make-stall-watchdog.py --label x --diag-dir d -- \\\n"
         "    make -j4 -f x.mk\n", 0, "a wrapped make across a continuation"),
        ('echo "running make -j$n"\n', 0, "an echo that mentions make"),
        ("make olddefconfig\n", 0, "a serial make"),
        ("# make -j4\n", 0, "a comment"),
    )
    for text, want, what in cases:
        got = len(scan(text))
        if got != want:
            errs.append(f"self-test: {what}: {got} finding(s), want {want}")
    return errs


def tracked():
    out = subprocess.run(
        ["git", "ls-files", "-z", "--", "justfile", "just/*.just", "scripts/*.sh"],
        cwd=ROOT, capture_output=True, check=True).stdout.decode()
    return [p for p in out.split("\0") if p]


def main(argv):
    errs = self_test()
    if errs or "--self-test" in argv:
        for e in errs:
            print(e, file=sys.stderr)
        if errs:
            print("check-jobserver-makes-watched: SELF-TEST FAILED — the scan cannot "
                  "be trusted", file=sys.stderr)
            return 1
        print("check-jobserver-makes-watched: self-test OK")
        return 0
    findings = []
    files = tracked()
    for path in files:
        with open(os.path.join(ROOT, path), encoding="utf-8", errors="replace") as fh:
            for n, body in scan(fh.read()):
                findings.append(f"  {path}:{n}: {body[:160]}")
    if findings:
        print("check-jobserver-makes-watched: FAILED (issue 1403) — a parallel make "
              "runs outside scripts/build/make-stall-watchdog.py, so a jobserver stall "
              "there is a silent hang instead of NO VERDICT (rc 75):", file=sys.stderr)
        print("\n".join(findings), file=sys.stderr)
        print("  Wrap it: python3 scripts/build/make-stall-watchdog.py --label <name> "
              "--diag-dir <log dir> -- make -j…", file=sys.stderr)
        return 1
    print(f"check-jobserver-makes-watched: OK — every parallel make in {len(files)} "
          "file(s) runs under the stall watchdog (self-test OK)")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
