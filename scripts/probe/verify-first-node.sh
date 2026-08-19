# PROBE-OWNED verification (issue #204; rewritten phase-368 W4). Appended
# after the book-extracted steps by run-bootstrap-probe.sh; runs in the same
# shell (env + cwd carry over). The book's Run sections are interactive, so
# they can't execute verbatim — this harness runs the QUICK START's runtime
# non-interactively and asserts its documented output.
#
# What it verifies is first-project.md's whole promise: `nros new --workspace`
# scaffolds a C++ workspace that builds with two cmake commands and publishes
# with NOTHING else running — no router, no daemon, no ROS 2 install. The
# previous version of this file asserted `zenohd` on PATH after `nros setup`,
# which has been false since the router became ROS 2's `rmw_zenohd` (we ship
# none) — the probe was green only while nothing re-ran it, and the first
# re-run under this phase caught it.

echo '=== probe verify: quick-start workspace runtime ==='
repo_root="$(git rev-parse --show-toplevel)"
cd "$repo_root"

# The book's flow happens in a fresh terminal that sources the workspace env;
# the probe is one shell where `nros setup` ran AFTER activate. Re-source =
# "open a new terminal".
source ./activate.sh

command -v nros >/dev/null || { echo "PROBE FAIL: nros not on PATH after bootstrap+activate"; exit 1; }
nros version
command -v cmake >/dev/null || { echo "PROBE FAIL: cmake not present — the host-prereqs block must install it"; exit 1; }

# first-project.md, verbatim in spirit: scaffold outside the checkout, the
# way a user would.
ws=/tmp/probe_quickstart
rm -rf "$ws"
( cd /tmp && nros new probe_quickstart --workspace )
cd "$ws"
cmake -S . -B build "-DNANO_ROS_ROOT=$repo_root"
cmake --build build --parallel

bin=build/src/robot_entry/robot_entry
[ -x "$bin" ] || { echo "PROBE FAIL: entry binary missing at $bin"; exit 1; }

# CycloneDDS: no router to start. The scaffold's talker prints `Published: N`
# every 500 ms; require the first two so we know the timer ticks, not just
# that main() was reached.
timeout 60 "$bin" >/tmp/quickstart.log 2>&1 &
entry_pid=$!
deadline=$((SECONDS + 45))
until grep -q "Published: 1" /tmp/quickstart.log; do
    if ! kill -0 "$entry_pid" 2>/dev/null; then
        echo "PROBE FAIL: entry exited before publishing"
        tail -50 /tmp/quickstart.log
        exit 1
    fi
    if ((SECONDS >= deadline)); then
        echo "PROBE FAIL: no 'Published: 1' within 45 s (the book shows it at ~1 s)"
        tail -50 /tmp/quickstart.log
        exit 1
    fi
    sleep 2
done
kill "$entry_pid" 2>/dev/null || true

echo "PROBE PASS: scaffolded C++ workspace published on a pristine host, no router running"

# The Rust leg of the same promise: scaffold, sync, run. Exercises the path
# the C++ leg cannot — `nros sync` + the cargo build of a cyclone workspace
# on a host where the zenoh-pico source was never provisioned (the cargo
# path does not self-provision submodules; a cyclone graph must not need
# them).
echo '=== probe verify: quick-start Rust workspace runtime ==='
ws_rs=/tmp/probe_quickstart_rs
rm -rf "$ws_rs"
( cd /tmp && nros new probe_quickstart_rs --workspace --lang rust )
cd "$ws_rs"
NROS_REPO_DIR="$repo_root" nros sync
cargo build
timeout 60 ./target/debug/robot_entry >/tmp/quickstart_rs.log 2>&1 &
rs_pid=$!
deadline=$((SECONDS + 45))
until grep -q "Publishing: 1" /tmp/quickstart_rs.log; do
    if ! kill -0 "$rs_pid" 2>/dev/null; then
        echo "PROBE FAIL: rust entry exited before publishing"
        tail -50 /tmp/quickstart_rs.log
        exit 1
    fi
    if ((SECONDS >= deadline)); then
        echo "PROBE FAIL: no 'Publishing: 1' within 45 s"
        tail -50 /tmp/quickstart_rs.log
        exit 1
    fi
    sleep 2
done
kill "$rs_pid" 2>/dev/null || true

echo "PROBE PASS: scaffolded Rust workspace published on a pristine host, no router running"
