#!/bin/bash
# tests/cmake-system-include-tests.sh -- issue 2587, phase-580 W3
#
# THE CMAKE ROAD READS A system.toml WITH ITS `include = [..]` FRAGMENTS.
#
# `nros_system_toml_expanded()` (cmake/NanoRosSystemInclude.cmake) hands a
# CMake line reader the file itself when it names no include, and otherwise
# the expansion `nros dev image system-toml` writes into the build dir, with
# every fragment a configure dependency.
#
#   P. A file with no include: the reader gets the file itself.
#   E. A file with an include: the reader gets the expansion (a key from the
#      fragment is in it), and the fragment is a configure input (ninja's
#      re-run edge lists it).
#   R. A key stated in the file and a fragment: the configure stops with the
#      expander's refusal, naming both files.
#   W. nano_ros_workspace over a bringup whose per-image knob rows sit in a
#      fragment, with no image named: refused, exactly as when the rows are in
#      the file itself. Before W3 it either missed the rows or refused the
#      include.
#
# PRECONDITIONS ARE HARD FAILURES; skipping belongs in the `just` recipe.
#
# Usage: ./tests/cmake-system-include-tests.sh
# Exit:  0 all assertions held; 1 otherwise.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# shellcheck source=lib/common.sh
source "$SCRIPT_DIR/lib/common.sh"
# shellcheck source=../scripts/lib/grep-q.sh
source "$PROJECT_ROOT/scripts/lib/grep-q.sh"

FAILURES=0
fail() {
    log_error "$*"
    FAILURES=$((FAILURES + 1))
}

for _t in cmake ninja; do
    if ! command -v "$_t" >/dev/null 2>&1; then
        fail "$_t is not on PATH -- this test cannot report a verdict without it"
        exit 1
    fi
done

# shellcheck source=scripts/build/cargo.sh
source "$PROJECT_ROOT/scripts/build/cargo.sh"
NROS_BIN="$(nros_cli_bin --checkout-only)" || NROS_BIN=""
if [ -z "$NROS_BIN" ]; then
    fail "in-tree CLI not built -- run \`just setup-cli\`"
    exit 1
fi
export PATH="$(dirname "$NROS_BIN"):$PATH"
export NROS_CLI="$NROS_BIN"
export nano_ros_ROOT="$PROJECT_ROOT"
export NROS_WORKSPACE="$PROJECT_ROOT"
export NROS_REPO_DIR="$PROJECT_ROOT"

init_test_tmpdir "nros-system-include"
trap 'cleanup_test_tmpdir' EXIT

# probe <dir> <system.toml> <log> -> sets RC; the project prints READ=<path>.
probe() {
    local d="$1"
    mkdir -p "$d/p"
    printf '%s\n' 'cmake_minimum_required(VERSION 3.22)' 'project(p NONE)' \
        "include(\"$PROJECT_ROOT/cmake/NanoRosSystemInclude.cmake\")" \
        "nros_system_toml_expanded(\"$2\" _read probe)" \
        'message(STATUS "READ=${_read}")' > "$d/p/CMakeLists.txt"
    RC=0
    cmake -G Ninja -S "$d/p" -B "$d/p/b" >"$3" 2>&1 || RC=$?
}

# ---- P: no include -> the file itself ---------------------------------------
P="$TEST_TMPDIR/p"
mkdir -p "$P/b1"
printf '%s\n' '[system]' 'name = "p"' > "$P/b1/system.toml"
probe "$P" "$P/b1/system.toml" "$TEST_TMPDIR/p.log"
if [ "$RC" -ne 0 ] || ! nros_grep_q "READ=$P/b1/system.toml" "$TEST_TMPDIR/p.log"; then
    fail "P: a file with no include was not read as itself (rc=$RC)"
    tail -10 "$TEST_TMPDIR/p.log" >&2
fi

# ---- E: include -> the expansion, the fragment a configure input -------------
E="$TEST_TMPDIR/e"
mkdir -p "$E/b1" "$E/shared"
printf '%s\n' '[system]' 'rmw = "xrce"' > "$E/shared/rmw.toml"
printf '%s\n' 'include = ["../shared/rmw.toml"]' '[[domain]]' 'id = 1' > "$E/b1/system.toml"
probe "$E" "$E/b1/system.toml" "$TEST_TMPDIR/e.log"
read_path="$(sed -n 's/.*READ=//p' "$TEST_TMPDIR/e.log" | head -1)"
if [ "$RC" -ne 0 ] || [ -z "$read_path" ] || [ "$read_path" = "$E/b1/system.toml" ]; then
    fail "E: a file with an include was not read as its expansion (rc=$RC, read=$read_path)"
    tail -10 "$TEST_TMPDIR/e.log" >&2
elif ! nros_grep_q '^rmw = "xrce"' "$read_path" || nros_grep_q '^include' "$read_path"; then
    fail "E: the expansion lacks the fragment's key, or still names the include:"
    cat "$read_path" >&2
fi
if ! nros_grep_q "$E/shared/rmw.toml" "$E/p/b/build.ninja"; then
    fail "E: the fragment is not a configure input (ninja's re-run edge does not list it)"
fi

# ---- R: a key stated twice -> the expander's refusal ------------------------
R="$TEST_TMPDIR/r"
mkdir -p "$R/b1"
printf '%s\n' '[system]' 'rmw = "xrce"' > "$R/shared.toml"
printf '%s\n' 'include = ["../shared.toml"]' '[system]' 'rmw = "zenoh"' > "$R/b1/system.toml"
probe "$R" "$R/b1/system.toml" "$TEST_TMPDIR/r.log"
if [ "$RC" -eq 0 ] || ! nros_grep_q 'system.rmw. is stated in both' "$TEST_TMPDIR/r.log"; then
    fail "R: a key stated twice did not stop the configure with the expander's refusal (rc=$RC)"
    tail -10 "$TEST_TMPDIR/r.log" >&2
fi

# ---- W: nano_ros_workspace sees image rows stated in a fragment --------------
W="$TEST_TMPDIR/w"
mkdir -p "$W/root" "$W/src/demo_bringup" "$W/shared"
printf '%s\n' '<?xml version="1.0"?>' '<package format="3">' \
    '  <name>demo_bringup</name>' '  <version>0.0.0</version>' \
    '  <description>t</description>' '  <maintainer email="t@t">t</maintainer>' \
    '  <license>Apache-2.0</license>' '</package>' > "$W/src/demo_bringup/package.xml"
printf '%s\n' '[image.posix.knobs.zenoh]' 'max_subscribers = 3' > "$W/shared/knobs.toml"
printf '%s\n' 'include = ["../../shared/knobs.toml"]' '[system]' 'name = "t2587"' \
    'rmw = "zenoh"' 'domain_id = 0' '' '[image.posix]' 'board = "linux"' \
    > "$W/src/demo_bringup/system.toml"
printf '%s\n' 'cmake_minimum_required(VERSION 3.22)' 'project(t2587 LANGUAGES C CXX)' \
    'find_package(nano_ros REQUIRED COMPONENTS workspace)' \
    'nano_ros_workspace(' '    WORKSPACE_ROOT ".."' '    BACKEND zenoh' \
    '    PLATFORM "posix"' '    SYSTEM demo_bringup' '    NO_PLAN' '    SUBDIRS' ')' \
    > "$W/root/CMakeLists.txt"
RC=0
cmake -S "$W/root" -B "$W/root/b" -Dnano_ros_ROOT="$PROJECT_ROOT" >"$TEST_TMPDIR/w.log" 2>&1 || RC=$?
if [ "$RC" -eq 0 ] || ! nros_grep_q 'states per-image' "$TEST_TMPDIR/w.log"; then
    fail "W: image knob rows stated in a fragment were not seen (rc=$RC):"
    tail -15 "$TEST_TMPDIR/w.log" >&2
fi

if [ "$FAILURES" -ne 0 ]; then
    log_error "cmake-system-include: $FAILURES assertion(s) failed"
    exit 1
fi
echo "cmake-system-include: OK -- P (no include, the file), E (the expansion, the fragment a configure input), R (a key twice refused), W (nano_ros_workspace sees a fragment's image rows)"
