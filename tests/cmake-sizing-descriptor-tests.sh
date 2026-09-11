#!/bin/bash
# tests/cmake-sizing-descriptor-tests.sh -- phase-454 W4 (RFC-0100 D4)
#
# Exercises the CMake READER for the sizing descriptor:
# `cmake/NanoRosSizingDescriptor.cmake`. Sibling of
# `tests/cmake-entity-inventory-tests.sh` and shaped like it, including the
# stubbed CLI.
#
# THE CLI IS STUBBED, DELIBERATELY. The descriptor's schema, its per-field
# refusal and its CMake projection are all unit-tested in Rust
# (`nros-sizing-descriptor`, `nros_cli_core::sizing_descriptor`). What CMAKE
# owns is a different set of facts, and each is a failure mode this tree has
# already paid for once:
#
#   A. The values reach the CALLER's scope. `include()` inside a function keeps
#      its `set()`s in that frame, which pops -- the `_NROS_ENTRY_DIR` trap in
#      AGENTS.md's CMake pitfalls, reached a different way. A reader that
#      publishes into the wrong scope reads as working right up until a caller
#      asks.
#
#   B. A REFUSED field has NO value variable and a `_REFUSED` reason instead, so
#      `if(DEFINED ...)` is the only road to a number (RFC-0100 D6).
#
#   C. The descriptor is registered in CMAKE_CONFIGURE_DEPENDS -- issue 1018.
#      `execute_process()` has already run by the time ninja decides anything,
#      so this list is the ONLY thing that makes the emitted fragment fresh.
#
#   C2. It is registered even when the descriptor does NOT EXIST YET. CMake
#      re-configures when a listed file appears, so this is what makes the first
#      `nros sync` after a configure visible; without it that sync is invisible
#      until something unrelated triggers a reconfigure.
#
#   D. A MISSING descriptor is a reported NO-OP, not a fatal. An image nobody
#      has synced has none, and every consumer keeps its own defaults.
#
#   E. A descriptor that EXISTS and cannot be read is FATAL. Somebody generated
#      it; sizing from our own literals while a user believes they supplied
#      numbers is the silent default this artifact exists to remove. This is the
#      NEGATIVE CONTROL for the whole transport.
#
#   F. The endpoint columns stay aligned. A refused cell is the literal
#      `REFUSED`; an empty list element would vanish on the next `list()`
#      operation and mis-align every row after it.
#
# Usage: ./tests/cmake-sizing-descriptor-tests.sh
# Exit:  0 all assertions held; 1 otherwise.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# shellcheck source=lib/common.sh
source "$SCRIPT_DIR/lib/common.sh"

MODULE="$PROJECT_ROOT/cmake/NanoRosSizingDescriptor.cmake"

FAILURES=0
CHECKS=0

fail() {
    log_error "$*"
    FAILURES=$((FAILURES + 1))
}

check() {
    CHECKS=$((CHECKS + 1))
}

if [ ! -f "$MODULE" ]; then
    fail "module not found: $MODULE"
    exit 1
fi
if ! command -v cmake >/dev/null 2>&1; then
    fail "cmake is not on PATH -- this test cannot report a verdict without it"
    exit 1
fi

init_test_tmpdir "nros-sizing-descriptor"
trap 'cleanup_test_tmpdir' EXIT

# ---------------------------------------------------------------------------
# A stub `nros`. It writes the fragment named by --output-cmake, copying a body
# this script supplies through NROS_STUB_BODY, and exits with NROS_STUB_RC.
#
# The bodies below are the shape `nros_cli_core::sizing_descriptor::to_cmake`
# emits (see its unit tests) -- if that emitter changes shape, these stop
# matching and this test is where it is noticed.
# ---------------------------------------------------------------------------
STUB="$TEST_TMPDIR/nros-stub"
cat > "$STUB" <<'STUB_EOF'
#!/bin/bash
out=""
prev=""
for a in "$@"; do
    if [ "$prev" = "--output-cmake" ]; then out="$a"; fi
    prev="$a"
done
if [ -n "${NROS_STUB_BODY:-}" ] && [ -n "$out" ]; then
    mkdir -p "$(dirname "$out")"
    cp "$NROS_STUB_BODY" "$out"
fi
if [ -n "${NROS_STUB_STDERR:-}" ]; then echo "$NROS_STUB_STDERR" >&2; fi
exit "${NROS_STUB_RC:-0}"
STUB_EOF
chmod +x "$STUB"

BODY="$TEST_TMPDIR/projection.cmake"
cat > "$BODY" <<'EOF'
set(NROS_SIZING_SCHEMA_VERSION 1)
set(NROS_SIZING_ENTRY "talker")
set(NROS_SIZING_STATUS "partial")
set(NROS_SIZING_BASIS "contract")
set(NROS_SIZING_UNDECLARED_ENDPOINTS 0)
set(NROS_SIZING_TARGET_POINTER_BYTES 4)
set(NROS_SIZING_TARGET_MAX_ALIGN 8)
set(NROS_SIZING_TARGET_HEAP_BUDGET_BYTES_REFUSED "the board states no memory rung")
set(NROS_SIZING_TYPES_DISTINCT_COUNT 2)
set(NROS_SIZING_ENDPOINT_COUNT 2)
set(NROS_SIZING_ENDPOINT_KIND "subscription;subscription")
set(NROS_SIZING_ENDPOINT_TYPE "sensor_msgs/msg/Image;std_msgs/msg/String")
set(NROS_SIZING_ENDPOINT_TOPIC "/image;/chatter")
set(NROS_SIZING_ENDPOINT_DEPTH "REFUSED;10")
set(NROS_SIZING_ENDPOINT_REGISTRATION_PATH "rust_typed_schemaless;rust_typed_schemaless")
set(NROS_SIZING_ENDPOINT_STORAGE_BYTES "REFUSED;12914")
EOF

# A driver that stands in for a real configure: it supplies the two commands
# the module reaches for (`nros_resolve_cli`, `nros_codegen_tool_reconfigure`),
# includes the module, calls it, and prints what landed IN THE DRIVER'S OWN
# SCOPE -- which is assertion A.
DRIVER="$TEST_TMPDIR/driver.cmake"
cat > "$DRIVER" <<'EOF'
function(nros_resolve_cli _out)
    set(${_out} "$ENV{NROS_TEST_CLI}" PARENT_SCOPE)
endfunction()
function(nros_codegen_tool_reconfigure _tool)
    # A GLOBAL property and not a variable: a `set(... CACHE)` inside a function
    # is shadowed by any normal variable of the same name in the caller, which
    # is how the first version of this stub reported "no" while the call had
    # happened.
    set_property(GLOBAL PROPERTY NROS_TEST_TOOL_RECONFIGURE_SEEN "yes")
endfunction()
include("$ENV{NROS_TEST_MODULE}")
nros_sizing_descriptor_read("$ENV{NROS_TEST_DESCRIPTOR}")
message(STATUS "SCOPE_STATUS=${NROS_SIZING_STATUS}")
message(STATUS "SCOPE_POINTER=${NROS_SIZING_TARGET_POINTER_BYTES}")
message(STATUS "SCOPE_ENDPOINT_COUNT=${NROS_SIZING_ENDPOINT_COUNT}")
message(STATUS "SCOPE_DEPTHS=${NROS_SIZING_ENDPOINT_DEPTH}")
message(STATUS "SCOPE_TOPICS=${NROS_SIZING_ENDPOINT_TOPIC}")
if(DEFINED NROS_SIZING_TARGET_HEAP_BUDGET_BYTES)
    message(STATUS "SCOPE_HEAP_DEFINED=yes value=${NROS_SIZING_TARGET_HEAP_BUDGET_BYTES}")
else()
    message(STATUS "SCOPE_HEAP_DEFINED=no")
endif()
message(STATUS "SCOPE_HEAP_REFUSED=${NROS_SIZING_TARGET_HEAP_BUDGET_BYTES_REFUSED}")
get_directory_property(_deps CMAKE_CONFIGURE_DEPENDS)
message(STATUS "SCOPE_CONFIGURE_DEPENDS=${_deps}")
get_property(_tool_seen GLOBAL PROPERTY NROS_TEST_TOOL_RECONFIGURE_SEEN)
message(STATUS "SCOPE_TOOL_RECONFIGURE=${_tool_seen}")
EOF

# A real project, not `cmake -P`: `set_property(DIRECTORY ... CMAKE_CONFIGURE_DEPENDS)`
# needs a directory scope, and assertion C is precisely about that property.
PROJ="$TEST_TMPDIR/proj"
mkdir -p "$PROJ"
cat > "$PROJ/CMakeLists.txt" <<'EOF'
cmake_minimum_required(VERSION 3.20)
project(nros_sizing_descriptor_reader_test NONE)
include("$ENV{NROS_TEST_DRIVER}")
EOF

# run <descriptor> <body-or-empty> <rc> -> stdout+stderr; returns cmake's rc
run() {
    local descriptor="$1" body="$2" rc="$3"
    rm -rf "$PROJ/build"
    NROS_TEST_MODULE="$MODULE" \
    NROS_TEST_DRIVER="$DRIVER" \
    NROS_TEST_CLI="$STUB" \
    NROS_TEST_DESCRIPTOR="$descriptor" \
    NROS_STUB_BODY="$body" \
    NROS_STUB_RC="$rc" \
        cmake -S "$PROJ" -B "$PROJ/build" 2>&1
}

DESCRIPTOR="$TEST_TMPDIR/build/nros/sizing/talker.toml"
mkdir -p "$(dirname "$DESCRIPTOR")"
# Content is irrelevant to the CMake side -- the CLI is what reads it. What
# matters here is only that it EXISTS.
echo 'schema_version = 1' > "$DESCRIPTOR"

# ---------------------------------------------------------------------------
# A. the values reach the CALLER's scope
# ---------------------------------------------------------------------------
log_info "A. a projection reaches the caller's scope"
OUT="$(run "$DESCRIPTOR" "$BODY" 0)"
check
if ! nros_grep_q "SCOPE_STATUS=partial" <<<"$OUT"; then
    fail "A: status did not reach the caller -- $OUT"
fi
check
if ! nros_grep_q "SCOPE_POINTER=4" <<<"$OUT"; then
    fail "A: [target] pointer_bytes did not reach the caller -- $OUT"
fi
check
if ! nros_grep_q "SCOPE_ENDPOINT_COUNT=2" <<<"$OUT"; then
    fail "A: endpoint count did not reach the caller -- $OUT"
fi

# ---------------------------------------------------------------------------
# B. a REFUSED field has no value variable, only a reason
# ---------------------------------------------------------------------------
log_info "B. a refused field publishes a reason and NO number"
check
if ! nros_grep_q "SCOPE_HEAP_DEFINED=no" <<<"$OUT"; then
    fail "B: a refused field left a value variable defined -- $OUT"
fi
check
if ! nros_grep_q "SCOPE_HEAP_REFUSED=the board states no memory rung" <<<"$OUT"; then
    fail "B: the refusal reason did not reach the caller -- $OUT"
fi

# ---------------------------------------------------------------------------
# C. the descriptor is on CMAKE_CONFIGURE_DEPENDS, and so is the tool
# ---------------------------------------------------------------------------
log_info "C. issue 1018 -- the descriptor and the tool are configure dependencies"
check
if ! nros_grep_q "SCOPE_CONFIGURE_DEPENDS=.*talker.toml" <<<"$OUT"; then
    fail "C: the descriptor is not a configure dependency -- $OUT"
fi
check
if ! nros_grep_q "SCOPE_TOOL_RECONFIGURE=yes" <<<"$OUT"; then
    fail "C: the CLI was not registered as a configure dependency -- $OUT"
fi

# ---------------------------------------------------------------------------
# F. the endpoint columns stay aligned, with REFUSED as a literal cell
# ---------------------------------------------------------------------------
log_info "F. a refused cell is a literal, so the columns stay aligned"
check
if ! nros_grep_q "SCOPE_DEPTHS=REFUSED;10" <<<"$OUT"; then
    fail "F: the depth column lost its refused cell -- $OUT"
fi
check
if ! nros_grep_q "SCOPE_TOPICS=/image;/chatter" <<<"$OUT"; then
    fail "F: the topic column did not survive -- $OUT"
fi

# ---------------------------------------------------------------------------
# D + C2. a MISSING descriptor is a reported no-op, still watched
# ---------------------------------------------------------------------------
log_info "D. a missing descriptor reports and keeps the consumer's defaults"
MISSING="$TEST_TMPDIR/build/nros/sizing/absent.toml"
OUT="$(run "$MISSING" "$BODY" 0)"
RC=$?
check
if [ "$RC" -ne 0 ]; then
    fail "D: a missing descriptor was fatal -- $OUT"
fi
check
if ! nros_grep_q "no sizing descriptor at" <<<"$OUT"; then
    fail "D: a missing descriptor was silent -- $OUT"
fi
check
if ! nros_grep_q "SCOPE_ENDPOINT_COUNT=$" <<<"$OUT"; then
    fail "D: something was published for a descriptor that does not exist -- $OUT"
fi
log_info "C2. a descriptor that does not exist yet is still watched"
check
if ! nros_grep_q "SCOPE_CONFIGURE_DEPENDS=.*absent.toml" <<<"$OUT"; then
    fail "C2: an absent descriptor was not registered, so the sync that creates it \
would be invisible -- $OUT"
fi

# ---------------------------------------------------------------------------
# E. the NEGATIVE CONTROL -- a descriptor that exists and cannot be read
# ---------------------------------------------------------------------------
log_info "E. a descriptor that exists and cannot be read is FATAL"
OUT="$(NROS_STUB_STDERR='sizing descriptor does not parse: expected an integer' \
       run "$DESCRIPTOR" "" 1)"
RC=$?
check
if [ "$RC" -eq 0 ]; then
    fail "E: a broken descriptor configured cleanly -- the consumer would size from its \
own literals while a user believes they supplied numbers -- $OUT"
fi
check
# CMake wraps a long `message(FATAL_ERROR)`, so match a fragment that
# cannot land on a line boundary.
if ! nros_grep_q "could not be read" <<<"$OUT"; then
    fail "E: the fatal did not name the failure -- $OUT"
fi
check
if ! nros_grep_q "nros sync" <<<"$OUT"; then
    fail "E: the fatal did not name the remedy -- $OUT"
fi

# ---------------------------------------------------------------------------
echo
if [ "$FAILURES" -eq 0 ]; then
    log_success "cmake sizing-descriptor reader: $CHECKS checks passed"
    exit 0
fi
log_error "cmake sizing-descriptor reader: $FAILURES of $CHECKS checks failed"
exit 1
