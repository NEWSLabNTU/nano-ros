#!/bin/bash
# tests/cmake-reconfigure-tests.sh -- issue 0991
#
# Gates `cmake/NanoRosReconfigure.cmake`: the mechanism that makes a fragment
# written LATE in a configure reach the readers that already ran, inside the
# same build.
#
# WHY THIS IS A REAL PROJECT AND NOT `cmake -P`. Its two siblings
# (`cmake-message-bounds-tests.sh`, `cmake-entity-inventory-tests.sh`) drive
# their modules in script mode, because what they assert is a DERIVATION -- a
# pure function of a fragment. What this module does is not a derivation: it is
# a claim about what NINJA does with `build.ninja` and an mtime. Script mode
# cannot observe that, and the defect being fixed is precisely a mechanism that
# READ as working in every review and never fired. So the assertion has to be a
# real configure followed by a real `ninja`, and the project below is the
# smallest one that has the shape: a fragment read early, rewritten late.
#
# It needs cmake + ninja and NOTHING else -- no compiler (`project(... NONE)`),
# no nano-ros build, no SDK, no codegen. Two seconds.
#
# NOTHING HERE MAY DEPEND ON THE CLOCK OR ON HOST SPEED. The probe projects
# declare `cmake_minimum_required(VERSION 3.20)` and must behave identically on
# every cmake at or above it: `string(TIMESTAMP ... "%f")` is a cmake >= 3.23
# feature that DEGRADES SILENTLY to second granularity below, which made case E
# pass on cmake 4.3 and fail on an older one -- a green that depended on the
# reviewer's toolchain. Anything that must differ between two passes uses a
# cache counter.
#
# WHAT IS ASSERTED:
#
#   A. THE BUG. Without the mechanism -- `CMAKE_CONFIGURE_DEPENDS` plus a write
#      during the configure, which is exactly what three call sites claimed was
#      self-healing -- `ninja` does NOT re-run cmake, and the build proceeds
#      with the placeholder. This case is the control. It is here because the
#      whole issue is that the absent mechanism was indistinguishable from a
#      present one; a fix with no failing control is the same mistake again.
#
#   B. THE FIX. With `nros_reconfigure_on_change`, the same project re-runs
#      cmake during `ninja` and the build proceeds with the REAL answer.
#
#   C. TERMINATION. It re-runs EXACTLY ONCE. A mechanism that dates a file
#      forward can loop until the wall clock catches up -- measured at 100
#      re-configures before `nros_reconfigure_settle` existed -- so "it
#      converges" is not a detail, it is the difference between a fix and an
#      outage. A second `ninja` re-runs cmake zero times.
#
#   D. IDENTICAL BYTES ARM NOTHING. A producer that rewrites the same content
#      every configure must not re-arm, or every build re-configures forever.
#
#   E. THE BOUND HOLDS. With `NROS_RECONFIGURE_MAX_PASSES` exhausted by a
#      deliberately non-convergent producer, the build completes with a WARNING
#      rather than looping. An escape hatch that cannot be exhausted is not a
#      bound.
#
#   F. `nros_reconfigure_settle` is a no-op on an ordinary file. It runs on
#      every reader on every configure, so "cheap and silent when nothing is
#      armed" is part of the contract.
#
# PRECONDITIONS ARE HARD FAILURES. This script never prints-and-returns: a green
# here is read as "the re-configure mechanism works". Skipping belongs in the
# `just` recipe's check ledger, which is a different claim from "it passed".
#
# Usage: ./tests/cmake-reconfigure-tests.sh
# Exit:  0 all assertions held; 1 otherwise.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# shellcheck source=lib/common.sh
source "$SCRIPT_DIR/lib/common.sh"

MODULE="$PROJECT_ROOT/cmake/NanoRosReconfigure.cmake"

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
for _t in cmake ninja touch; do
    if ! command -v "$_t" >/dev/null 2>&1; then
        fail "$_t is not on PATH -- this test cannot report a verdict without it"
        exit 1
    fi
done

init_test_tmpdir "nros-reconfigure"
trap 'cleanup_test_tmpdir' EXIT

# ---------------------------------------------------------------------------
# The project under test.
#
# `$1` selects the producer's behaviour, so one source tree covers every case:
#
#   armed         -- the module's mechanism (the fix)
#   bare          -- CMAKE_CONFIGURE_DEPENDS alone (the bug, case A)
#   identical     -- rewrites the same bytes every configure (case D)
#   never-settles -- writes a DIFFERENT answer every configure (case E)
#
# The `go` target echoes the answer the configure that generated it had read,
# so what the BUILD used is observable in ninja's output rather than inferred
# from the fragment on disk. That distinction is the entire issue: the fragment
# was always correct; what was wrong was the answer the build was sized from.
# ---------------------------------------------------------------------------
write_project() {
    local dir="$1" mode="$2"
    mkdir -p "$dir"
    cat > "$dir/CMakeLists.txt" <<EOF
cmake_minimum_required(VERSION 3.20)
project(nros_reconfigure_probe NONE)

include("$MODULE")

set(_frag "\${CMAKE_BINARY_DIR}/frag.cmake")
set(_mode "$mode")

# ---- the READER, early in the configure -------------------------------------
if(NOT EXISTS "\${_frag}")
    file(WRITE "\${_frag}" "set(ANSWER placeholder)\n")
endif()
set_property(DIRECTORY APPEND PROPERTY CMAKE_CONFIGURE_DEPENDS "\${_frag}")
if(NOT _mode STREQUAL "bare")
    nros_reconfigure_settle("\${_frag}")
endif()
include("\${_frag}")
message(STATUS "PROBE_READ=\${ANSWER}")

# ---- the PRODUCER, later in the same configure -------------------------------
nros_reconfigure_snapshot("\${_frag}" _before)
if(_mode STREQUAL "identical")
    file(WRITE "\${_frag}" "set(ANSWER placeholder)\n")
elseif(_mode STREQUAL "never-settles")
    # DETERMINISTIC non-convergence -- a counter in the cache, never a clock.
    # This used string(TIMESTAMP) with a %f microsecond field, which needs
    # cmake 3.23 or newer. Below that it does not error, it silently gives
    # SECOND granularity, so two re-configures inside one second write
    # IDENTICAL bytes: the producer that must look non-convergent looks
    # convergent, nothing arms the second pass, the bound is never reached and
    # case E fails on its own assertion. A test for a bound must not depend on
    # how fast the host is, nor on which cmake the reviewer happens to run.
    #
    # NOTE this block is inside an UNQUOTED heredoc, so backticks and bare $
    # are shell-expanded. Write no backticks here.
    if(NOT DEFINED NROS_PROBE_SEQ)
        set(NROS_PROBE_SEQ 0 CACHE INTERNAL "probe: configures so far")
    endif()
    math(EXPR _seq "\${NROS_PROBE_SEQ} + 1")
    set(NROS_PROBE_SEQ "\${_seq}" CACHE INTERNAL "probe: configures so far")
    file(WRITE "\${_frag}" "set(ANSWER moving_\${_seq})\n")
else()
    file(WRITE "\${_frag}" "set(ANSWER real)\n")
endif()
if(NOT _mode STREQUAL "bare")
    nros_reconfigure_on_change("\${_frag}" "\${_before}" LABEL "the probe answer")
endif()

add_custom_target(go ALL
    COMMAND \${CMAKE_COMMAND} -E echo "PROBE_BUILT=\${ANSWER}")
EOF
}

# Run `ninja` once and report what the build actually used plus how many times
# cmake re-ran. `timeout` is a guard, not a convenience: the failure mode this
# mechanism can have IS an unbounded re-configure loop, and a test that hangs
# reports nothing.
run_build() {
    local build="$1" out
    out="$(timeout 120 ninja -C "$build" 2>&1)"
    BUILD_RC=$?
    BUILD_OUT="$out"
    BUILD_USED="$(printf '%s\n' "$out" | sed -n 's/.*PROBE_BUILT=\([A-Za-z0-9_]*\).*/\1/p' | tail -1)"
    BUILD_RERUNS="$(printf '%s\n' "$out" | grep -c 'Re-running CMake')"
}

log_header "A. the control: CONFIGURE_DEPENDS alone does NOT re-run cmake"

SRC_BARE="$TEST_TMPDIR/bare-src"
write_project "$SRC_BARE" "bare"
cmake -G Ninja -S "$SRC_BARE" -B "$TEST_TMPDIR/bare-build" >/dev/null 2>&1
run_build "$TEST_TMPDIR/bare-build"
check
if [ "$BUILD_USED" = "placeholder" ] && [ "$BUILD_RERUNS" -eq 0 ]; then
    log_success "bare CONFIGURE_DEPENDS: 0 re-runs, built with the placeholder (the bug reproduces)"
else
    fail "the control did not reproduce the bug: used='$BUILD_USED' re-runs=$BUILD_RERUNS.
  If ninja has started honouring a same-configure write, this module is
  obsolete rather than broken -- check that before deleting the assertion."
fi

log_header "B/C. the fix: exactly one re-configure, and the build uses the real answer"

SRC_ARMED="$TEST_TMPDIR/armed-src"
write_project "$SRC_ARMED" "armed"
CONFIGURE_OUT="$(cmake -G Ninja -S "$SRC_ARMED" -B "$TEST_TMPDIR/armed-build" 2>&1)"
check
if printf '%s\n' "$CONFIGURE_OUT" | grep -q 'PROBE_READ=placeholder'; then
    log_success "first pass read the placeholder, as it must"
else
    fail "first pass did not read the placeholder:
$CONFIGURE_OUT"
fi

check
if printf '%s\n' "$CONFIGURE_OUT" | grep -q 'cmake will run once more'; then
    log_success "the changed answer announced the re-configure"
else
    fail "a changed answer armed nothing, or said nothing about it:
$CONFIGURE_OUT"
fi

run_build "$TEST_TMPDIR/armed-build"
check
if [ "$BUILD_RC" -ne 0 ]; then
    fail "the build failed (rc=$BUILD_RC):
$BUILD_OUT"
elif [ "$BUILD_USED" = "real" ]; then
    log_success "the build used the REAL answer, not the placeholder"
else
    fail "the build used '$BUILD_USED', expected 'real':
$BUILD_OUT"
fi

check
if [ "$BUILD_RERUNS" -eq 1 ]; then
    log_success "exactly 1 re-configure -- it converges"
else
    fail "expected exactly 1 re-configure, saw $BUILD_RERUNS.
  More than one is the future-dated-mtime loop this mechanism must not have:
  \`nros_reconfigure_settle\` is what clears the date on the next pass."
fi

# A settled tree must stay settled. This is the half that a future-dated mtime
# gets wrong for as long as the clock is behind it.
run_build "$TEST_TMPDIR/armed-build"
check
if [ "$BUILD_RERUNS" -eq 0 ] && [ "$BUILD_RC" -eq 0 ]; then
    log_success "a second build re-configures 0 times -- the date was cleared"
else
    fail "a second build re-ran cmake $BUILD_RERUNS time(s) (rc=$BUILD_RC); the future date was not cleared:
$BUILD_OUT"
fi

log_header "D. identical bytes arm nothing"

SRC_SAME="$TEST_TMPDIR/identical-src"
write_project "$SRC_SAME" "identical"
CONFIGURE_OUT="$(cmake -G Ninja -S "$SRC_SAME" -B "$TEST_TMPDIR/identical-build" 2>&1)"
check
if printf '%s\n' "$CONFIGURE_OUT" | grep -q 'cmake will run once more'; then
    fail "a producer that rewrote IDENTICAL bytes armed a re-configure -- every build would re-configure forever"
else
    log_success "identical bytes armed nothing"
fi
run_build "$TEST_TMPDIR/identical-build"
check
if [ "$BUILD_RERUNS" -eq 0 ]; then
    log_success "and the build re-ran cmake 0 times"
else
    fail "identical bytes still caused $BUILD_RERUNS re-configure(s)"
fi

log_header "E. the bound holds: a non-convergent producer warns, it does not loop"

SRC_LOOP="$TEST_TMPDIR/loop-src"
write_project "$SRC_LOOP" "never-settles"
cmake -G Ninja -S "$SRC_LOOP" -B "$TEST_TMPDIR/loop-build" \
    -DNROS_RECONFIGURE_MAX_PASSES=2 >/dev/null 2>&1
run_build "$TEST_TMPDIR/loop-build"
check
if [ "$BUILD_RC" -eq 124 ]; then
    fail "a producer whose answer never settles LOOPED until the timeout -- NROS_RECONFIGURE_MAX_PASSES did not bound it"
elif [ "$BUILD_RC" -ne 0 ]; then
    fail "the bounded build failed for another reason (rc=$BUILD_RC):
$BUILD_OUT"
elif [ "$BUILD_RERUNS" -le 2 ]; then
    log_success "bounded at $BUILD_RERUNS re-configure(s) with MAX_PASSES=2"
else
    fail "MAX_PASSES=2 allowed $BUILD_RERUNS re-configures"
fi

check
if printf '%s\n' "$BUILD_OUT" | grep -q 'NROS_RECONFIGURE_MAX_PASSES'; then
    log_success "and it said WHY it stopped, naming the knob"
else
    fail "the bound was hit silently -- a build sized from a stale answer must say so:
$BUILD_OUT"
fi

log_header "F. settle is a no-op on an ordinary file"

SETTLE_PROBE="$TEST_TMPDIR/settle-probe.cmake"
ORDINARY="$TEST_TMPDIR/ordinary.txt"
echo "content" > "$ORDINARY"
BEFORE_MTIME="$(stat -c %Y "$ORDINARY")"
cat > "$SETTLE_PROBE" <<EOF
include("$MODULE")
nros_reconfigure_settle("$ORDINARY")
nros_reconfigure_settle("$TEST_TMPDIR/does-not-exist.txt")
message(STATUS "SETTLE_OK")
EOF
# The past mtime makes "unchanged" observable: `file(TOUCH)` would move it to
# now, so an equal mtime is proof the no-op path ran rather than the clearing
# one landing on the same second.
touch -d "@$((BEFORE_MTIME - 3600))" "$ORDINARY"
BEFORE_MTIME="$(stat -c %Y "$ORDINARY")"
SETTLE_OUT="$(cmake -P "$SETTLE_PROBE" 2>&1)"
AFTER_MTIME="$(stat -c %Y "$ORDINARY")"
check
if printf '%s\n' "$SETTLE_OUT" | grep -q 'SETTLE_OK' && [ "$BEFORE_MTIME" = "$AFTER_MTIME" ]; then
    log_success "settle left an ordinary file alone, and a missing file is not an error"
else
    fail "settle was not a no-op: before=$BEFORE_MTIME after=$AFTER_MTIME
$SETTLE_OUT"
fi

log_header "G. INTEGRATION: a clean build dir sizes itself from the SUBSCRIBED set"

# The acceptance issue 0991 names, at the level this gate can reach: configure a
# CLEAN build dir once, build, and require that the build was sized from the
# image's declaration rather than from the "nothing composed yet" placeholder.
#
# This drives the REAL modules in the REAL order -- `nros_derive_message_bound_
# knobs` (the reader, standing in for the end of `nros_find_interfaces()`) and
# then `nros_derive_entity_inventory_knobs` (the producer, standing in for
# `nano_ros_entry()`) -- so it fails if either call site loses its
# snapshot/on_change pair, which a module-only test cannot see.
#
# The island's own symptom was one step further down: the closure basis set the
# small payload class from a `std_msgs/Float64MultiArray` the image links
# through and never receives, and the image overflowed RAM by 103160 bytes at
# LINK. The linking half needs a cross toolchain and a 320 KiB part; the BASIS
# half is the cause and is checkable here.

INT_SRC="$TEST_TMPDIR/integration-src"
INT_BUILD="$TEST_TMPDIR/integration-build"
mkdir -p "$INT_SRC"

# Two bounded types. `Float64MultiArray` is the big one the image LINKS and does
# not subscribe to; `Int32` is what it actually receives. The closure basis
# picks the first, the subscribed basis the second -- so the two bases are
# distinguishable by the number alone.
BOUNDS_FRAG="$TEST_TMPDIR/integration-bounds.cmake"
cat > "$BOUNDS_FRAG" <<'EOF'
set(NROS_MESSAGE_BOUNDS_SCHEMA_VERSION 1)
list(APPEND NROS_MESSAGE_BOUND_PACKAGES "std_msgs")
list(APPEND NROS_MESSAGE_BOUND_TYPES "std_msgs/msg/Float64MultiArray")
set(NROS_MESSAGE_BOUND_std_msgs_msg_Float64MultiArray_STATE "bounded")
set(NROS_MESSAGE_BOUND_std_msgs_msg_Float64MultiArray_TX 1496)
set(NROS_MESSAGE_BOUND_std_msgs_msg_Float64MultiArray_RX 1496)
list(APPEND NROS_MESSAGE_BOUND_TYPES "std_msgs/msg/Int32")
set(NROS_MESSAGE_BOUND_std_msgs_msg_Int32_STATE "bounded")
set(NROS_MESSAGE_BOUND_std_msgs_msg_Int32_TX 880)
set(NROS_MESSAGE_BOUND_std_msgs_msg_Int32_RX 880)
list(REMOVE_DUPLICATES NROS_MESSAGE_BOUND_PACKAGES)
list(REMOVE_DUPLICATES NROS_MESSAGE_BOUND_TYPES)
EOF

# The entity fragment the stubbed `nros ws entity-inventory` composes: this
# image subscribes to Int32 only.
# The schema version is READ FROM THE MODULE, never hardcoded. Its sibling
# `cmake-message-bounds-tests.sh` records why in its own words: a literal here
# "silently stopped testing anything it claimed to" the moment the supported
# version moved. It moved again on 2026-09-03 (2 -> 3, phase-403 step 2's
# `@depth=`), and a hardcoded 2 turned this case into a configure FATAL rather
# than a wrong answer -- loud, but for the wrong reason.
ENTITY_SCHEMA="$(sed -n 's/^set(NROS_ENTITY_INVENTORY_SCHEMA_SUPPORTED \([0-9]*\).*/\1/p' \
    "$PROJECT_ROOT/cmake/NanoRosEntityInventory.cmake" | head -1)"
if [ -z "$ENTITY_SCHEMA" ]; then
    fail "could not read NROS_ENTITY_INVENTORY_SCHEMA_SUPPORTED from the module"
    exit 1
fi

ENTITY_BODY="$TEST_TMPDIR/integration-entity.cmake"
cat > "$ENTITY_BODY" <<EOF
set(NROS_ENTITY_INVENTORY_SCHEMA_VERSION ${ENTITY_SCHEMA})
set(NROS_ENTITY_INVENTORY_STATUS "derived")
set(NROS_ENTITY_INVENTORY_COMPONENT_COUNT 1)
set(NROS_ENTITY_INVENTORY_ENTITY_TOTAL 1)
set(NROS_ENTITY_COUNT_SUBSCRIPTION 1)
set(NROS_DERIVED_EXECUTOR_MAX_CBS 1)
set(NROS_ENTITY_SUBSCRIBED_TYPES_STATUS "resolved")
set(NROS_ENTITY_SUBSCRIBED_TYPES "std_msgs/msg/Int32")
set(NROS_ENTITY_SUBSCRIBED_TYPE_COUNTS "std_msgs/msg/Int32=1")
set(NROS_ENTITY_SUBSCRIBED_ENTITY_COUNT 1)
set(NROS_ENTITY_RECEIVED_TYPES_STATUS "resolved")
set(NROS_ENTITY_RECEIVED_TYPES "std_msgs/msg/Int32")
set(NROS_ENTITY_RECEIVED_TYPE_COUNTS "std_msgs/msg/Int32=1")
set(NROS_ENTITY_RECEIVED_ENTITY_COUNT 1)
EOF

INT_STUB="$TEST_TMPDIR/integration-nros"
cat > "$INT_STUB" <<'STUB_EOF'
#!/bin/bash
out=""
prev=""
for a in "$@"; do
    if [ "$prev" = "--output-cmake" ]; then out="$a"; fi
    prev="$a"
done
if [ -n "$out" ]; then
    mkdir -p "$(dirname "$out")"
    cp "$NROS_STUB_BODY" "$out"
fi
exit 0
STUB_EOF
chmod +x "$INT_STUB"

# `nros_derive_entity_inventory_knobs` refuses without a metadata file, exactly
# as a launch-only image would -- so give it one. Its CONTENT is the stub's
# business, not this module's.
INT_META="$TEST_TMPDIR/integration-metadata.json"
echo '{}' > "$INT_META"

cat > "$INT_SRC/CMakeLists.txt" <<EOF
cmake_minimum_required(VERSION 3.20)
project(nros_reconfigure_integration NONE)

include("$PROJECT_ROOT/cmake/NanoRosMessageBounds.cmake")
include("$PROJECT_ROOT/cmake/NanoRosEntityInventory.cmake")
include("$PROJECT_ROOT/cmake/NanoRosReconfigure.cmake")

nros_entity_inventory_knobs_file(_entity)
nros_message_bounds_knobs_file(_bounds)
nros_entity_inventory_seed_knobs_file("\${_entity}")
nros_message_bounds_seed_knobs_file("\${_bounds}")
set_property(DIRECTORY APPEND PROPERTY CMAKE_CONFIGURE_DEPENDS "\${_entity}" "\${_bounds}")

# ---- the READER half, as \`nros_find_interfaces()\` runs it ---------------------
nros_reconfigure_settle("\${_entity}")
nros_reconfigure_settle("\${_bounds}")
nros_reconfigure_snapshot("\${_bounds}" _bounds_before)
nros_derive_message_bound_knobs(
    FRAGMENTS "$BOUNDS_FRAG"
    OUTPUT_FILE "\${_bounds}"
    ENTITY_INVENTORY "\${_entity}")
nros_reconfigure_on_change("\${_bounds}" "\${_bounds_before}"
    LABEL "this image's derived message-bound sizes")

# ---- the PRODUCER half, as \`nano_ros_entry()\` runs it ------------------------
nros_reconfigure_snapshot("\${_entity}" _entity_before)
nros_derive_entity_inventory_knobs(CLI "$INT_STUB" METADATA "$INT_META" QUIET)
nros_reconfigure_on_change("\${_entity}" "\${_entity_before}"
    LABEL "this image's entity inventory")

add_custom_target(go ALL COMMAND \${CMAKE_COMMAND} -E echo
    "PROBE_BASIS=\${NROS_MESSAGE_BOUNDS_BASIS} PROBE_SMALL=\${NROS_DERIVED_SUBSCRIBER_BUFFER_SIZE}")
EOF

NROS_STUB_BODY="$ENTITY_BODY" \
    cmake -G Ninja -S "$INT_SRC" -B "$INT_BUILD" >/dev/null 2>&1
INT_OUT="$(NROS_STUB_BODY="$ENTITY_BODY" timeout 120 ninja -C "$INT_BUILD" 2>&1)"
INT_RC=$?
INT_BASIS="$(printf '%s\n' "$INT_OUT" | sed -n 's/.*PROBE_BASIS=\([a-z]*\).*/\1/p' | tail -1)"
INT_SMALL="$(printf '%s\n' "$INT_OUT" | sed -n 's/.*PROBE_SMALL=\([0-9]*\).*/\1/p' | tail -1)"

check
if [ "$INT_RC" -ne 0 ]; then
    fail "the integration build failed (rc=$INT_RC):
$INT_OUT"
elif [ "$INT_BASIS" = "subscribed" ]; then
    log_success "one build from nothing: basis=subscribed (not the closure placeholder)"
else
    fail "a CLEAN build dir built on basis='$INT_BASIS', expected 'subscribed'.
  That is issue 0991 exactly: the pass that BUILT was sized from the entity
  fragment's placeholder, so the payload classes came from the linked closure.
$INT_OUT"
fi

check
if [ "$INT_SMALL" = "880" ]; then
    log_success "and the small payload class is 880 -- the type it receives, not the 1496 it merely links"
else
    fail "the small payload class built as '$INT_SMALL', expected 880.
  1496 is the closure answer (std_msgs/msg/Float64MultiArray), which this image
  links through and never receives -- the island's 103160-byte overflow.
$INT_OUT"
fi

log_header "Result"
if [ "$FAILURES" -eq 0 ]; then
    log_success "$CHECKS/$CHECKS assertions held"
    exit 0
fi
log_error "$FAILURES of $CHECKS assertions failed"
exit 1
