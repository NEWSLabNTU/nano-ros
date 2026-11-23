#!/usr/bin/env bash
# Issue 2893: a lane reaches `scripts/build/fixture-lane.sh` in two spellings,
# `tier2` and `lane=tier2`, because `just build-test-fixtures lane=tier2`
# passes the second verbatim (just's `name=value` sets variables, not recipe
# parameters). A helper that validated the raw value refused the second, and
# when `_require-owned-provisioned-roots` started forwarding `{{lane}}` to
# `nros_lane_modules`, every scheduled tier 2 run stopped at
# `fixture-lane: unknown lane 'lane=tier2'` before building anything.
#
# Two checks:
#   1. structural: every function in fixture-lane.sh that validates a lane
#      (`nros_lane_validate "$lane"`) first set it through `nros_lane_arg`;
#   2. the recipe the scheduled run died in, with both spellings, on the
#      `linux` lane (it resolves its modules without cargo, so the check is
#      buildless). Negative control: a copy of the helper with the normaliser
#      removed refuses the `lane=` spelling, so check 2 can fail.
set -uo pipefail
cd "$(dirname "$0")/.."
fail=0
ok() { echo "  ok   $*"; }
bad() { echo "  FAIL $*"; fail=1; }

lib=scripts/build/fixture-lane.sh

# 1. Every validating function normalises first.
python3 - "$lib" <<'PY' || fail=1
import re, sys
src = open(sys.argv[1]).read()
bad = []
for m in re.finditer(r'^(nros_[a-z_]+)\(\) \{\n(.*?)^\}', src, re.S | re.M):
    name, body = m.group(1), m.group(2)
    if name == "nros_lane_validate" or 'nros_lane_validate "$lane"' not in body:
        continue
    head = body.split('nros_lane_validate "$lane"', 1)[0]
    if 'nros_lane_arg' not in head:
        bad.append(name)
if bad:
    print("  FAIL these validate a lane without normalising it through nros_lane_arg: "
          + ", ".join(bad))
    sys.exit(1)
print("  ok   every lane-validating helper normalises through nros_lane_arg")
PY

# 2. The helper and the recipe take both spellings.
for spelling in linux lane=linux; do
    out="$(bash -c 'source scripts/build/fixture-lane.sh; nros_lane_modules "$1"' _ "$spelling" 2>&1)"
    [ "$out" = linux ] && ok "nros_lane_modules $spelling -> linux" \
        || bad "nros_lane_modules $spelling: '$out'"
    rc=0
    out="$(NROS_QUIET_ACTIVATE=1 just _require-owned-provisioned-roots "$spelling" 2>&1)" || rc=$?
    case "$out" in
        *"lane linux builds no Zephyr fixture"*)
            [ "$rc" -eq 0 ] && ok "_require-owned-provisioned-roots $spelling skips the Zephyr probes" \
                || bad "_require-owned-provisioned-roots $spelling: rc $rc" ;;
        *) bad "_require-owned-provisioned-roots $spelling (rc $rc): $out" ;;
    esac
done

# Negative control: without the normaliser the `lane=` spelling is refused.
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
python3 - "$lib" "$tmp/fixture-lane.sh" <<'PY'
import sys
s = open(sys.argv[1]).read()
s = s.replace('nros_lane_modules() {\n    local lane\n    lane="$(nros_lane_arg "$1")"\n',
              'nros_lane_modules() {\n    local lane="$1"\n', 1)
open(sys.argv[2], "w").write(s)
PY
rc=0
out="$(bash -c 'source "$1"; nros_lane_modules lane=linux' _ "$tmp/fixture-lane.sh" 2>&1)" || rc=$?
case "$out" in
    *"unknown lane 'lane=linux'"*) ok "negative control: an unnormalised helper refuses lane=linux (rc $rc)" ;;
    *) bad "negative control did not reproduce the refusal (rc $rc): $out" ;;
esac

if [ "$fail" -ne 0 ]; then
    echo "lane-arg-spelling: FAIL"
    exit 1
fi
echo "lane-arg-spelling: OK"
