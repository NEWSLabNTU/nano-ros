#!/usr/bin/env bash
#
# phase-348 W3 — the provider index and the cmake seam that reads it.
#
# THE PROPERTIES
#
#   T1  `nano_ros_load_providers()` returns rows and per-(kind,name) variables,
#       reading the index THROUGH the CLI. cmake never parses the index; a
#       second parser of one file is the two-derivations defect this repo keeps
#       paying for.
#   T2  case-distinct aliases stay distinct. `nuttx` and `NuttX` are two names
#       the board descriptor really declares, so folding them (an upper-cased
#       variable suffix) would let the last one read win silently.
#   T3  every package.xml the index recorded lands in CMAKE_CONFIGURE_DEPENDS —
#       providers AND non-providers, since adding a provision to a non-provider
#       is exactly the edit that must re-configure.
#   T4  `--check-index` catches a provider added AFTER the index was written.
#       This is the case no file watch can cover (a new file is in nobody's
#       watch list) and is issue 0196's shape, so it is handled by
#       rescan-and-compare rather than a cleverer watch.
#   T5  an index built for DIFFERENT roots is rejected, not served. Such an
#       index is wrong rather than stale — answering from it would answer a
#       question nobody asked;
#   T6  a LATER search root overlays an earlier one, and the loser is still
#       NAMED (phase-348 W5 — a silently-losing overlay is the expensive kind);
#   T7  two claimants in ONE root are an error listing both — precedence
#       between roots is defined, precedence within a root is not;
#   T8  an unknown name reports the names that DO exist.
#
# Needs the in-tree CLI (`just setup-cli`); no fixtures, no compiler.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../../.." && pwd)"
NROS="$ROOT/packages/cli/target/release/nros"
MODULE="$ROOT/cmake/NanoRosProviders.cmake"

# shellcheck source=../../../../scripts/build/check-skip.sh
. "$ROOT/scripts/build/check-skip.sh"

# Issue 0726/0732 — every assertion below is `grep -q … || bad T<n>`, i.e. a
# grep that did not RUN is indistinguishable from a provision that is not
# there, and the verdict it produces ("board:Widget missing (case folded
# away?)") is specific enough to send the reader into the index loader. Under
# the parallel gate fan-out a forked grep can fail to start, which is exactly
# how `workspace_order_gate.sh` next door announced a provider had stopped
# being discoverable. `nros_grep_q` exits 2 on rc>=2 instead of returning it.
# shellcheck source=../../../../scripts/lib/grep-q.sh
. "$ROOT/scripts/lib/grep-q.sh"

# The in-tree CLI is a BUILD PRODUCT, absent on the pristine worktree this
# tier documents itself green on (issue 0650's list names it). Skip loudly —
# the lane's closing report refuses to say "passed" over a skipped gate.
#
# STALE counts as unusable, not as a failure. `check-fast` is contractually the
# source-free, CLI-free tier, so this gate does not own the CLI and cannot
# demand one; and asserting against a binary built from other sources is worse
# than not asserting, because the verdict describes a program no longer in the
# tree. Before this, a branch switch turned one restaled stamp into THREE red
# gates whose printed cause was the same single remedy.
# shellcheck source=../../../../scripts/build/cli-usable.sh
. "$ROOT/scripts/build/cli-usable.sh"
nros_cli_usable "$NROS" || {
    nros_check_skip "provider-index" "$nros_cli_unusable_reason"
    exit 0
}
[ -f "$MODULE" ] || {
    echo "FAIL: module not found at $MODULE" >&2
    exit 1
}

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
fail=0

note() { echo "  $*"; }
bad() {
    echo "FAIL[$1]: $2" >&2
    fail=1
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

# --- a standalone workspace, so the test never depends on repo contents ------
WS="$WORK/ws"
mkdir -p "$WS/src/acme_rmw" "$WS/src/plain_node"
cat >"$WS/src/acme_rmw/package.xml" <<'XML'
<?xml version="1.0"?>
<package format="3">
  <name>acme_rmw</name>
  <version>0.0.0</version>
  <export>
    <nano_ros_provides kind="rmw" name="acme"/>
    <nano_ros_provides kind="rmw" name="acme-fast"/>
  </export>
</package>
XML
# A non-provider. It must still be an INDEX INPUT (T3): adding a provision to
# it later is the edit a watch has to notice.
cat >"$WS/src/plain_node/package.xml" <<'XML'
<?xml version="1.0"?>
<package format="3">
  <name>plain_node</name>
  <version>0.0.0</version>
  <depend>std_msgs</depend>
</package>
XML
# Two aliases differing only by case, as the real nuttx board descriptor does.
mkdir -p "$WS/src/case_board"
cat >"$WS/src/case_board/package.xml" <<'XML'
<?xml version="1.0"?>
<package format="3">
  <name>case_board</name>
  <version>0.0.0</version>
  <export>
    <nano_ros_provides kind="board" name="widget"/>
    <nano_ros_provides kind="board" name="Widget"/>
  </export>
</package>
XML

IDX="$WORK/providers.json"
"$NROS" ws providers --workspace "$WS" --nano-ros-root "$WS" --write-index "$IDX" >/dev/null

# --- T1 / T2 / T3: the cmake loader -----------------------------------------
cat >"$WORK/CMakeLists.txt" <<EOF
cmake_minimum_required(VERSION 3.20)
project(provider_index_gate NONE)
# The real spelling is the CACHE var _NANO_ROS_CODEGEN_TOOL, which
# nros_bootstrap_codegen() honours as its first resolution source. Setting
# it here is what keeps the gate from depending on activate.sh having been
# sourced -- and makes it exercise the variable real builds use.
set(_NANO_ROS_CODEGEN_TOOL "$NROS" CACHE FILEPATH "in-tree nros")
include("$MODULE")
nano_ros_load_providers(WORKSPACE "$WS" NANO_ROS_ROOT "$WS" INDEX "$IDX" REUSE_INDEX)
list(LENGTH NANO_ROS_PROVIDER_ROWS _n)
message(STATUS "GATE rows=\${_n}")
message(STATUS "GATE kinds=\${NANO_ROS_PROVIDER_KINDS}")
message(STATUS "GATE acme_pkg=\${NANO_ROS_PROVIDER_RMW_acme_PACKAGE}")
message(STATUS "GATE acmefast_dir=\${NANO_ROS_PROVIDER_RMW_acme_fast_DIR}")
message(STATUS "GATE lower=\${NANO_ROS_PROVIDER_BOARD_widget_PACKAGE}")
message(STATUS "GATE upper=\${NANO_ROS_PROVIDER_BOARD_Widget_PACKAGE}")
get_property(_deps DIRECTORY PROPERTY CMAKE_CONFIGURE_DEPENDS)
foreach(_d IN LISTS _deps)
  message(STATUS "GATE dep=\${_d}")
endforeach()
EOF

# nros-cmake-prefix-exempt: project(... NONE) — no compiler, no Rust, no
# Corrosion in this tree at all. It configures a module that only shells the
# CLI and reads variables, so no cargo target-dir topology is decided here.
OUT="$(cd "$WORK" && cmake -S . -B build 2>&1)" || {
    echo "FAIL: cmake configure errored" >&2
    echo "$OUT" >&2
    exit 1
}

nros_grep_q "GATE rows=4" <<<"$OUT" ||
    bad T1 "expected 4 provision rows (2 rmw + 2 board), got: $(grep -o 'GATE rows=[0-9]*' <<<"$OUT")"
nros_grep_q "GATE kinds=board;rmw" <<<"$OUT" ||
    bad T1 "expected kinds 'board;rmw', got: $(grep -o 'GATE kinds=.*' <<<"$OUT")"
nros_grep_q "GATE acme_pkg=acme_rmw" <<<"$OUT" ||
    bad T1 "per-name PACKAGE variable not set for rmw:acme"
nros_grep_q "GATE acmefast_dir=$WS/src/acme_rmw" <<<"$OUT" ||
    bad T1 "hyphenated name 'acme-fast' did not map to a usable variable suffix"

# T2 — both case spellings resolve, and to their OWN entries. If the suffix
# were upper-cased these two would be one variable.
nros_grep_q "GATE lower=case_board" <<<"$OUT" || bad T2 "board:widget missing"
nros_grep_q "GATE upper=case_board" <<<"$OUT" || bad T2 "board:Widget missing (case folded away?)"

# T3 — the non-provider is watched too, and so is the index itself.
nros_grep_q "GATE dep=$WS/src/plain_node/package.xml" <<<"$OUT" ||
    bad T3 "a NON-provider package.xml is not in CMAKE_CONFIGURE_DEPENDS"
nros_grep_q "GATE dep=$IDX" <<<"$OUT" ||
    bad T3 "the index itself is not in CMAKE_CONFIGURE_DEPENDS"

# --- T4: a provider appearing after the index was written --------------------
"$NROS" ws providers --workspace "$WS" --nano-ros-root "$WS" --check-index "$IDX" >/dev/null 2>&1 ||
    bad T4 "a freshly written index reports itself STALE"

mkdir -p "$WS/src/latecomer"
cat >"$WS/src/latecomer/package.xml" <<'XML'
<?xml version="1.0"?>
<package format="3">
  <name>latecomer</name>
  <version>0.0.0</version>
  <export>
    <nano_ros_provides kind="rmw" name="latecomer"/>
  </export>
</package>
XML

if CHECK="$("$NROS" ws providers --workspace "$WS" --nano-ros-root "$WS" \
        --check-index "$IDX" 2>&1)"; then
    bad T4 "adding a new provider did NOT make --check-index fail"
else
    nros_grep_q "latecomer" <<<"$CHECK" ||
        bad T4 "--check-index failed but never named the new provider: $CHECK"
    nros_grep_q -i "STALE" <<<"$CHECK" ||
        bad T4 "--check-index failed without saying the index is stale"
fi

# --- T5: an index for other roots ------------------------------------------
OTHER="$WORK/other_ws"
mkdir -p "$OTHER/src"
if OUT5="$("$NROS" ws providers --workspace "$OTHER" --nano-ros-root "$OTHER" \
        --index "$IDX" 2>&1)"; then
    bad T5 "an index built for different roots was served instead of rejected"
else
    nros_grep_q -i "roots" <<<"$OUT5" ||
        bad T5 "rejected, but the message does not mention the roots: $OUT5"
fi

# --- T6/T7/T8: resolution and shadowing (W5) ---------------------------------
# A later search root OVERLAYS an earlier one — the user's workspace copy wins
# over the nano-ros one. That is the whole point of allowing a workspace
# provider (testing a patched backend), and the loser must stay NAMED: a
# silently-losing overlay is the failure that costs an afternoon.
OVER="$WORK/overlay_ws"
mkprovider "$OVER" patched_backend rmw acme        # same rmw name as $WS's acme_rmw
R6="$("$NROS" ws providers --workspace "$OVER" --nano-ros-root "$WS" \
        --resolve rmw:acme 2>&1)" || {
    bad T6 "resolving a shadowed name failed: $R6"
    R6=""
}
if [ -n "$R6" ]; then
    # Position matters. Both names appear in the output whichever one wins, so
    # a bare `grep patched_backend` passes even with the precedence INVERTED —
    # verified by perturbation, which is how this weakness was found. Check the
    # WINNER line (first) and the shadows line separately.
    R6_WINNER="$(head -1 <<<"$R6")"
    # Issue 0726 in CAPTURE form, and the `-q` helper cannot serve it because
    # the LINE is wanted, not just its presence. `grep … || true` collapses
    # "no shadows line" (rc 1) and "grep did not run" (rc>=2) into the same
    # empty string, which the `[ -n … ]` below reports as "the shadowed
    # provider was not reported" — a specific, wrong claim about the CLI.
    if R6_SHADOW="$(grep 'shadows' <<<"$R6")"; then :; else
        r6_rc=$?
        [ "$r6_rc" -eq 1 ] || {
            echo "FATAL: grep failed (rc=$r6_rc) reading the shadows line." >&2
            echo "       A tool failure, not a finding (issue 0726)." >&2
            exit 2
        }
        R6_SHADOW=""
    fi
    nros_grep_q "patched_backend" <<<"$R6_WINNER" ||
        bad T6 "the OVERLAY did not win; winner line was: $R6_WINNER"
    nros_grep_q "root\[1\]" <<<"$R6_WINNER" ||
        bad T6 "the winner is not from the LATER root: $R6_WINNER"
    [ -n "$R6_SHADOW" ] ||
        bad T6 "the shadowed provider was not reported: $R6"
    nros_grep_q "acme_rmw" <<<"$R6_SHADOW" ||
        bad T6 "the shadows line does not name the loser: $R6_SHADOW"
fi

# T7 — two claimants in ONE root have no precedence to appeal to.
DUP="$WORK/dup_ws"
mkprovider "$DUP" dup_one rmw twice
mkprovider "$DUP" dup_two rmw twice
if R7="$("$NROS" ws providers --workspace "$DUP" --nano-ros-root "$DUP" \
        --resolve rmw:twice 2>&1)"; then
    bad T7 "same-root ambiguity resolved instead of erroring: $R7"
else
    nros_grep_q "dup_one" <<<"$R7" && nros_grep_q "dup_two" <<<"$R7" ||
        bad T7 "ambiguity error does not list both candidates: $R7"
fi

# T8 — an unknown name is usually a typo, so the error carries the list.
if R8="$("$NROS" ws providers --workspace "$WS" --nano-ros-root "$WS" \
        --resolve rmw:acmee 2>&1)"; then
    bad T8 "an unknown provider name resolved"
else
    nros_grep_q "acme" <<<"$R8" ||
        bad T8 "not-found error does not list the available names: $R8"
fi

if [ "$fail" -ne 0 ]; then
    echo >&2
    echo "cmake output was:" >&2
    printf '%s\n' "$OUT" >&2
    exit 1
fi

note "index + cmake seam: rows/vars OK, case-distinct aliases preserved,"
note "non-provider inputs watched, staleness and wrong-roots both rejected."
echo "provider index gate: OK"
