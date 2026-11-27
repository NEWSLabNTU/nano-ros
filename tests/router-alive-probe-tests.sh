#!/usr/bin/env bash
#
# Issue 2973 -- `nros_router_alive_after` says "still running" only for a
# process that is present at the deadline AND exits cleanly on our SIGTERM.
# `timeout`'s 124 said it for a process still dumping core, and
# `--preserve-status` cannot tell a live router (0 on SIGTERM, measured with
# ROS humble's rmw_zenohd) from one that exited 0 on its own. Stand-ins:
#   alive       -- sleeps past the deadline, exits 0 on SIGTERM (a router);
#   alive-143   -- the default SIGTERM death;
#   early-exit  -- exits 0 before the deadline;
#   crashes     -- present at the deadline, aborts on SIGTERM (a dump in
#                  progress reads exactly like this: present, then a signal
#                  status other than TERM's).
# Each must get its verdict, and `timeout`'s 124 must get `crashes` WRONG,
# or this file models nothing (the negative control).
set -uo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck source=scripts/dev/zenohd.sh
. "$repo_root/scripts/dev/zenohd.sh"
# shellcheck source=scripts/lib/deadline.sh
. "$repo_root/scripts/lib/deadline.sh"

failures=0
checks=0
ok()   { echo "  [ok]   $1"; checks=$((checks + 1)); }
fail() { echo "  [FAIL] $1" >&2; failures=$((failures + 1)); checks=$((checks + 1)); }

alive=(bash -c 'trap "exit 0" TERM; sleep 30 & wait')
alive143=(bash -c 'exec sleep 30')
deaf=(bash -c 'trap "" TERM; exec sleep 30')
early=(bash -c 'exit 0')
crashes=(bash -c 'trap "kill -ABRT \$\$" TERM; sleep 30 & wait')

expect() { # <label> <want 0|1> <command>...
    local label="$1" want="$2" rc=0
    shift 2
    nros_router_alive_after 1 "$@" 2>/dev/null || rc=$?
    [ "$rc" -ne 0 ] && rc=1
    if [ "$rc" = "$want" ]; then ok "$label"; else fail "$label: rc=$rc, want $want"; fi
}
expect "a process alive at the deadline that exits 0 on SIGTERM is running" 0 "${alive[@]}"
expect "a process alive at the deadline that dies of SIGTERM is running" 0 "${alive143[@]}"
expect "a process that ignores SIGTERM is killed, not waited on, and is running" 0 "${deaf[@]}"
expect "a process that exited 0 before the deadline is NOT running" 1 "${early[@]}"
expect "a process that aborts when signalled is NOT running" 1 "${crashes[@]}"

# Negative control: the old check, `timeout`'s 124, passes the crashing one.
rc=0
"${NROS_DEADLINE[@]}" 1 "${crashes[@]}" 2>/dev/null || rc=$?
if [ "$rc" -eq 124 ]; then
    ok "control: timeout's 124 reads the crashing process as running"
else
    fail "control: timeout gave rc=$rc for the crashing stand-in, so it models nothing"
fi

echo ""
if [ "$failures" -eq 0 ]; then
    echo "router-alive-probe: $checks check(s) passed"
    exit 0
fi
echo "router-alive-probe: $failures of $checks check(s) FAILED" >&2
exit 1
