#!/usr/bin/env bash
#
# Keep a work claim alive for as long as its agent is — phase-395 W14.
#
# WHY A SUPERVISOR AND NOT A TIMER IN THE AGENT
#
# `reserve-claim.sh` prints this instruction on every successful claim, and
# nothing obeyed it: "run it from a LIVENESS supervisor, not between steps — a
# 40-minute fixture build must not look like death." A claim therefore lapsed
# during exactly the long work it exists to protect, and another agent could
# then legitimately `steal` it. That is WORSE than no claim at all, because the
# steal looks sanctioned: the ref says expired, the tooling says take it, and
# two agents end up on one task each believing they hold it.
#
# An agent cannot renew its own claim reliably for the same reason it cannot
# notice it has hung: the renew has to happen while the agent is BUSY, which is
# precisely when it is not running bookkeeping. So renewal belongs outside the
# agent's control flow, keyed on the one fact that is still observable from
# outside — is the process alive?
#
# WHAT COUNTS AS ALIVE, AND WHAT DELIBERATELY DOES NOT
#
# ALIVE  = the supervised pid exists and is not a zombie.
# NOT    = "made progress recently", "printed something", "the build moved on".
#
# Progress heuristics are what turn a slow build into a false death. A process
# blocked for forty minutes inside one `cargo build` is healthy and has produced
# no observable progress at all. Issue 0853 is the same distinction one layer
# down: a zombie keeps its pid and its pgid and stays in `ps` output, so a
# liveness check that does not exclude `Z` calls a corpse alive — which here
# would renew a dead agent's claim forever, the exact failure this prevents.
#
# Usage:
#   scripts/ci/claim-supervisor.sh <id> [--pid N] [--interval SEC] [--ttl HOURS]
#   scripts/ci/claim-supervisor.sh --selftest
#
# Typical: an agent claims, then backgrounds a supervisor on its own pid.
#
#   just claim issue-0827
#   scripts/ci/claim-supervisor.sh issue-0827 --pid $$ &
#
# It exits when the supervised process does, and RELEASES the claim on a clean
# exit so a finished task does not leave hours of phantom occupancy.

set -uo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/../.."

CLAIM="scripts/reserve-claim.sh"
INTERVAL="${NROS_CLAIM_RENEW_INTERVAL:-600}"   # 10 min, well inside a 4 h lease
TTL_HOURS=""
PID=""
ID=""

# ALIVE = exists AND not a zombie. `stat=` is not optional here: a zombie keeps
# its pid until reaped, so `kill -0` and a bare `ps -p` both report a corpse as
# running — and a supervisor that believes that renews a dead agent's claim
# forever. Same predicate as `scripts/build/subtree-guard.sh`. -> issue 0853
_alive() {
    local pid="$1" st
    st="$(ps -o stat= -p "$pid" 2>/dev/null | tr -d ' ')"
    case "$st" in
        "")   return 1 ;;   # gone
        Z*)   return 1 ;;   # reaped-but-not-collected: dead
        *)    return 0 ;;
    esac
}

if [ "${1:-}" = "--selftest" ]; then
    fails=0
    ok() { printf '  ok    %s\n' "$1"; }
    no() { printf '  FAIL  %s\n' "$1"; fails=$((fails + 1)); }

    _alive $$ && ok "this shell is alive" || no "this shell reported dead"

    # A pid that cannot exist.
    if _alive 999999 2>/dev/null; then no "a nonexistent pid reported alive"
    else ok "a nonexistent pid is not alive"; fi

    # A REAL zombie, staged deliberately: this is the case a naive `kill -0` or
    # a bare `ps -p` gets wrong, and it is the entire reason `_alive` inspects
    # `stat=`. Proving it by construction beats trusting the comment.
    #
    # bash reaps its own background children, so `sleep 0 &` cannot stage one.
    # Python can: fork, let the child exit, and have the parent NOT wait. The
    # child then sits in Z until the parent dies.
    ztmp="$(mktemp)"
    python3 - "$ztmp" <<'PY' &
import os, sys, time
pid = os.fork()
if pid == 0:
    os._exit(0)              # the child: exits immediately, never reaped
open(sys.argv[1], "w").write(str(pid))
time.sleep(10)               # the parent: stays alive, deliberately no wait()
PY
    zparent=$!
    zpid=""
    for _ in $(seq 1 50); do
        zpid="$(cat "$ztmp" 2>/dev/null)"
        [ -n "$zpid" ] && break
        sleep 0.1
    done
    zstat=""
    if [ -n "$zpid" ]; then
        for _ in $(seq 1 50); do
            zstat="$(ps -o stat= -p "$zpid" 2>/dev/null | tr -d ' ')"
            case "$zstat" in Z*) break ;; esac
            sleep 0.1
        done
    fi
    case "$zstat" in
        Z*) if _alive "$zpid"; then no "a ZOMBIE reported alive (the 0853 bug)"
            else ok "a real zombie (state $zstat) is not alive"; fi ;;
        *)  no "could not stage a zombie (state '${zstat}') — the 0853 case is UNPROVEN" ;;
    esac
    kill "$zparent" 2>/dev/null || true
    wait "$zparent" 2>/dev/null || true
    rm -f "$ztmp"

    [ "$fails" -eq 0 ] || { echo "claim-supervisor selftest: FAILED"; exit 1; }
    echo "claim-supervisor selftest: OK"
    exit 0
fi

while [ "$#" -gt 0 ]; do
    case "$1" in
        --pid)      shift; PID="${1:-}" ;;
        --interval) shift; INTERVAL="${1:-}" ;;
        --ttl)      shift; TTL_HOURS="${1:-}" ;;
        -h|--help)  sed -n '2,45p' "$0"; exit 0 ;;
        -*)         echo "unknown option $1" >&2; exit 1 ;;
        *)          [ -z "$ID" ] && ID="$1" || { echo "unexpected arg $1" >&2; exit 1; } ;;
    esac
    shift
done

[ -n "$ID" ] || { echo "[FAIL] no claim id. Try: $0 issue-0827 --pid \$\$" >&2; exit 1; }
PID="${PID:-$PPID}"

if ! _alive "$PID"; then
    echo "[FAIL] pid $PID is not alive — nothing to supervise." >&2
    exit 1
fi

ttl_args=()
[ -n "$TTL_HOURS" ] && ttl_args=(--ttl "$TTL_HOURS")

echo "claim-supervisor: renewing '$ID' every ${INTERVAL}s while pid $PID lives"
echo "  liveness is PROCESS EXISTENCE, not progress: a 40-minute build is not death."

renewals=0
while _alive "$PID"; do
    sleep "$INTERVAL" &
    wait $! 2>/dev/null || true
    # Re-check AFTER the sleep: the process may have exited during it, and
    # renewing then would extend a dead agent's lease by a full interval.
    _alive "$PID" || break
    if bash "$CLAIM" renew "$ID" "${ttl_args[@]}" >/dev/null 2>&1; then
        renewals=$((renewals + 1))
    else
        # A failed renew is NOT fatal. The remote may be briefly unreachable,
        # or the claim may have been stolen — and in the stolen case the agent
        # is still working, so killing its supervisor would only remove the one
        # thing that could take the claim back.
        echo "claim-supervisor: renew of '$ID' FAILED (rc=$?) — will retry" >&2
    fi
done

echo "claim-supervisor: pid $PID is gone after $renewals renewal(s); releasing '$ID'"
# Release on exit so a finished task does not leave hours of phantom occupancy.
# Best-effort: if the agent died mid-task the claim SHOULD lapse rather than be
# released, but we cannot tell those apart from here — and an explicit release
# is the friendlier default, since the leftovers report on `steal` is what
# surfaces a dead agent's remains either way.
bash "$CLAIM" release "$ID" >/dev/null 2>&1 || true
