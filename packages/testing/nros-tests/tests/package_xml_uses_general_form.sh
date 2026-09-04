#!/usr/bin/env bash
#
# RFC-0087 D3 / phase-420 W1 — `<nano_ros_uses kind= name=/>`, the general
# consumption form, as the cmake reader sees it.
#
# THE PROPERTY
#
# A consumption export has two spellings and they must mean the same thing:
#
#   <nano_ros deploy="freertos" board="mps2-an385-freertos" rmw="zenoh"/>
#   <nano_ros_uses kind="board" name="mps2-an385-freertos"/>
#
# 91 packages use the sugar, so it is not going away; the general form exists so
# that a NEW provider family — a serializer, phase-421 W4 — costs this reader no
# fourth attribute and costs `cargo-nano-ros`'s parser no new special case. If
# the two spellings ever diverged, that promise would be false and the divergence
# would be invisible: both would still configure, just against different values.
#
# Cases:
#
#   T1  the general form sets NANO_ROS_EXPORT_USES_<KIND> for a family this
#       reader has never heard of;
#   T2  the sugar desugars into the SAME variables (board/rmw), so a consumer
#       cannot tell which spelling was used;
#   T3  `deploy=` does NOT become a selection. It names a `[deploy.*]` block in
#       system.toml, not a provider, and inventing a `deploy` family would give
#       it a descriptor lookup that must always fail;
#   T4  a commented-out selection is not a selection (issue 0516, re-asserted
#       for the new tag because the strip covers the FILE, not a tag list);
#   T5  a selection missing `name=` is a hard error, not a silent skip — the
#       same rule `<nano_ros_provides>` has always had.
#
# Buildless: `cmake -P`, no compiler, no cargo, no fixtures.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../../.." && pwd)"
MODULE="$ROOT/cmake/NanoRosPackageXml.cmake"

# shellcheck source=../../../../scripts/lib/grep-q.sh
. "$ROOT/scripts/lib/grep-q.sh"

[ -f "$MODULE" ] || {
    echo "FAIL: module not found at $MODULE" >&2
    exit 1
}

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

# T1 — a family with no bespoke attribute anywhere in this reader.
cat >"$WORK/general.xml" <<'XML'
<?xml version="1.0"?>
<package format="3">
  <name>general</name>
  <export>
    <nano_ros deploy="freertos"/>
    <nano_ros_uses kind="board" name="mps2-an385-freertos"/>
    <nano_ros_uses kind="rmw" name="zenoh"/>
    <nano_ros_uses kind="serdes" name="flatbuf"/>
  </export>
</package>
XML

# T2 — the same selection, written as the sugar.
cat >"$WORK/sugar.xml" <<'XML'
<?xml version="1.0"?>
<package format="3">
  <name>sugar</name>
  <export>
    <nano_ros deploy="freertos" board="mps2-an385-freertos" rmw="zenoh"/>
  </export>
</package>
XML

# T4 — a documented example beside a real one.
cat >"$WORK/commented.xml" <<'XML'
<?xml version="1.0"?>
<package format="3">
  <name>commented</name>
  <export>
    <!-- example: <nano_ros_uses kind="serdes" name="ghost"/> -->
    <nano_ros_uses kind="serdes" name="real"/>
  </export>
</package>
XML

cat >"$WORK/run.cmake" <<CMAKE
include("$MODULE")
foreach(_case general sugar commented)
    nano_ros_read_package_export(PACKAGE_XML "$WORK/\${_case}.xml")
    message(STATUS "RESULT \${_case} kinds=[\${NANO_ROS_EXPORT_USES_KINDS}] board=\${NANO_ROS_EXPORT_USES_BOARD} rmw=\${NANO_ROS_EXPORT_USES_RMW} serdes=\${NANO_ROS_EXPORT_USES_SERDES} deploy=\${NANO_ROS_EXPORT_DEPLOY} usesdeploy=[\${NANO_ROS_EXPORT_USES_DEPLOY}]")
endforeach()
CMAKE

OUT="$(cmake -P "$WORK/run.cmake" 2>&1)" || {
    echo "FAIL: cmake -P errored" >&2
    echo "$OUT" >&2
    exit 1
}

fail=0
expect() {
    local label="$1" want="$2"
    local got grc
    # Issue 0726 — a `grep … || true` would map "no RESULT line" and "grep did
    # not run" onto the same empty string, and the empty string is reported
    # below as a claim about the cmake loop. Split the statuses by hand.
    if got="$(grep -E "RESULT $label " <<<"$OUT")"; then :; else
        grc=$?
        [ "$grc" -eq 1 ] || {
            echo "FATAL: grep failed (rc=$grc) selecting the RESULT line for" >&2
            echo "       '$label'. A tool failure, not a finding (issue 0726)." >&2
            exit 2
        }
        got=""
    fi
    if [ -z "$got" ]; then
        echo "FAIL[$label]: no RESULT line — the case never ran" >&2
        fail=1
        return
    fi
    if ! nros_grep_q -F -- "$want" <<<"$got"; then
        echo "FAIL[$label]: expected to contain '$want'" >&2
        echo "  got: ${got#*-- }" >&2
        fail=1
    fi
}

# T1 — an unknown family resolves, and the known ones come along.
expect general "serdes=flatbuf"
expect general "board=mps2-an385-freertos"
expect general "rmw=zenoh"

# T2 — the sugar lands in the SAME variables. This is the equivalence.
expect sugar "board=mps2-an385-freertos"
expect sugar "rmw=zenoh"
expect sugar "kinds=[board;rmw]"

# T3 — deploy is read, and is not a selection, in EITHER spelling.
expect general "deploy=freertos"
expect general "usesdeploy=[]"
expect sugar "deploy=freertos"
expect sugar "usesdeploy=[]"

# T4 — the comment declares nothing; the real one survives.
expect commented "serdes=real"

# T5 — a malformed selection must FAIL the configure rather than be skipped.
cat >"$WORK/broken.xml" <<'XML'
<?xml version="1.0"?>
<package format="3">
  <name>broken</name>
  <export>
    <nano_ros_uses kind="serdes"/>
  </export>
</package>
XML
cat >"$WORK/broken.cmake" <<CMAKE
include("$MODULE")
nano_ros_read_package_export(PACKAGE_XML "$WORK/broken.xml")
message(STATUS "RESULT broken REACHED")
CMAKE

if BROKEN_OUT="$(cmake -P "$WORK/broken.cmake" 2>&1)"; then
    echo "FAIL[broken]: a <nano_ros_uses> with no name= configured successfully" >&2
    echo "  got: $BROKEN_OUT" >&2
    fail=1
# cmake wraps a long `message(FATAL_ERROR …)` across lines, so match a fragment
# that survives the wrap rather than the whole sentence.
elif ! nros_grep_q -F -- "needs non-empty kind=" <<<"$BROKEN_OUT"; then
    echo "FAIL[broken]: failed for the wrong reason" >&2
    echo "  got: $BROKEN_OUT" >&2
    fail=1
fi

if [ "$fail" -ne 0 ]; then
    echo "check-package-xml-uses: FAILED" >&2
    exit 1
fi

echo "check-package-xml-uses: OK (general form, sugar equivalence, deploy, comments, malformed)"
