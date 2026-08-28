#!/usr/bin/env bash
# Issue 0573 — a test that spawns zenohd itself leaks it.
#
# WHAT THIS CATCHES
#
# A `Command` built on the router path helper anywhere except the shared
# fixture. Such a spawn misses THREE guards that only live in
# `fixtures/zenohd_router.rs`:
#
#   1. `set_new_process_group` — arms `setpgid(0,0)` + `PR_SET_PDEATHSIG`.
#      This is the ONLY thing that reaps the router when nextest SIGKILLs the
#      test binary, or when the handle sits in a `static` whose `Drop` Rust
#      never runs. Both applied to the two copies 0573 removed, which is why
#      eleven orphans (oldest 3.8 days) were found on a dev host with no test
#      runner active.
#   2. the issue-0470 port LEASE — a private copy binds port 0 and closes the
#      socket, which hands the same port to a concurrent caller (measured: 87
#      collisions in 2400 allocations across 12 processes).
#   3. `graceful_kill_process_group` — SIGTERM before SIGKILL, so the port does
#      not sit in TIME_WAIT for the next fixture.
#
# THE RULE
#
# `ZenohRouter` is the one spelling. Resolving the BINARY PATH elsewhere stays
# fine (a test may reasonably check whether zenohd exists before skipping);
# what is banned is spawning it.
#
# WHY A GATE AT ALL
#
# The fixture was hardened twice — issue 0470 (port lease) and issue 0388
# (binary resolution) — and both hardenings reached the one call site everyone
# knew about while two private copies kept the original defects. Worse, the
# copies had an `impl Drop` that LOOKED like cleanup and never ran, so reading
# them suggested the opposite of what they did. This is CLAUDE.md's "fix the
# CLASS, not the reported site": one shared helper, and a gate that keeps it
# the only one.
#
# Sweep this gate encodes:
#   git grep -ln 'ros_zenohd_path' -- '*.rs'
#
# phase-362 — the helper is `ros_zenohd_path()` now (the ROS-shipped
# `rmw_zenohd`), not the retired `zenohd_binary_path()`. The RULE is unchanged:
# `ZenohRouter` stays the only spawner, because the three guards this gate
# protects live in that fixture and nowhere else. When the helper was renamed
# this gate's own staleness self-test fired — which is why it is worth having
# a gate that knows when it has stopped watching anything.
set -euo pipefail
cd "$(dirname "$0")/.."

# issue 0726 — both conditionals below are `if ! grep -q`, one asserting the
# fixture still carries its guards and one being this gate's own self-test. A
# grep that failed to start makes the first announce that a live guard is gone
# and the second announce that the detector is broken; neither is true and both
# are specific. `nros_grep_q` exits 2 on a tool failure instead.
# shellcheck source=scripts/lib/grep-q.sh
source scripts/lib/grep-q.sh

FIXTURE="packages/testing/nros-tests/src/fixtures/zenohd_router.rs"

[ -f "$FIXTURE" ] || {
    echo "check-zenohd-spawn-sites: $FIXTURE missing — this gate is stale" >&2
    exit 2
}

fail=0

# Every Rust file naming the zenohd binary helper. `git grep` so untracked
# scratch files and build output never enter the population.
mapfile -t candidates < <(git grep -ln 'ros_zenohd_path' -- '*.rs' || true)

if [ "${#candidates[@]}" -eq 0 ]; then
    echo "check-zenohd-spawn-sites: no file references ros_zenohd_path — stale" >&2
    exit 2
fi

saw_fixture=0
for f in "${candidates[@]}"; do
    if [ "$f" = "$FIXTURE" ]; then
        saw_fixture=1
        continue
    fi
    # `Command::new(<something zenohd>)` is the spawn signature. Match the
    # helper reaching a Command, on one line or via a local binding.
    if grep -nE 'Command::new\(&?[A-Za-z_:]*zenohd' "$f" >/dev/null 2>&1 ||
        grep -nE 'Command::new\(.*ros_zenohd_path' "$f" >/dev/null 2>&1; then
        echo "ERROR: $f spawns zenohd directly — use nros_tests::fixtures::ZenohRouter" >&2
        grep -nE 'Command::new\(&?[A-Za-z_:]*zenohd|Command::new\(.*ros_zenohd_path' "$f" >&2
        fail=1
    fi
done

if [ "$saw_fixture" -eq 0 ]; then
    echo "check-zenohd-spawn-sites: $FIXTURE no longer resolves the binary — stale" >&2
    exit 2
fi

# The guards must actually still be IN the fixture: a gate that only forbids
# copies is worthless if the original loses the property it is protecting.
for guard in set_new_process_group graceful_kill_process_group port_lease; do
    if ! nros_grep_q "$guard" "$FIXTURE"; then
        echo "ERROR: $FIXTURE no longer uses $guard — the guarded property is gone" >&2
        fail=1
    fi
done

# Self-test: the detector must fire on the shape 0573 removed.
probe="$(mktemp -t zenohd-spawn-probe.XXXXXX.rs)"
trap 'rm -f "$probe"' EXIT
cat >"$probe" <<'PROBE'
let zenohd = nros_tests::process::ros_zenohd_path().unwrap();
let child = Command::new(&zenohd)
    .args(["--listen", &endpoint, "--no-multicast-scouting"])
    .spawn();
PROBE
if ! nros_grep_q -E 'Command::new\(&?[A-Za-z_:]*zenohd' "$probe"; then
    echo "check-zenohd-spawn-sites: self-test failed — the detector would not have" \
        "caught the private RouterHandle that issue 0573 removed" >&2
    exit 2
fi

if [ "$fail" -ne 0 ]; then
    echo "" >&2
    echo "zenohd must be started through nros_tests::fixtures::ZenohRouter," >&2
    echo "which arms PR_SET_PDEATHSIG, leases the port (issue 0470) and kills" >&2
    echo "the process group gracefully. See issue 0573." >&2
    exit 1
fi

echo "check-zenohd-spawn-sites: OK (ZenohRouter is the only zenohd spawner)"
