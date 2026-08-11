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
#       question nobody asked.
#
# Needs the in-tree CLI (`just setup-cli`); no fixtures, no compiler.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../../.." && pwd)"
NROS="$ROOT/packages/cli/target/release/nros"
MODULE="$ROOT/cmake/NanoRosProviders.cmake"

[ -x "$NROS" ] || {
    echo "FAIL: no in-tree nros at $NROS — run: just setup-cli" >&2
    exit 1
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
set(NANO_ROS_CODEGEN_TOOL "$NROS")
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

grep -q "GATE rows=4" <<<"$OUT" ||
    bad T1 "expected 4 provision rows (2 rmw + 2 board), got: $(grep -o 'GATE rows=[0-9]*' <<<"$OUT")"
grep -q "GATE kinds=board;rmw" <<<"$OUT" ||
    bad T1 "expected kinds 'board;rmw', got: $(grep -o 'GATE kinds=.*' <<<"$OUT")"
grep -q "GATE acme_pkg=acme_rmw" <<<"$OUT" ||
    bad T1 "per-name PACKAGE variable not set for rmw:acme"
grep -q "GATE acmefast_dir=$WS/src/acme_rmw" <<<"$OUT" ||
    bad T1 "hyphenated name 'acme-fast' did not map to a usable variable suffix"

# T2 — both case spellings resolve, and to their OWN entries. If the suffix
# were upper-cased these two would be one variable.
grep -q "GATE lower=case_board" <<<"$OUT" || bad T2 "board:widget missing"
grep -q "GATE upper=case_board" <<<"$OUT" || bad T2 "board:Widget missing (case folded away?)"

# T3 — the non-provider is watched too, and so is the index itself.
grep -q "GATE dep=$WS/src/plain_node/package.xml" <<<"$OUT" ||
    bad T3 "a NON-provider package.xml is not in CMAKE_CONFIGURE_DEPENDS"
grep -q "GATE dep=$IDX" <<<"$OUT" ||
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
    grep -q "latecomer" <<<"$CHECK" ||
        bad T4 "--check-index failed but never named the new provider: $CHECK"
    grep -qi "STALE" <<<"$CHECK" ||
        bad T4 "--check-index failed without saying the index is stale"
fi

# --- T5: an index for other roots ------------------------------------------
OTHER="$WORK/other_ws"
mkdir -p "$OTHER/src"
if OUT5="$("$NROS" ws providers --workspace "$OTHER" --nano-ros-root "$OTHER" \
        --index "$IDX" 2>&1)"; then
    bad T5 "an index built for different roots was served instead of rejected"
else
    grep -qi "roots" <<<"$OUT5" ||
        bad T5 "rejected, but the message does not mention the roots: $OUT5"
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
