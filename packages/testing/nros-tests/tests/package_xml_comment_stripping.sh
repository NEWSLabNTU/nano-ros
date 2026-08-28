#!/usr/bin/env bash
#
# issue 0516 / phase-348 W1 — a COMMENTED-OUT element in a package.xml is not a
# declaration.
#
# THE PROPERTY
#
# cmake has no XML parser, so every package.xml reader here matches regexes
# against raw file text. A regex cannot tell an element from the same element
# quoted inside a comment, so before this fix a package.xml that merely
# DOCUMENTED a tag declared it:
#
#   <!-- Provision, NOT consumption. `<nano_ros rmw="zenoh"/>` in a leaf … -->
#
# made `nano_ros_read_package_export()` report the file as consuming zenoh.
# Seven readers had the same shape; the `<depend>`-presence ones are the
# likeliest to fire in the wild, since commenting a dependency in and out is
# routine ROS practice.
#
# The fix is one shared helper, `nros_read_package_xml_body()`, and this gate
# covers the three ways it can go wrong:
#
#   T1  a commented-out declaration must NOT be seen;
#   T2  a real declaration must still be seen (a strip that ate everything
#       would pass T1 for the wrong reason — that is the failure mode this
#       whole file exists to prevent);
#   T3  content BETWEEN two comments must survive. cmake regexes are greedy
#       with no lazy quantifier, so the naive `<!--.*-->` deletes from the
#       first `<!--` to the last `-->`. That is silent content loss, not an
#       error, so it needs its own case.
#
# Buildless: `cmake -P`, no compiler, no cargo, no fixtures.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../../.." && pwd)"
MODULE="$ROOT/cmake/NanoRosPackageXml.cmake"

# Issue 0726 — `expect` below reports "expected to contain '<want>'", i.e. that
# the cmake comment stripper read the wrong value, on the strength of a
# `grep -qF` that may not have run at all. `nros_grep_q` exits 2 on rc>=2.
# shellcheck source=../../../../scripts/lib/grep-q.sh
. "$ROOT/scripts/lib/grep-q.sh"

[ -f "$MODULE" ] || {
    echo "FAIL: module not found at $MODULE" >&2
    exit 1
}

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

# T1 — the declaration is inside a comment.
cat >"$WORK/commented.xml" <<'XML'
<?xml version="1.0"?>
<package format="3">
  <name>commented</name>
  <export>
    <!-- <nano_ros deploy="native" board="native" rmw="zenoh"/> -->
  </export>
</package>
XML

# T2 — a real declaration, alongside a comment that quotes a DIFFERENT value.
# If the strip were to run away, or the regex were to pick the commented value,
# this reports `zenoh` instead of `cyclonedds`.
cat >"$WORK/real.xml" <<'XML'
<?xml version="1.0"?>
<package format="3">
  <name>real</name>
  <export>
    <!-- example only: <nano_ros deploy="native" board="native" rmw="zenoh"/> -->
    <nano_ros deploy="native" board="native" rmw="cyclonedds"/>
  </export>
</package>
XML

# T3 — a real declaration sandwiched BETWEEN two comments.
cat >"$WORK/between.xml" <<'XML'
<?xml version="1.0"?>
<package format="3">
  <name>between</name>
  <export>
    <!-- first comment, with a - dash -->
    <nano_ros deploy="freertos" board="mps2-an385-freertos" rmw="xrce"/>
    <!-- second comment -->
  </export>
</package>
XML

cat >"$WORK/run.cmake" <<CMAKE
include("$MODULE")
foreach(_case commented real between)
    nano_ros_read_package_export(PACKAGE_XML "$WORK/\${_case}.xml")
    message(STATUS "RESULT \${_case} found=\${NANO_ROS_EXPORT_FOUND} rmw=\${NANO_ROS_EXPORT_RMW} board=\${NANO_ROS_EXPORT_BOARD}")
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
    # Issue 0726 in CAPTURE form: `grep … || true` maps BOTH "no RESULT line"
    # (rc 1) and "grep did not run" (rc>=2) to the empty string, and the empty
    # string is reported below as "the case never ran" — a claim about the
    # cmake loop, produced by a grep that never answered. The `-q` helper does
    # not fit (the LINE is wanted), so the statuses are split by hand.
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
    # A herestring, not a pipe: in a pipeline the helper is a SUBSHELL and its
    # `exit 2` would end only that segment.
    if ! nros_grep_q -F -- "$want" <<<"$got"; then
        echo "FAIL[$label]: expected to contain '$want'" >&2
        echo "  got: ${got#*-- }" >&2
        fail=1
    fi
}

# T1: a comment declares nothing.
expect commented "found=FALSE"
# T2: the real element is still read, and with ITS value not the comment's.
expect real "found=TRUE rmw=cyclonedds"
# T3: the greedy-strip failure mode — the element between two comments survives.
expect between "found=TRUE rmw=xrce board=mps2-an385-freertos"

if [ "$fail" -ne 0 ]; then
    echo >&2
    echo "cmake output was:" >&2
    printf '%s\n' "$OUT" >&2
    exit 1
fi

echo "package.xml comment stripping: OK (commented ignored, real read, between-comments preserved)"
