# shellcheck shell=bash
# PROBE-OWNED verification for the `installed` track (phase-447 A3, RFC-0099
# D1). Appended after the book-extracted steps by run-bootstrap-probe.sh; runs
# in the same shell, so cwd is the workspace first-project.md scaffolded and
# built.
#
# The book steps have already done the thing under test — install a release,
# provision, scaffold, configure, build — and check-installed-sdk-root.sh ran
# right after the install to prove there is no checkout and the SDK root is the
# release's own. What is left is the Run step, whose documented command never
# exits, plus one re-check that nothing since the install planted a nano-ros
# root the build could have resolved instead.

echo '=== probe verify: the installed path, with no checkout ==='
probe_fail() {
    echo "PROBE FAIL: $*" >&2
    exit 1
}

nros_store="$HOME/.nros"
marker_hits="$(find / -xdev \( -path /proc -o -path "$nros_store" \) -prune -o \
    -path '*/packages/core/nros-core/Cargo.toml' -print 2>/dev/null || true)"
[ -z "$marker_hits" ] \
    || probe_fail "a nano-ros root appeared outside the store during setup/build: $marker_hits"

# --- the Run step, non-interactively -------------------------------------------
# first-project.md's Run block, with the output it documents: `Published: N`
# from the talker and `Received: N` from the listener, both in one process.
# Require the SECOND tick of each, so the timer fired more than once and
# delivery is a stream rather than a coincidence.
bin=./build/src/robot_entry/robot_entry
[ -x "$bin" ] || probe_fail "the entry binary first-project.md runs is missing at $PWD/${bin#./}"

log=/tmp/installed-first-project.log
timeout 60 "$bin" >"$log" 2>&1 &
entry_pid=$!
deadline=$((SECONDS + 45))
# `nros_grep_q` comes from scripts/lib/grep-q.sh, which run-bootstrap-probe.sh
# prepends to the post-install check in this same shell. A grep that cannot run
# exits 2 here rather than reading as "not published yet" for 45 s (issue 0726).
until nros_grep_q "Published: 1" "$log" && nros_grep_q "Received: 1" "$log"; do
    if ! kill -0 "$entry_pid" 2>/dev/null; then
        tail -50 "$log"
        probe_fail "the entry exited before publishing and receiving twice"
    fi
    if ((SECONDS >= deadline)); then
        tail -50 "$log"
        probe_fail "no 'Published: 1' + 'Received: 1' within 45 s (the book shows them at ~1 s)"
    fi
    sleep 2
done
kill "$entry_pid" 2>/dev/null || true
head -6 "$log"

echo "PROBE PASS: installed release -> scaffold -> build -> run, with no nano-ros checkout on the host"
