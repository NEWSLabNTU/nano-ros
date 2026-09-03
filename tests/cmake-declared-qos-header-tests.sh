#!/bin/bash
# tests/cmake-declared-qos-header-tests.sh -- phase-403 step 2
#
# Does the DECLARED `@depth=` actually REACH A COMPILER?
#
# The check itself is proven by `just check cpp`, which compiles a TU whose
# declared depth and passed QoS disagree and asserts the build is rejected. That
# gate hands the table to the compiler with a `-I` of its own, so it proves the
# MECHANISM and says nothing about the DELIVERY.
#
# This gate is the delivery half, and it is the half this campaign keeps
# getting wrong: six sizing mechanisms so far have been correct and unreachable,
# because the number never arrived where the code that reads it runs. Here the
# unreachable shape is silent by construction -- a component whose table never
# lands compiles with every declared-depth assertion disabled, and looks exactly
# like a component whose depths all agree.
#
# So this drives `_nros_emit_declared_qos_header()` in a REAL configure, with
# the REAL `nros` CLI, and asserts:
#
#   A. the header lands at the path the include spelling needs
#      (`<dir>/nros/nros_declared_qos_generated.h`), with the declared depth in
#      it, keyed on BOTH type spellings;
#   B. `<dir>` is on the target's include path, so `__has_include` in
#      `nros/declared_qos.hpp` finds it;
#   C. the dir is PRIVATE -- a consumer that links the component must not
#      inherit a table describing somebody else's call sites;
#   D. a component that declares no `ENTITIES` gets NO header and NO error.
#      Absence is not zero here either: that image has not opted in;
#   E. a BROKEN declaration is FATAL and names the component. "You have not
#      declared" and "what you declared is wrong" license different actions,
#      and only the second may stop the build.
#
# Needs cmake, a C compiler and a built `nros` (`just setup-cli`). The `just`
# recipe records a SKIP when any is missing -- which is a different claim from
# "it passed", and belongs in the check ledger rather than here.
#
# Usage: ./tests/cmake-declared-qos-header-tests.sh
# Exit:  0 all assertions held; 1 otherwise.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# shellcheck source=lib/common.sh
source "$SCRIPT_DIR/lib/common.sh"

MODULE="$PROJECT_ROOT/cmake/NanoRosNodeRegister.cmake"

FAILURES=0
CHECKS=0
fail() { log_error "$*"; FAILURES=$((FAILURES + 1)); }
check() { CHECKS=$((CHECKS + 1)); }

if [ ! -f "$MODULE" ]; then
    fail "module not found: $MODULE"
    exit 1
fi
if ! command -v cmake >/dev/null 2>&1; then
    fail "cmake is not on PATH -- this test cannot report a verdict without it"
    exit 1
fi

# The REAL CLI, not a stub: the CONTENT of the header is the thing under test,
# and a stub that writes a file only proves cmake can copy one.
NROS_BIN="${NROS_CLI:-$PROJECT_ROOT/packages/cli/target/release/nros}"
if [ ! -x "$NROS_BIN" ]; then
    fail "no built \`nros\` at $NROS_BIN -- run \`just setup-cli\`"
    exit 1
fi

init_test_tmpdir "nros-declared-qos-header"
trap 'cleanup_test_tmpdir' EXIT

# One tiny project per case. `_nros_emit_declared_qos_header` is called
# directly rather than through `nano_ros_node_register()`, because that verb
# needs the whole `find_package(nano_ros)` surface and this gate is about ONE
# seam. The arguments are exactly the ones the register call passes.
#
#   configure <case> <entities-json-or-empty> [extra cmake lines]
configure() {
    local name="$1" entities="$2" extra="${3:-}"
    local dir="$TEST_TMPDIR/$name"
    mkdir -p "$dir/build"
    echo "int nros_dq_probe(void) { return 0; }" > "$dir/x.c"
    {
        echo 'cmake_minimum_required(VERSION 3.22)'
        # The pkg in the metadata is `PROJECT_NAME`, because that is what
        # `_nros_metadata_emit()` writes -- so the project must be `demo` for
        # the fixture's `demo::listener` row to be the one narrowed to.
        echo 'project(demo LANGUAGES C)'
        echo "include(\"$MODULE\")"
        echo 'add_library(demo_listener_component STATIC x.c)'
        echo 'set(_NRC_ENTITIES stub)'
        echo '_nros_emit_declared_qos_header("listener" "demo_listener_component" "CPP")'
        echo 'get_target_property(_inc demo_listener_component INCLUDE_DIRECTORIES)'
        echo 'get_target_property(_iface demo_listener_component INTERFACE_INCLUDE_DIRECTORIES)'
        echo 'message(STATUS "PRIVATE_INCLUDES=[${_inc}]")'
        echo 'message(STATUS "IFACE_INCLUDES=[${_iface}]")'
        printf '%s\n' "$extra"
    } > "$dir/CMakeLists.txt"
    if [ -n "$entities" ]; then
        cp "$entities" "$dir/build/nros-metadata.json"
    fi
    (cd "$dir/build" && NROS_CLI="$NROS_BIN" cmake .. 2>&1)
}

FIXTURE="$PROJECT_ROOT/packages/api/nros-cpp/tests/compile/declared-qos-fixture/entities.json"
if [ ! -f "$FIXTURE" ]; then
    fail "the declared-QoS compile fixture is missing: $FIXTURE"
    exit 1
fi

# ---------------------------------------------------------------------------
# A + B + C. A declared depth reaches a header on the target's PRIVATE include
# path.
# ---------------------------------------------------------------------------
log_info "A. a declared depth reaches a header on the component's include path"
OUT="$(configure declared "$FIXTURE")"
HDR="$TEST_TMPDIR/declared/build/nros-declared-qos/listener/nros/nros_declared_qos_generated.h"
check
if [ ! -f "$HDR" ]; then
    fail "A: no header at $HDR -- the declaration never reaches a compiler.
Everything downstream (the static_assert, the boot-time check) then does nothing
at all, silently. Configure said: $OUT"
fi
if [ -f "$HDR" ]; then
    check
    if ! grep -q 'NROS_DECLARED_QOS_ROW("std_msgs::msg::dds_::Int32_", "/chatter", 1)' "$HDR"; then
        fail "A: the header carries no row for the declared endpoint -- $(cat "$HDR")"
    fi
    check
    if ! grep -q 'NROS_DECLARED_QOS_ROW("std_msgs/msg/Int32", "/chatter", 1)' "$HDR"; then
        fail "A: the ROS spelling of the type is missing, so a message class carrying
it would look undeclared -- $(cat "$HDR")"
    fi
    check
    # The endpoint the fixture declares NO depth for must produce no row. A row
    # at some default would be the whole defect this step exists to prevent.
    if grep -q '"/undeclared"' "$HDR"; then
        fail "A: an endpoint that declared no depth got a row -- absence became a number"
    fi
fi
check
if ! grep -q "PRIVATE_INCLUDES=\[.*nros-declared-qos/listener\]" <<<"$OUT"; then
    fail "B: the generated dir is not on the target's include path, so
\`__has_include(<nros/nros_declared_qos_generated.h>)\` finds nothing and every
call site in the component compiles unchecked -- $OUT"
fi
check
if grep -q "IFACE_INCLUDES=\[.*nros-declared-qos" <<<"$OUT"; then
    fail "C: the dir leaked onto INTERFACE_INCLUDE_DIRECTORIES. A consumer that
links this component would then have its OWN NROS_SUBSCRIBE calls checked against
this component's declaration -- $OUT"
fi

# ---------------------------------------------------------------------------
# D. No declaration is not an error.
# ---------------------------------------------------------------------------
log_info "D. a component that declares nothing gets no header and no error"
NO_ENT="$TEST_TMPDIR/no-entities.json"
cat > "$NO_ENT" <<'EOF'
{"components": [{"name": "listener", "pkg": "demo", "class": "demo::Listener"}]}
EOF
OUT="$(configure undeclared "$NO_ENT")"
check
if grep -q "CMake Error" <<<"$OUT"; then
    fail "D: a component with no ENTITIES broke the configure. Every image built
before this step is in exactly that state -- $OUT"
fi
check
if [ -f "$TEST_TMPDIR/undeclared/build/nros-declared-qos/listener/nros/nros_declared_qos_generated.h" ] && \
   grep -q "NROS_DECLARED_QOS_ROWS" \
        "$TEST_TMPDIR/undeclared/build/nros-declared-qos/listener/nros/nros_declared_qos_generated.h"; then
    fail "D: a component that declared nothing got a table of rows"
fi

# ---------------------------------------------------------------------------
# E. A broken declaration is FATAL and names the component.
# ---------------------------------------------------------------------------
log_info "E. a broken declaration stops the build and names the component"
BAD="$TEST_TMPDIR/bad.json"
cat > "$BAD" <<'EOF'
{"components": [{"name": "listener", "pkg": "demo", "class": "demo::Listener",
                 "entities": ["sub:std_msgs/msg/Int32:/chatter@depth=0"]}]}
EOF
OUT="$(configure broken "$BAD" | tr '\n' ' ' | tr -s ' ')"
check
if ! grep -q "CMake Error" <<<"$OUT"; then
    fail "E: a \`@depth=0\` declaration configured cleanly. Skipping it would leave
that component with no table and every check off -- $OUT"
fi
check
if ! grep -q "demo::listener" <<<"$OUT"; then
    fail "E: the failure does not name the component the user has to fix -- $OUT"
fi
check
if ! grep -q "states nothing" <<<"$OUT"; then
    fail "E: the CLI's own reason did not reach the user -- $OUT"
fi

echo ""
if [ "$FAILURES" -eq 0 ]; then
    log_success "cmake-declared-qos-header: $CHECKS assertion(s) held"
    exit 0
fi
log_error "cmake-declared-qos-header: $FAILURES of $CHECKS assertion(s) failed"
exit 1
