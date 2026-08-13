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
                    # issue 0512 — the literal EXTENDS a constant: it opens with
                    # a marker this module defines and then adds text of its own.
                    #
                    # This is the case the gate was blind to, and the only one
                    # GUARANTEED to fail. `exact` and `prefix` both describe a
                    # literal that still matches something; a literal matching
                    # NOTHING never fires, so the wait burns its whole timeout
                    # and the test blames the fixture. Issue 0489:
                    # `esp32_emulator.rs` waited on `"Waiting for messages..."`
                    # after phase-342 W7 converged the examples onto
                    # `LISTENER_READY_MARKER` — 108 s of a 137.9 s suite, with
                    # this gate reporting `OK (32 baselined, 0 new)` throughout.
                    #
                    # Deliberately NOT "any literal matching no constant": the
                    # suite is full of legitimate ad-hoc patterns (`"crc=ok"`,
                    # `"data:"`, QEMU boot strings) and a gate that fires on
                    # correct code is switched off within a week (0512 says so
                    # outright). Extending a KNOWN marker is the narrow signal —
                    # it means someone hardcoded a readiness banner and pinned
                    # more of it than the constant guarantees.
                    extends = [
                        n
                        for n, v in consts.items()
                        if v != lit and lit.startswith(v) and len(v) >= MIN_PREFIX_LEN
                    ]
                    if not (exact or len(prefix) >= 2 or extends):
                        continue
                    counts[(path, lit)] += 1
                    where[(path, lit)].append(
                        (lineno, sorted(exact or extends or prefix))
                    )
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
