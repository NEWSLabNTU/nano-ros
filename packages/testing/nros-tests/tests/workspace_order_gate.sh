#!/usr/bin/env bash
#
# phase-348 W4 — build order is DERIVED from `<depend>`, not from the order a
# SUBDIRS list happens to be written in.
#
# THE PROPERTY
#
# Every workspace CMakeLists in this tree carries a variant of the comment
# "Node pkgs BEFORE entries so the entry codegen sees their
# nano_ros_node_register metadata". That is a real constraint, and the entry
# packages ALREADY state it — `<exec_depend>talker_pkg</exec_depend>` — so a
# hand-maintained ordering is a second spelling of a fact package.xml holds,
# and one that can silently go wrong.
#
# W4 derives the order and leaves the SET authored: a workspace filters SUBDIRS
# by PLATFORM, and which board is active is a selection no `<depend>` can
# express.
#
#   T1  a consumer is ordered after the package it depends on, even when the
#       request lists it FIRST and it sorts first alphabetically — so passing
#       is not an accident of input order;
#   T2  the acceptance criterion: a workspace whose src/ holds a PROVIDER and a
#       CONSUMER of it orders the provider first with nothing authored;
#   T3  a dependency outside the workspace (std_msgs) is ignored, not treated
#       as a missing package — otherwise every real workspace is rejected;
#   T4  a cycle is a hard error naming every package on it, not a partial
#       order that fails somewhere downstream with no clue;
#   T5  a requested subdir that names no package is an error, not a silently
#       shorter list — that would drop a package from the build;
#   T6  a package BETWEEN two requested ones still orders them, though it is
#       not itself in the requested set (a bringup pkg is passed as SYSTEM,
#       not as a subdir);
#   T7  cmake's ORDER_FROM_DEPENDS calls through and reorders.
#
# Needs the in-tree CLI (`just setup-cli`); no compiler, no fixtures.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../../.." && pwd)"
NROS="$ROOT/packages/cli/target/release/nros"
# shellcheck source=../../../../scripts/build/check-skip.sh
. "$ROOT/scripts/build/check-skip.sh"

# Issue 0732 — this gate is where the class was OBSERVED: it announced "the
# provider stopped being discoverable" from a pipeline that died rather than
# answered. Splitting the pipeline (T2 below) fixed the SIGPIPE half; the other
# half is that `grep -q` still cannot tell rc 1 from rc>=2, so a forked grep
# that fails to start under the gate fan-out reads as a missing cycle report or
# a cmake seam that stopped reordering. `nros_grep_q` exits 2 on rc>=2.
# shellcheck source=../../../../scripts/lib/grep-q.sh
. "$ROOT/scripts/lib/grep-q.sh"

# The in-tree CLI is a BUILD PRODUCT, absent on the pristine worktree this
# tier documents itself green on (issue 0650's list names it). Skip loudly —
# the lane's closing report refuses to say "passed" over a skipped gate.
[ -x "$NROS" ] || {
    nros_check_skip "check-workspace-order" "no in-tree nros at packages/cli/target/release/nros (just setup-cli)"
    exit 0
}

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
fail=0
bad() {
    echo "FAIL[$1]: $2" >&2
    fail=1
}

mkpkg() { # <ws> <name> [deps...]
    local ws="$1" name="$2"
    shift 2
    mkdir -p "$ws/src/$name"
    {
        echo '<?xml version="1.0"?>'
        echo '<package format="3">'
        echo "  <name>$name</name>"
        echo '  <version>0.0.0</version>'
        for d in "$@"; do echo "  <exec_depend>$d</exec_depend>"; done
        echo '</package>'
    } >"$ws/src/$name/package.xml"
}

mkprovider() { # <ws> <name> <kind> <provides>
    mkdir -p "$1/src/$2"
    cat >"$1/src/$2/package.xml" <<XML
<?xml version="1.0"?>
<package format="3">
  <name>$2</name>
  <version>0.0.0</version>
  <export>
    <nano_ros_provides kind="$3" name="$4"/>
  </export>
</package>
XML
}

# --- T1 / T3: an entry after its nodes, external deps ignored ---------------
WS="$WORK/ws"
# `aaa_entry` sorts FIRST by name and by path, and depends on both nodes. If
# order were input- or path-driven it would come out first.
mkpkg "$WS" aaa_entry talker_pkg listener_pkg
mkpkg "$WS" talker_pkg std_msgs
mkpkg "$WS" listener_pkg std_msgs

ORDER="$("$NROS" ws order --workspace "$WS" --lines | cut -f1)" || {
    echo "FAIL[T1]: ws order errored" >&2
    exit 1
}
pos() { printf '%s\n' "$ORDER" | grep -n "^$1\$" | cut -d: -f1; }
[ "$(pos talker_pkg)" -lt "$(pos aaa_entry)" ] ||
    bad T1 "aaa_entry not ordered after talker_pkg: $ORDER"
[ "$(pos listener_pkg)" -lt "$(pos aaa_entry)" ] ||
    bad T1 "aaa_entry not ordered after listener_pkg: $ORDER"
[ "$(printf '%s\n' "$ORDER" | wc -l)" -eq 3 ] ||
    bad T3 "std_msgs (not a workspace package) leaked into the order: $ORDER"

# --- T2: the acceptance criterion -------------------------------------------
WS2="$WORK/ws_provider"
mkprovider "$WS2" my_backend rmw acme
mkpkg "$WS2" app my_backend
A2="$("$NROS" ws order --workspace "$WS2" --lines | cut -f1)"
[ "$A2" = "$(printf 'my_backend\napp')" ] ||
    bad T2 "provider not ordered before its consumer, got: $A2"
# and the provider is still discoverable AS one — ordering did not consume it.
#
# CAPTURE, then test — NOT `"$NROS" … | grep -q`. Issue 0732: `grep -q` exits at
# the first match, which closes the pipe under a producer that is still writing;
# `nros` gets EPIPE, and because Rust ignores SIGPIPE its `println!` PANICS
# ("failed printing to stdout: Broken pipe"). With `set -o pipefail` the pipeline
# is then non-zero EVEN THOUGH GREP MATCHED, so `|| bad T2` fired and this gate
# announced "the provider stopped being discoverable" — a specific, false claim
# about the tree, produced by a child that died rather than answered. It is a
# race, so it failed green->red only under the parallel gate fan-out, which is
# the direction that teaches people to re-run a gate instead of believing it.
#
# `check-archive-lang-items.sh` carries the same lesson from an earlier round
# ("grep's early exit gives `nm` SIGPIPE and the pipeline reports FAILURE on a
# match — which inverted an earlier revision of this gate silently"). That fix
# stayed local as a comment; this is the second occurrence.
#
# Splitting it also separates the two verdicts the pipeline conflated: the CLI
# failing to run at all is now distinct from the provider genuinely being absent.
if ! PROVIDERS="$("$NROS" ws providers --workspace "$WS2" --nano-ros-root "$WS2" --kind rmw 2>&1)"; then
    bad T2 "ws providers errored: $PROVIDERS"
elif ! nros_grep_q "acme" <<<"$PROVIDERS"; then
    bad T2 "the provider stopped being discoverable"
fi

# --- T4: a cycle -------------------------------------------------------------
WS3="$WORK/ws_cycle"
mkpkg "$WS3" a b
mkpkg "$WS3" b c
mkpkg "$WS3" c a
mkpkg "$WS3" unrelated
if CY="$("$NROS" ws order --workspace "$WS3" 2>&1)"; then
    bad T4 "a dependency cycle did not fail"
else
    nros_grep_q -i "cycle" <<<"$CY" || bad T4 "failed without saying 'cycle': $CY"
    for n in a b c; do
        nros_grep_q "\b$n\b" <<<"$CY" || bad T4 "cycle report omits '$n': $CY"
    done
fi

# --- T5: a subdir naming no package ------------------------------------------
if MISS="$("$NROS" ws order --workspace "$WS" --subdir src/talker_pkg \
        --subdir src/not_a_pkg 2>&1)"; then
    bad T5 "a subdir with no package.xml was silently dropped"
else
    nros_grep_q "not_a_pkg" <<<"$MISS" || bad T5 "error does not name the bad subdir: $MISS"
fi

# --- T6: an unrequested package still orders the requested ones --------------
# `mid` is depended on by `late` and depends on `early`, but is NOT requested.
# Filtering it out BEFORE the sort would lose the edge that orders early<late.
WS4="$WORK/ws_between"
mkpkg "$WS4" early
mkpkg "$WS4" mid early
mkpkg "$WS4" late mid
B="$("$NROS" ws order --workspace "$WS4" --subdir src/late --subdir src/early --lines | cut -f1)"
[ "$B" = "$(printf 'early\nlate')" ] ||
    bad T6 "requested set not ordered through an unrequested package, got: $B"

# --- T7: the cmake seam ------------------------------------------------------
cat >"$WORK/CMakeLists.txt" <<EOF
cmake_minimum_required(VERSION 3.20)
project(order_gate NONE)
# The real spelling is the CACHE var _NANO_ROS_CODEGEN_TOOL, which
# nros_bootstrap_codegen() honours as its first resolution source. Setting
# it here is what keeps the gate from depending on activate.sh having been
# sourced -- and makes it exercise the variable real builds use.
set(_NANO_ROS_CODEGEN_TOOL "$NROS" CACHE FILEPATH "in-tree nros")
include("$ROOT/cmake/NanoRosWorkspace.cmake")
# Requested deliberately in the WRONG order.
_nano_ros_order_subdirs("$WS" "src/aaa_entry;src/talker_pkg;src/listener_pkg" _out)
message(STATUS "GATE order=\${_out}")
EOF
# nros-cmake-prefix-exempt: project(... NONE) — no compiler, no Rust, no
# Corrosion; this configures one module that shells the CLI and sets a variable.
CM="$(cd "$WORK" && cmake -S . -B build_order 2>&1)" || {
    echo "FAIL[T7]: cmake configure errored" >&2
    echo "$CM" >&2
    exit 1
}
GOT="$(grep -o 'GATE order=.*' <<<"$CM" || true)"
# The entry MOVED to the end because it declares a dependency; the two node
# packages kept the relative order the caller asked for, because nothing
# declares one between them. Both halves matter — see
# `caller_order_wins_ties_so_undeclared_workspaces_are_untouched`.
nros_grep_q "GATE order=src/talker_pkg;src/listener_pkg;src/aaa_entry" <<<"$CM" ||
    bad T7 "cmake did not reorder as expected; got: $GOT"

if [ "$fail" -ne 0 ]; then
    exit 1
fi

echo "workspace order gate: OK (deps drive order, cycle + bad subdir rejected,"
echo "  external deps ignored, cmake seam reorders)"
