#!/usr/bin/env bash
# Is every gate's verdict independent of the order the gates run in?
# (phase-543, the order-independence check.)
#
# The gate harness runs gates concurrently and starts the slowest first, so a
# gate that passes only because another gate ran before it (or fails only
# because one ran beside it) is invisible as long as the order holds. This runs
# one tier of one lane TWICE, with the tests started in opposite name orders
# (`NROS_GATE_ORDER=asc|desc`, priority by name across nextest's whole range),
# and compares each test's verdict between the two runs.
#
#   * a test that passed in one order and failed in the other is the finding:
#     an order dependence, named with both verdicts;
#   * a test that failed in BOTH is a red, reported as one, but it is not an
#     order question;
#   * both runs green and identical: OK.
#
# It costs two runs of the tier, so it is not a gate on any lane. Run it on
# demand, and after a change to how gates share state:
#
#   just check gate-order            # the fast tier of this machine's lane
#   just check gate-order build      # the build tier
#
# The lane is the same one `just check fast` picks (scripts/build/gate-scope.sh).
set -uo pipefail
cd "$(dirname "$0")/../.."
# shellcheck source=scripts/lib/grep-q.sh
. scripts/lib/grep-q.sh

tier="${1:-fast}"
case "$tier" in fast|build) ;; *) echo "check-gate-order: tier is fast or build, not \`$tier\`" >&2; exit 2 ;; esac
lane="$(bash scripts/build/gate-scope.sh)" || exit 2

manifest=packages/testing/nros-gates/Cargo.toml
tgt="$(cargo metadata --no-deps --offline --format-version 1 --manifest-path "$manifest" \
    | python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])')" || exit 2
junit="$tgt/nextest/default/junit.xml"
out="$PWD/build/gates/order"
mkdir -p "$out"

# compare_junit <asc.xml> <desc.xml>: report and exit 1 on any difference.
compare_junit() {
    python3 - "$1" "$2" <<'PY'
import sys
import xml.etree.ElementTree as ET


def verdicts(path):
    out = {}
    for c in ET.parse(path).getroot().iter("testcase"):
        failed = any(x.tag in ("failure", "error") for x in c)
        out[c.get("name", "")] = "fail" if failed else "pass"
    return out


def compare(a, b):
    """(order-dependent, red in both, ran in one order only)."""
    dep = sorted(n for n in a.keys() & b.keys() if a[n] != b[n])
    both = sorted(n for n in a.keys() & b.keys() if a[n] == b[n] == "fail")
    one = sorted(a.keys() ^ b.keys())
    return dep, both, one


a, b = verdicts(sys.argv[1]), verdicts(sys.argv[2])
dep, both, one = compare(a, b)
for n in dep:
    print(f"  ORDER-DEPENDENT {n}: {a[n]} when started in asc order, {b[n]} in desc")
for n in both:
    print(f"  RED IN BOTH     {n} (a red, not an order question)")
for n in one:
    print(f"  RAN IN ONE RUN  {n} (the listing differed between runs)")
if dep or both or one:
    print(f"check-gate-order: FAIL — {len(dep)} order-dependent, {len(both)} red in both, "
          f"{len(one)} listed in one run only; logs and junit under build/gates/order/")
    sys.exit(1)
print(f"check-gate-order OK — {len(a)} test(s), the same verdict in both orders")
PY
}

# Self-test, on the NORMAL path: the comparison reports an order-dependent
# test, a red in both orders and a test only one run listed, and passes two
# identical green runs.
selftest() {
    local d rc got
    d="$(mktemp -d "${TMPDIR:-/tmp}/nros-gate-order-test.XXXXXX")" || return 2
    printf '%s' '<testsuites><testsuite><testcase name="x"/><testcase name="y"><failure/></testcase></testsuite></testsuites>' > "$d/a.xml"
    printf '%s' '<testsuites><testsuite><testcase name="x"><failure/></testcase><testcase name="y"><failure/></testcase><testcase name="w"/></testsuite></testsuites>' > "$d/b.xml"
    printf '%s' '<testsuites><testsuite><testcase name="x"/></testsuite></testsuites>' > "$d/c.xml"
    rc=0; got="$(compare_junit "$d/a.xml" "$d/b.xml")" || rc=$?
    if [ "$rc" -ne 1 ] || ! nros_grep_q "ORDER-DEPENDENT x" <<<"$got" \
            || ! nros_grep_q "RED IN BOTH     y" <<<"$got" || ! nros_grep_q "RAN IN ONE RUN  w" <<<"$got"; then
        echo "check-gate-order SELFTEST FAILED: differing runs were not reported (rc=$rc):" >&2
        printf '%s\n' "$got" >&2; rm -r "$d"; return 1
    fi
    if ! compare_junit "$d/c.xml" "$d/c.xml" >/dev/null; then
        echo "check-gate-order SELFTEST FAILED: two identical green runs were reported" >&2
        rm -r "$d"; return 1
    fi
    rm -r "$d"
}
selftest || exit 2

declare -A rc
for order in asc desc; do
    echo "check-gate-order: lane \`$lane\`, tier \`$tier\`, tests started in $order name order"
    rm -f "$junit"
    r=0
    NROS_GATE_ORDER="$order" bash scripts/build/gate-lane.sh "$lane" -E "test(/^${tier}::/)" \
        > "$out/$order.log" 2>&1 || r=$?
    rc[$order]=$r
    if [ ! -s "$junit" ]; then
        echo "check-gate-order: the $order run (rc=$r) wrote no junit; its log: $out/$order.log" >&2
        tail -20 "$out/$order.log" >&2
        exit 2
    fi
    cp "$junit" "$out/$order.xml"
    grep -E '^\s+Summary \[' "$out/$order.log" | tail -1
done

compare_junit "$out/asc.xml" "$out/desc.xml"
