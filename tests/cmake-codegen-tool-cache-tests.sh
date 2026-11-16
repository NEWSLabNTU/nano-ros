#!/bin/bash
# tests/cmake-codegen-tool-cache-tests.sh -- phase-478 I24 (GitHub #4067)
#
# A build dir caches `_NANO_ROS_CODEGEN_TOOL` on its first configure, and
# `nros_bootstrap_codegen()` (cmake/NanoRosBootstrapCodegen.cmake) honors a
# cached tool. The island configured a dir with an in-tree CLI, ran a release
# `nros build` into it, and the configure kept running the in-tree binary
# until the RMW dispatch query failed "in-tree nros CLI is STALE".
#
# Two halves, and this gates the CMAKE one:
#
#   * `nros build` passes `-D_NANO_ROS_CODEGEN_TOOL=<its own path>` on every
#     configure -- asserted in Rust (`build_verb_pipeline.rs`,
#     `every_configure_names_the_invoking_nros_as_the_codegen_tool`). Arm B
#     here checks that such a `-D` replaces the cached tool, which is what
#     makes that pass enough.
#   * a bare `cmake` re-configure has no invoker, so the cache is kept; but a
#     cached tool whose `--codegen-version` this tree does not accept is
#     refused AT the bootstrap, naming both sides (arm C, the negative
#     control). Arms A and D are the controls that the refusal does not fire
#     on a usable tool or on a tool that cannot answer.
#
# THE CLI IS STUBBED: the question is what the cmake module does with a
# tool's answer, not what the real CLI answers.
#
# Usage: ./tests/cmake-codegen-tool-cache-tests.sh
# Exit:  0 all assertions held; 1 otherwise.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# shellcheck source=lib/common.sh
source "$SCRIPT_DIR/lib/common.sh"

MODULE="$PROJECT_ROOT/cmake/NanoRosBootstrapCodegen.cmake"
VERSION_RS="$PROJECT_ROOT/packages/core/nros-core/src/codegen_version.rs"

FAILURES=0
CHECKS=0
fail() { log_error "$*"; FAILURES=$((FAILURES + 1)); }
check() { CHECKS=$((CHECKS + 1)); }

for f in "$MODULE" "$VERSION_RS"; do
    [ -f "$f" ] || { fail "missing: $f"; exit 1; }
done
if ! command -v cmake >/dev/null 2>&1; then
    fail "cmake is not on PATH -- this test cannot report a verdict without it"
    exit 1
fi

# The tree's accepted range, read the way the module reads it.
max="$(sed -n 's/^pub const NROS_CODEGEN_VERSION: *u32 *= *\([0-9]*\).*/\1/p' "$VERSION_RS")"
min="$(sed -n 's/^pub const NROS_CODEGEN_VERSION_MIN: *u32 *= *\([0-9]*\).*/\1/p' "$VERSION_RS")"
if [ -z "$max" ] || [ -z "$min" ]; then
    fail "cannot read the accepted range from $VERSION_RS"
    exit 1
fi

init_test_tmpdir "nros-codegen-tool-cache"
trap 'cleanup_test_tmpdir' EXIT
T="$TEST_TMPDIR"

# stub <path> <answer|fail>: a `nros` that answers `--codegen-version`.
stub() {
    if [ "$2" = fail ]; then
        printf '#!/bin/sh\nexit 2\n' >"$1"
    else
        printf '#!/bin/sh\n[ "$1" = --codegen-version ] && echo %s\n' "$2" >"$1"
    fi
    chmod +x "$1"
}

mkdir -p "$T/src"
cat >"$T/src/CMakeLists.txt" <<EOF
cmake_minimum_required(VERSION 3.22)
project(codegen_tool_cache NONE)
include("$MODULE")
nros_bootstrap_codegen()
message(STATUS "TOOL=\${_NANO_ROS_CODEGEN_TOOL}")
EOF

configure() {
    # The ambient environment must not pick the tool: no NROS_CLI.
    env -u NROS_CLI cmake -S "$T/src" -B "$T/build" "$@" >"$T/out" 2>&1
}
cached_tool() {
    sed -n "s/^_NANO_ROS_CODEGEN_TOOL:[A-Z]*=//p" "$T/build/CMakeCache.txt"
}

stub "$T/tool-a" "$max"
stub "$T/tool-b" "$min"

# A. a usable cached tool is kept by a bare re-configure.
check
if configure -D_NANO_ROS_CODEGEN_TOOL="$T/tool-a" && configure \
    && [ "$(cached_tool)" = "$T/tool-a" ]; then
    log_success "A: a bare re-configure keeps a usable cached tool"
else
    fail "A: expected tool-a kept; cache has '$(cached_tool)'"; cat "$T/out"
fi

# B. the invoker's `-D` replaces the cached tool (what `nros build` passes).
check
if configure -D_NANO_ROS_CODEGEN_TOOL="$T/tool-b" && [ "$(cached_tool)" = "$T/tool-b" ] \
    && grep -q "TOOL=$T/tool-b" "$T/out"; then
    log_success "B: a configure naming another tool uses it"
else
    fail "B: expected tool-b; cache has '$(cached_tool)'"; cat "$T/out"
fi

# C. the cached tool now emits a version this tree refuses: a bare
#    re-configure stops at the bootstrap, naming the tool and both numbers.
check
stub "$T/tool-b" 999999
if configure; then
    fail "C: a cached tool emitting 999999 configured (accepts $min..$max)"; cat "$T/out"
elif grep -q "cached CLI:        $T/tool-b" "$T/out" \
    && grep -q "it emits codegen:  999999" "$T/out" \
    && grep -q "this tree accepts: $min..$max" "$T/out"; then
    log_success "C: a cached tool this tree cannot use is refused at configure"
else
    fail "C: refused, but not with the bootstrap's message"; cat "$T/out"
fi

# D. a tool that cannot answer `--codegen-version` is not refused.
check
stub "$T/tool-b" fail
if configure; then
    log_success "D: a tool with no codegen-version answer is not refused"
else
    fail "D: a silent tool was refused"; cat "$T/out"
fi

echo "cmake-codegen-tool-cache: $CHECKS checks, $FAILURES failure(s)"
[ "$FAILURES" -eq 0 ]
