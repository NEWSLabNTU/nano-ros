#!/usr/bin/env bash
# Issue 0481 — a readiness grep that names an AMBIGUOUS literal waits out its
# whole timeout in silence.
#
# WHAT THIS CATCHES
#
# `wait_for_output_pattern("Waiting for", …)` looks harmless. `"Waiting for"` is
# a prefix of FOUR different markers in `nros_tests::output`:
#
#     INT32_SINK_READY_MARKER       "Waiting for Int32"
#     WS_C_LISTENER_READY_MARKER    "Waiting for messages"
#     SERVICE_SERVER_READY_MARKER   "Waiting for service requests"
#     ACTION_SERVER_READY_MARKER    "Waiting for action goals"
#
# so it matches whichever of those the process happens to print — and NOT the
# Rust listener, which prints `LISTENER_READY_MARKER` ("Subscriber created for
# topic:") instead. Measured cost of one such mismatch: `rust_cyclone` sat at
# 34.1 s against `cpp_cyclone`'s 5.2 s, being 30 s of timeout plus 2 s of settle
# plus 2 s of actual work.
#
# It is silent for two compounding reasons: callers discard the result (the wait
# is a courtesy), and per issue 0471 `wait_for_output_pattern` returns Ok on
# TIMEOUT whenever the process printed anything at all. So no site learns that
# its marker never appeared. Only wall clock shows it, and only to someone
# already looking.
#
# THE RULE
#
# A literal is flagged when it is a strict PREFIX of two or more `output::`
# constants (>=10 chars, so short coincidences like "Listener" do not trip it),
# or when it EQUALS a constant's value outright. Either way the fix is the same:
# name the constant you mean.
#
# Deliberately NOT "no literals in wait_for_output_pattern": 92 of 185 call
# sites pass a literal and most are waiting for ordinary runtime output that no
# constant defines. A gate that flags those is noise, and noise gets suppressed.
#
# BASELINE
#
# 34 sites predate this gate. They are listed in the baseline file so the gate
# can land green and the backlog can only SHRINK — the same shape as
# `check-leaf-lockfiles`. A baselined site that stops matching must be REMOVED
# from the file, which is what stops the list becoming a permanent exemption.
set -euo pipefail
cd "$(dirname "$0")/.."

BASELINE="scripts/readiness-marker-literal-baseline.txt"
OUTPUT_RS="packages/testing/nros-tests/src/output.rs"
[ -f "$OUTPUT_RS" ] || { echo "check-readiness-marker-literals: $OUTPUT_RS missing" >&2; exit 2; }

python3 - "$OUTPUT_RS" "$BASELINE" <<'PY'
import re, sys, glob, os

output_rs, baseline_path = sys.argv[1], sys.argv[2]

consts = dict(
    re.findall(r'pub const (\w+): &str = "((?:[^"\\]|\\.)*)"', open(output_rs).read())
)

baseline = set()
if os.path.exists(baseline_path):
    for line in open(baseline_path):
        line = line.strip()
        if line and not line.startswith("#"):
            baseline.add(line)

found, seen = [], set()
for path in sorted(glob.glob("packages/testing/nros-tests/tests/*.rs")
                   + glob.glob("packages/testing/nros-tests/src/*.rs")):
    for lineno, line in enumerate(open(path, errors="ignore"), 1):
        for m in re.finditer(r'wait_for_output_pattern\("((?:[^"\\]|\\.)*)"', line):
            lit = m.group(1)
            exact = [n for n, v in consts.items() if v == lit]
            prefix = [n for n, v in consts.items()
                      if v != lit and v.startswith(lit) and len(lit) >= 10]
            if exact:
                why, owners = "duplicates", exact
            elif len(prefix) >= 2:
                why, owners = "is an ambiguous prefix of", sorted(prefix)
            else:
                continue
            key = f"{path}:{lineno}"
            seen.add(key)
            if key not in baseline:
                found.append((key, lit, why, owners))

fail = 0
if found:
    print("ERROR: readiness grep(s) naming a literal instead of an output:: constant:",
          file=sys.stderr)
    for key, lit, why, owners in found:
        print(f'  {key}\n      "{lit}" {why}: {", ".join(owners)}', file=sys.stderr)
    print("", file=sys.stderr)
    print("  A literal that matches several markers matches whichever the process", file=sys.stderr)
    print("  happens to print, and NONE when it prints a different one — the wait", file=sys.stderr)
    print("  then burns its whole timeout and continues (issue 0481). Name the", file=sys.stderr)
    print("  constant for the binary you are actually waiting on.", file=sys.stderr)
    fail = 1

stale = sorted(baseline - seen)
if stale:
    print("ERROR: baselined site(s) no longer match — delete them from", file=sys.stderr)
    print(f"       {baseline_path}. The backlog shrinks, it does not persist.", file=sys.stderr)
    for s in stale:
        print(f"  {s}", file=sys.stderr)
    fail = 1

if fail == 0:
    print(f"readiness marker literals: OK ({len(baseline)} baselined, 0 new)")
sys.exit(fail)
PY
