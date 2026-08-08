#!/usr/bin/env python3
"""Issue 0481 — flag readiness greps naming an ambiguous literal.

See scripts/check-readiness-marker-literals.sh for the why. This is the engine:
it reads the `output::` constants, scans the test sources, and compares the hit
counts against a baseline keyed `file<TAB>literal<TAB>count`.
"""

import collections
import glob
import os
import re
import sys

CONST_RE = re.compile(r'pub const (\w+): &str = "((?:[^"\\]|\\.)*)"')
CALL_RE = re.compile(r'wait_for_output_pattern\("((?:[^"\\]|\\.)*)"')
MIN_PREFIX_LEN = 10

SCAN = [
    "packages/testing/nros-tests/tests/*.rs",
    "packages/testing/nros-tests/src/*.rs",
]


def load_consts(path):
    return dict(CONST_RE.findall(open(path).read()))


def load_baseline(path):
    out = {}
    if not os.path.exists(path):
        return out
    for line in open(path):
        line = line.rstrip("\n")
        if not line.strip() or line.startswith("#"):
            continue
        f, lit, cnt = line.split("\t")
        out[(f, lit)] = int(cnt)
    return out


def scan(consts):
    """-> (counts per (file, literal), lines per (file, literal))."""
    counts = collections.Counter()
    where = collections.defaultdict(list)
    for pattern in SCAN:
        for path in sorted(glob.glob(pattern)):
            for lineno, line in enumerate(open(path, errors="ignore"), 1):
                for m in CALL_RE.finditer(line):
                    lit = m.group(1)
                    exact = [n for n, v in consts.items() if v == lit]
                    prefix = [
                        n
                        for n, v in consts.items()
                        if v != lit and v.startswith(lit) and len(lit) >= MIN_PREFIX_LEN
                    ]
                    if not (exact or len(prefix) >= 2):
                        continue
                    counts[(path, lit)] += 1
                    where[(path, lit)].append((lineno, sorted(exact or prefix)))
    return counts, where


def main():
    output_rs, baseline_path = sys.argv[1], sys.argv[2]
    consts = load_consts(output_rs)
    baseline = load_baseline(baseline_path)
    counts, where = scan(consts)

    fail = 0
    new = [(k, c) for k, c in counts.items() if c > baseline.get(k, 0)]
    if new:
        print(
            "ERROR: readiness grep(s) naming a literal instead of a role:",
            file=sys.stderr,
        )
        for (path, lit), c in sorted(new):
            allowed = baseline.get((path, lit), 0)
            print(
                f'  {path}: {c} site(s) using "{lit}" (baseline allows {allowed})',
                file=sys.stderr,
            )
            for lineno, owners in where[(path, lit)]:
                print(f"      line {lineno} -> {', '.join(owners)}", file=sys.stderr)
        print("", file=sys.stderr)
        print(
            "  A literal matching several markers matches whichever the process",
            file=sys.stderr,
        )
        print(
            "  happens to print, and NONE when it prints a different one — the",
            file=sys.stderr,
        )
        print(
            "  wait then burns its whole timeout and continues (issue 0481).",
            file=sys.stderr,
        )
        print("", file=sys.stderr)
        print(
            "  Use ManagedProcess::expect_ready(DemoRole::…, lang, timeout): it",
            file=sys.stderr,
        )
        print(
            "  resolves the marker from the ROLE and FAILS when it never arrives.",
            file=sys.stderr,
        )
        fail = 1

    shrunk = [
        (k, baseline[k], counts.get(k, 0)) for k in baseline if counts.get(k, 0) < baseline[k]
    ]
    if shrunk:
        print(
            "ERROR: baseline is stale — these counts dropped, lower them in",
            file=sys.stderr,
        )
        print(f"       {baseline_path}:", file=sys.stderr)
        for (path, lit), was, now in sorted(shrunk):
            print(f'  {path}  "{lit}"  {was} -> {now}', file=sys.stderr)
        print("       The backlog shrinks, it does not persist.", file=sys.stderr)
        fail = 1

    if fail == 0:
        print(
            f"readiness marker literals: OK ({sum(baseline.values())} baselined, 0 new)"
        )
    return fail


if __name__ == "__main__":
    sys.exit(main())
