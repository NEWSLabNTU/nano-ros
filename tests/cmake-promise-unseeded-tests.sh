#!/bin/bash
# tests/cmake-promise-unseeded-tests.sh -- issue 2989
#
# `_nros_promise_check_unseeded` (cmake/NanoRosPromise.cmake) answers, at the
# end of a configure, for readers that ran before a fragment was composed and
# that no `nros configure` seeded. Two arms:
#
#   derived      the composed answer is DERIVED, so the readers' defaults and
#                the late consumers disagree: a FATAL that says the defaults
#                "can be smaller than this image needs".
#   not derived  both halves used the configured values and agree: no WARNING
#                (it used to warn with the derived arm's "can be smaller"),
#                only a status line saying the image is sized as configured.
#
# Each arm runs the real function under `cmake -P`.
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
fail=0

run_arm() { # <status> -> sets rc and out
    local status=$1
    local frag="$tmp/frag-$status.cmake"
    printf 'set(NROS_MESSAGE_BOUNDS_STATUS "%s")\n' "$status" >"$frag"
    printf '%s\n' \
        "include(\"$root/cmake/NanoRosPromise.cmake\")" \
        "_nros_promise_props(\"$frag\" _p)" \
        "set_property(GLOBAL PROPERTY \${_p_KIND} absent)" \
        "_nros_promise_check_unseeded(\"$frag\" \"this image's derived message-bound sizes\")" \
        >"$tmp/drive-$status.cmake"
    rc=0
    out="$(cd "$tmp" && cmake -P "$tmp/drive-$status.cmake" 2>&1)" || rc=$?
}

check() { # <what> <condition-result>
    if [ "$2" = 0 ]; then echo "  ok   $1"; else echo "  FAIL $1"; fail=1; fi
}

run_arm derived
check "derived: the configure stops" "$([ "$rc" -ne 0 ]; echo $?)"
check "derived: it says the defaults can be smaller" "$(grep -q 'can be smaller' <<<"$out"; echo $?)"
check "derived: it names the DERIVED answer" "$(grep -q 'DERIVED' <<<"$out"; echo $?)"

run_arm "not derived"
check "not derived: the configure goes on" "$([ "$rc" -eq 0 ]; echo $?)"
check "not derived: no WARNING" "$(! grep -q 'CMake Warning' <<<"$out"; echo $?)"
check "not derived: no claim the defaults can be smaller" "$(! grep -q 'can be smaller' <<<"$out"; echo $?)"
check "not derived: it says the image is sized as configured" "$(grep -q 'sized as configured' <<<"$out"; echo $?)"

if [ "$fail" -ne 0 ]; then
    echo "last output:"; echo "$out"
    exit 1
fi
echo "cmake-promise-unseeded: OK"
