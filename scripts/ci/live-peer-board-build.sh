#!/usr/bin/env bash
# Build the fixtures a live-peer board job's rows resolve — issue 1758.
#
#   live-peer-board-build.sh run      "<scopes>" "<KEY=value ...>"
#   live-peer-board-build.sh dry-run  "<scopes>" "<KEY=value ...>"
#   live-peer-board-build.sh selftest
#
# The narrowing comes from `check-interop-verdicts.py --narrowing`, one
# `KEY=value` per line (the workflow joins them with spaces). A key named ONCE
# narrows every pass. A key named SEVERAL times is an EXACT key with several
# cells behind it (`NROS_FIXTURE_ID` names one row), and each of its values is
# its own build pass: `just build <scope>` runs once per value, every other
# key exported as well.
#
# Both board jobs used an inline loop that exported each line in turn, so a
# repeated key kept only its LAST value. The producer had papered over that by
# joining every key's values with `|`, which is a union only for a regex. For
# `NROS_FIXTURE_ID` it named a row nobody has, and the board-ci job stopped in
# its build every night once a second FreeRTOS cell was recorded
# (run 39721379635).
#
# Two repeated keys would need a cross product, and nothing says which pairs
# belong together, so that is refused rather than guessed.
set -euo pipefail

# plan <narrowing>: prints one line per build pass, each the space-separated
# assignments that pass exports. Status 2 on a narrowing it cannot plan.
plan() {
    local narrowing="$1" kv key
    local -a order=()
    local -A values=() count=()
    for kv in $narrowing; do
        case "$kv" in
            *=*) ;;
            *) echo "live-peer-board-build: narrowing '$kv' is not KEY=value" >&2; return 2 ;;
        esac
        key="${kv%%=*}"
        if [ -z "${count[$key]:-}" ]; then
            order+=("$key")
            count[$key]=0
            values[$key]=""
        fi
        count[$key]=$((count[$key] + 1))
        values[$key]="${values[$key]} ${kv#*=}"
    done
    local fixed="" repeated=""
    for key in "${order[@]}"; do
        if [ "${count[$key]}" -gt 1 ]; then
            if [ -n "$repeated" ]; then
                echo "live-peer-board-build: both $repeated and $key carry several values;" \
                    "which pairs to build together is not stated, so this refuses" >&2
                return 2
            fi
            repeated="$key"
        else
            fixed="$fixed $key=${values[$key]# }"
        fi
    done
    fixed="${fixed# }"
    if [ -z "$repeated" ]; then
        printf '%s\n' "$fixed"
        return 0
    fi
    local v
    for v in ${values[$repeated]}; do
        printf '%s\n' "${fixed:+$fixed }$repeated=$v"
    done
}

build() {
    local mode="$1" scopes="$2" narrowing="$3" pass kv s
    local passes
    passes="$(plan "$narrowing")" || return 2
    while IFS= read -r pass; do
        echo "=== narrowing: ${pass:-<none>} ==="
        if [ "$mode" = dry-run ]; then
            for s in $scopes; do echo "just build $s"; done
            continue
        fi
        (
            for kv in $pass; do export "${kv?}"; done
            for s in $scopes; do
                echo "=== just build $s ==="
                just build "$s"
            done
        )
    done <<< "$passes"
}

selftest() {
    local fail=0 got
    check() {
        if [ "$2" != "$3" ]; then
            printf 'live-peer-board-build selftest: %s\n  want: %q\n  got:  %q\n' "$1" "$3" "$2" >&2
            fail=1
        fi
    }
    got="$(plan "NROS_FIXTURE_ID=a NROS_FIXTURE_ID=b NROS_FIXTURE_ID=c")"
    check "a repeated exact key is one pass per value" "$got" \
        $'NROS_FIXTURE_ID=a\nNROS_FIXTURE_ID=b\nNROS_FIXTURE_ID=c'
    got="$(plan "NROS_ZEPHYR_FIXTURE_FILTER=x|y")"
    check "a single key (a regex union) is one pass" "$got" "NROS_ZEPHYR_FIXTURE_FILTER=x|y"
    got="$(plan "K=1 NROS_FIXTURE_ID=a NROS_FIXTURE_ID=b")"
    check "a single key rides every pass" "$got" $'K=1 NROS_FIXTURE_ID=a\nK=1 NROS_FIXTURE_ID=b'
    got="$(plan "")"
    check "no narrowing is one unnarrowed pass" "$got" ""
    if plan "A=1 A=2 B=1 B=2" >/dev/null 2>&1; then
        echo "live-peer-board-build selftest: two repeated keys were planned, not refused" >&2
        fail=1
    fi
    got="$(build dry-run "freertos" "NROS_FIXTURE_ID=a NROS_FIXTURE_ID=b" | grep -c '^just build')"
    check "two passes over one scope build twice" "$got" "2"
    # The producer's real output for the ci image's rows must plan.
    local repo_root real
    repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
    real="$(python3 "$repo_root/scripts/check-interop-verdicts.py" \
        --list-passing --runner board-ci --narrowing | tr '\n' ' ')"
    plan "$real" >/dev/null || { echo "live-peer-board-build selftest: the ledger's board-ci narrowing does not plan: $real" >&2; fail=1; }
    [ "$fail" -eq 0 ] || return 1
    echo "live-peer-board-build: OK -- a repeated exact key builds once per value"
}

case "${1:-}" in
    run | dry-run)
        [ "$#" -eq 3 ] || { echo "usage: $0 run|dry-run <scopes> <narrowing>" >&2; exit 2; }
        build "$1" "$2" "$3"
        ;;
    selftest) selftest ;;
    *) echo "usage: $0 run|dry-run <scopes> <narrowing> | selftest" >&2; exit 2 ;;
esac
