# PROBE-OWNED verification (issue #204). Appended after the book-extracted
# steps by run-bootstrap-probe.sh; runs in the same shell (env + cwd carry
# over from the book steps). The book's Run section is interactive (three
# terminals), so it can't be executed verbatim — instead this harness runs
# the same commands non-interactively and asserts the book's own documented
# readiness signal: `Publishing: 'Hello World: 1'` within the first-node
# chapter's 30-second budget (padded for slow CI hosts).

echo '=== probe verify: first-node-rust runtime ==='
repo_root="$(git rev-parse --show-toplevel)"
cd "$repo_root"

# The book's Run section happens in NEW terminals, each of which sources the
# workspace env afresh (direnv / activate.sh). The probe is one shell, and
# `nros setup` ran AFTER the step-20 `source ./activate.sh`, so tool bins it
# installed (zenohd) aren't wired yet. Re-source = "open terminal 1".
source ./activate.sh

command -v nros >/dev/null || { echo "PROBE FAIL: nros not on PATH after bootstrap+activate"; exit 1; }
nros version
command -v zenohd >/dev/null || { echo "PROBE FAIL: zenohd not on PATH after 'nros setup native'"; exit 1; }

zenohd >/tmp/zenohd.log 2>&1 &
zenohd_pid=$!
trap 'kill "$zenohd_pid" 2>/dev/null || true' EXIT

cd examples/native/rust/talker
RUST_LOG=info timeout 120 cargo run >/tmp/talker.log 2>&1 &
talker_pid=$!

pattern="Publishing: 'Hello World: 1'"
deadline=$((SECONDS + 90))
while ! grep -qF "$pattern" /tmp/talker.log; do
    if ! kill -0 "$talker_pid" 2>/dev/null; then
        echo "PROBE FAIL: talker exited before printing the readiness signal"
        tail -50 /tmp/talker.log
        exit 1
    fi
    if ((SECONDS >= deadline)); then
        echo "PROBE FAIL: no readiness signal within 90 s (book documents ~6 s)"
        tail -50 /tmp/talker.log
        tail -20 /tmp/zenohd.log
        exit 1
    fi
    sleep 2
done
kill "$talker_pid" 2>/dev/null || true

echo "PROBE PASS: talker published '$pattern' on a pristine host"
