#!/usr/bin/env bash
# Run a build gate's independent UNITS a few at a time, and report them in order.
#
# phase-489. `check-scaffold-builds.sh` (21 `nros new` variants) and
# `check-template-copy-out.sh` (every template a user copies out) each built
# their units one after another, every unit a cold cargo/cmake build in its own
# scratch tree. On a 48-CPU host that made them `check::build`'s wall clock:
# 1197 s and 905 s of a lane whose other 19 gates were done long before, at
# 5255 and 3044 CPU-seconds. The units share nothing (each owns its scratch
# tree and its cargo root, issue 0616), so nothing but the loop serialised them.
#
# Usage, from a script that has sourced `gate-scratch.sh` and run
# `nros_gate_scratch_init`:
#
#     jobs="$(nros_gate_unit_jobs NROS_SCAFFOLD_JOBS)" || exit 2
#     nros_gate_scratch reports <gate>-reports || exit 3
#     nros_gate_units "$jobs" "$reports" <fn> <unit>...
#     for i in "${!units[@]}"; do
#         rc="$(nros_gate_unit_report "$reports" "$i" <ok-rc>...)"
#         ...
#     done
#
# `<fn> <unit>` runs in a subshell with its stdout and stderr captured to
# `<reports>/<i>.out`, and its status written to `<reports>/<i>.rc`. A unit
# must therefore report only through its output and its status — a global it
# assigns is lost, which is the property that makes it safe to run alongside
# the others. Both callers' units already did.

# The number of units to run at once: `$<var>` if set, else one per 8 CPUs,
# clamped to 1..8. A 4-vCPU hosted runner stays serial, as it was.
nros_gate_unit_jobs() {
    local var="$1" jobs
    jobs="${!var:-}"
    if [ -z "$jobs" ]; then
        jobs=$(( $(nproc 2>/dev/null || echo 4) / 8 ))
        [ "$jobs" -ge 1 ] || jobs=1
        [ "$jobs" -le 8 ] || jobs=8
    fi
    case "$jobs" in
        ''|*[!0-9]*|0)
            echo "$var must be a positive integer, got '$jobs'" >&2
            return 1 ;;
    esac
    printf '%s\n' "$jobs"
}

# Run `<fn> <unit>` for each unit, `<jobs>` at a time; returns when all are done.
# An interrupt stops the units still running instead of orphaning their builds,
# then exits as `nros_gate_scratch_init`'s traps do (its EXIT trap cleans up).
nros_gate_units() {
    local jobs="$1" dir="$2" fn="$3" running=0 i=0 unit
    shift 3
    trap '_nros_gate_units_stop 130' INT
    trap '_nros_gate_units_stop 143' TERM
    for unit in "$@"; do
        if [ "$running" -ge "$jobs" ]; then
            wait -n || true
            running=$((running - 1))
        fi
        ( rc=0; "$fn" "$unit" > "$dir/$i.out" 2>&1 || rc=$?; echo "$rc" > "$dir/$i.rc" ) &
        running=$((running + 1))
        i=$((i + 1))
    done
    wait
    trap 'exit 130' INT
    trap 'exit 143' TERM
}

_nros_gate_units_stop() {
    kill $(jobs -p) 2>/dev/null || true
    wait 2>/dev/null || true
    exit "$1"
}

# Print unit `<i>`'s captured output — to stdout when its status is one of the
# `<ok-rc>` values, to stderr otherwise — and set `NROS_GATE_UNIT_RC` to that
# status. A unit that recorded no status did not finish (killed, or its subshell
# died), which reads as status 1: a failure, never a pass.
nros_gate_unit_report() {
    local dir="$1" i="$2" rc ok
    shift 2
    rc="$(cat "$dir/$i.rc" 2>/dev/null)" || rc=""
    case "$rc" in
        ''|*[!0-9]*) rc=1; echo "  unit $i: FAIL — it did not finish" >&2 ;;
    esac
    NROS_GATE_UNIT_RC="$rc"
    for ok in "$@"; do
        if [ "$rc" = "$ok" ]; then
            cat "$dir/$i.out" 2>/dev/null
            return 0
        fi
    done
    cat "$dir/$i.out" >&2 2>/dev/null || true
    return 0
}

# The control: statuses survive, reports come back in unit order whatever order
# the units finished in, `<jobs>` really run at once, and a unit that never
# records a status is a failure.
_nros_gate_units_self_test() {
    local dir fail=0 t0 t1 out
    dir="$(mktemp -d "${TMPDIR:-/tmp}/nros-gate-units-test.XXXXXX")" || return 2
    _unit() {
        case "$1" in
            slow-ok) sleep 1; echo "said slow-ok" ;;
            fast-red) echo "said fast-red"; return 5 ;;
            skip) echo "said skip"; return 4 ;;
            dies) kill -9 "$BASHPID" ;;
        esac
    }
    t0="$(date +%s%N)"
    # `dies` is SIGKILLed on purpose; the shell's "Killed" notice is noise here.
    { nros_gate_units 4 "$dir" _unit slow-ok fast-red skip dies; } 2>/dev/null
    t1="$(date +%s%N)"
    out="$(for i in 0 1 2 3; do nros_gate_unit_report "$dir" "$i" 0 4 2>&1; printf 'rc=%s\n' "$NROS_GATE_UNIT_RC"; done)"
    local want
    want="$(printf 'said slow-ok\nrc=0\nsaid fast-red\nrc=5\nsaid skip\nrc=4\n  unit 3: FAIL — it did not finish\nrc=1')"
    if [ "$out" != "$want" ]; then
        echo "gate-units self-test FAIL: reports/statuses out of order or wrong:" >&2
        printf '%s\n' "$out" | sed 's/^/    /' >&2
        fail=1
    fi
    # Two 1 s units at 2 jobs take ~1 s, not ~2 s.
    rm -f "$dir"/*
    t0="$(date +%s%N)"
    nros_gate_units 2 "$dir" _unit slow-ok slow-ok
    t1="$(date +%s%N)"
    if [ $(( (t1 - t0) / 1000000 )) -ge 1800 ]; then
        echo "gate-units self-test FAIL: 2 one-second units at 2 jobs took $(( (t1 - t0) / 1000000 )) ms — they ran serially" >&2
        fail=1
    fi
    if NROS_GATE_UNITS_TEST_JOBS=0 nros_gate_unit_jobs NROS_GATE_UNITS_TEST_JOBS >/dev/null 2>&1; then
        echo "gate-units self-test FAIL: a job count of 0 was accepted" >&2
        fail=1
    fi
    rm -rf -- "$dir"
    [ "$fail" -eq 0 ] && echo "gate-units self-test OK: in-order reports, statuses kept, an unfinished unit fails, units overlap."
    return "$fail"
}

if [ "${BASH_SOURCE[0]}" = "$0" ] && [ "${1:-}" = "--self-test" ]; then
    _nros_gate_units_self_test
    exit $?
fi
