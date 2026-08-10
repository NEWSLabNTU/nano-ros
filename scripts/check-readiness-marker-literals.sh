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
#     LISTENER_WAITING_BANNER    "Waiting for messages"
#     SERVICE_SERVER_READY_MARKER   "Waiting for service requests"
#     ACTION_SERVER_READY_MARKER    "Waiting for action goals"
#
# so it matches whichever of those the process happens to print — and NOT the
# Rust listener, which prints `LISTENER_READY_MARKER` ("Subscriber created for
# topic:"). Measured cost of one mismatch: a pubsub cell at 34.1 s against its
# sibling's 5.2 s, being 30 s of timeout plus 2 s of settle plus 2 s of work.
# Nine such sites were found, ~90 s of silence across the suite.
#
# It is silent because callers discard the result: the wait reads as a courtesy.
# The fix for a NEW call site is not a better literal — it is
# `ManagedProcess::expect_ready(DemoRole::…, lang, timeout)`, which resolves the
# marker from the role and PANICS when it never arrives.
#
# THE RULE
#
# Flag a literal that is a strict PREFIX of two or more `output::` constants
# (>=10 chars, so short coincidences like "Listener" do not trip it), or that
# EQUALS one outright.
#
# Deliberately NOT "no literals in wait_for_output_pattern": 92 of 185 call sites
# pass a literal and most wait for ordinary runtime output no constant defines.
# A gate that flags those is noise, and noise gets suppressed.
#
# BASELINE
#
# Pre-existing sites are baselined so the gate lands green and the backlog can
# only SHRINK — the shape `check-leaf-lockfiles` uses. Keyed
# `file<TAB>literal<TAB>count`, NOT by line number: the first version keyed on
# `file:line` and broke the moment a doc comment was added above four baselined
# sites — four entries went stale and four "new" violations appeared, for code
# that had not changed. A baseline must survive edits elsewhere in its file or it
# manufactures the busywork it exists to prevent.
set -euo pipefail
cd "$(dirname "$0")/.."

BASELINE="scripts/readiness-marker-literal-baseline.txt"
OUTPUT_RS="packages/testing/nros-tests/src/output.rs"
[ -f "$OUTPUT_RS" ] || { echo "check-readiness-marker-literals: $OUTPUT_RS missing" >&2; exit 2; }

exec python3 scripts/lib/readiness_marker_check.py "$OUTPUT_RS" "$BASELINE"
