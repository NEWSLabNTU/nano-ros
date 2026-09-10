#!/usr/bin/env bash
#
# RFC-0087 D3 / phase-420 W1 — `<nano_ros_uses kind= name=/>`, the general
# consumption form, as the cmake reader sees it.
#
# THE PROPERTY
#
# A provider selection has ONE spelling:
#
#   <nano_ros_uses kind="board" name="mps2-an385-freertos"/>
#
# The general form exists so that a NEW provider family — a serializer,
# phase-421 W4 — costs this reader no bespoke attribute and costs
# `cargo-nano-ros`'s parser no new special case.
#
# Its sugar, `<nano_ros deploy="freertos" board="…" rmw="…"/>`, was a SECOND
# spelling of the board and RMW selections, and phase-445 W3b retired it: a
# single-package leaf states its board and RMW in `system.toml` (RFC-0098
# D3/D5), which `nano_ros_read_leaf_system()` reads into the same
# `NANO_ROS_EXPORT_USES_{BOARD,RMW}` variables. A leftover tuple is REFUSED —
# silently ignoring it would configure a freertos leaf as a host build.
#
# Cases:
#
#   T1  the general form sets NANO_ROS_EXPORT_USES_<KIND> for a family this
#       reader has never heard of;
#   T2  the retired tuple is a hard error naming `system.toml`, not a silent
#       skip (and not a selection);
#   T3  a package.xml with no selection reads as no selection, and `deploy`
#       is never a family;
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
    <nano_ros_uses kind="board" name="mps2-an385-freertos"/>
    <nano_ros_uses kind="rmw" name="zenoh"/>
    <nano_ros_uses kind="serdes" name="flatbuf"/>
  </export>
</package>
XML

# T3 — nothing selected.
cat >"$WORK/plain.xml" <<'XML'
<?xml version="1.0"?>
<package format="3">
  <name>plain</name>
  <export>
    <build_type>nros_cmake</build_type>
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
foreach(_case general plain commented)
    nano_ros_read_package_export(PACKAGE_XML "$WORK/\${_case}.xml")
    message(STATUS "RESULT \${_case} kinds=[\${NANO_ROS_EXPORT_USES_KINDS}] board=\${NANO_ROS_EXPORT_USES_BOARD} rmw=\${NANO_ROS_EXPORT_USES_RMW} serdes=\${NANO_ROS_EXPORT_USES_SERDES} deploy=\${NANO_ROS_EXPORT_DEPLOY} usesdeploy=[\${NANO_ROS_EXPORT_USES_DEPLOY}] found=\${NANO_ROS_EXPORT_FOUND}")
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
expect general "kinds=[board;rmw;serdes]"

# T3 — no selection reads as none; `deploy` is never a family, and nothing in
# a package.xml sets it any more (it is derived from system.toml's board).
expect plain "kinds=[]"
expect plain "deploy= usesdeploy=[] found=FALSE"
expect general "usesdeploy=[]"

# T4 — the comment declares nothing; the real one survives.
expect commented "serdes=real"

# A case that must FAIL the configure, and fail for the reason given.
expect_fatal() {
    local label="$1" xml="$2" want="$3"
    printf '%s\n' "$xml" >"$WORK/$label.xml"
    cat >"$WORK/$label.cmake" <<CMAKE
include("$MODULE")
nano_ros_read_package_export(PACKAGE_XML "$WORK/$label.xml")
message(STATUS "RESULT $label REACHED")
CMAKE
    local got
    if got="$(cmake -P "$WORK/$label.cmake" 2>&1)"; then
        echo "FAIL[$label]: configured successfully; it must not" >&2
        echo "  got: $got" >&2
        fail=1
    # cmake wraps a long `message(FATAL_ERROR …)` across lines, so match a
    # fragment that survives the wrap rather than the whole sentence.
    elif ! nros_grep_q -F -- "$want" <<<"$got"; then
        echo "FAIL[$label]: failed for the wrong reason (wanted '$want')" >&2
        echo "  got: $got" >&2
        fail=1
    fi
}

# T2 — the retired tuple, in each of the shapes the tree used to carry.
expect_fatal tuple \
    '<package format="3"><name>t</name><export><nano_ros deploy="freertos" board="mps2-an385-freertos" rmw="zenoh"/></export></package>' \
    "is retired"
expect_fatal tuple_native \
    '<package format="3"><name>t</name><export><nano_ros deploy="native"/></export></package>' \
    "system.toml"

# T5 — a malformed selection.
expect_fatal broken \
    '<package format="3"><name>broken</name><export><nano_ros_uses kind="serdes"/></export></package>' \
    "needs non-empty kind="

if [ "$fail" -ne 0 ]; then
    echo "check-package-xml-uses: FAILED" >&2
    exit 1
fi

echo "check-package-xml-uses: OK (general form, retired tuple refused, no selection, comments, malformed)"
