#!/bin/bash
# tests/cmake-message-bounds-tests.sh -- phase-403 W8 (issue 0940)
#
# Exercises the READER for the exported message-bound inventory:
# `cmake/NanoRosMessageBounds.cmake`, driven in `cmake -P` script mode against
# hand-written fragments. No nano-ros build, no toolchain, no codegen -- the
# fragments are the codegen CONTRACT, and writing them by hand is what lets a
# case like "one package is a schema ahead of the other" exist at all.
#
# WHAT IS ACTUALLY ASSERTED, and why each assertion exists:
#
#   A. A bounded closure DERIVES all four numbers, and derives the ones the
#      island bring-up produced by eye. The small class is the largest bound at
#      or under the split; the large count is how many types are above it; the
#      large size is the biggest of those; the take buffer is the biggest of
#      all. Asserting the VALUES and not just "it ran" is the point -- W6's
#      prototype was a one-off `cmake -P` whose answer nobody re-derived.
#
#   B. COMPOSITION over several packages. The image-wide answer is a property
#      of the closure, not of any one package: two fragments whose largest
#      types differ must produce the larger. A reader that took the last
#      fragment's answer would pass a single-package test.
#
#   C. An UNBOUNDED type REFUSES the whole derivation and names the type and
#      the member that costs it. This is requirement 3 of the wave and the one
#      that is tempting to get wrong: deriving over the bounded subset yields a
#      plausible number that a real sample can exceed, and the drop is silent
#      on the C++ arena path. The negative control matters as much -- the same
#      closure with the type bounded DOES derive.
#
#   D. An UNRESOLVED type refuses too. It is a different fact from unbounded
#      (a search-path problem, not a property of the message) and licenses the
#      same action: nothing may be sized from it.
#
#   E. A SCHEMA VERSION the reader does not understand is a FATAL_ERROR, not a
#      field-by-field read on the hope that nothing moved. Covers both shapes:
#      a wrong version, and a fragment that states none at all.
#
#   F. A MISSING fragment refuses rather than fataling -- on the canonical lane
#      codegen is a build-time custom command, so on a clean tree the file is a
#      promise. Refusing keeps that lane building; fataling would break it.
#
#   G. MAX_LARGE_SUBSCRIBERS = 0 is an ANSWER (every type fits the small
#      class), and the large SIZE is then deliberately NOT derived -- with zero
#      blocks the pool is zero bytes whatever size it names, and naming one
#      would be inventing a number. This is the case the reference image is in,
#      against a hand-set 2 / 2560.
#
#   I. The CONSUMER SEAM in a real configure + build. The knobs file is read
#      through CMAKE_CONFIGURE_DEPENDS, and that has a failure mode `cmake -P`
#      cannot show: a ninja input with no producing rule is `missing and no
#      known rule to make it`, raised at LOAD, which makes the whole build dir
#      unusable. The Zephyr lane is not buildable on a bare host; this seam is
#      generic cmake+ninja and is.
#
#   J. The REGISTRY path -- the one `nros_find_interfaces()` takes, where the
#      generators register fragments as they emit them and the composer is
#      called with none. Every other case passes fragments explicitly and would
#      keep passing with the registry broken.
#
#   H. The OUTPUT FILE carries the answer AND the provenance, and is
#      write-if-changed. The second is load-bearing rather than tidy: the
#      consumer registers the file with CMAKE_CONFIGURE_DEPENDS, so rewriting
#      identical bytes every configure would re-arm a reconfigure forever.
#
# PRECONDITIONS ARE HARD FAILURES. This script never prints-and-returns: a
# green here is read as "the derivation is sound". Skipping belongs in the
# `just` recipe's check ledger, which is a different claim from "it passed".
#
# Usage: ./tests/cmake-message-bounds-tests.sh
# Exit:  0 all assertions held; 1 otherwise.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# shellcheck source=lib/common.sh
source "$SCRIPT_DIR/lib/common.sh"

MODULE="$PROJECT_ROOT/cmake/NanoRosMessageBounds.cmake"

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

init_test_tmpdir "nros-message-bounds"
trap 'cleanup_test_tmpdir' EXIT

# ---------------------------------------------------------------------------
# Fragment fixtures. Byte-for-byte the shape `rosidl_codegen::bounds`'s
# `BoundInventory::to_cmake` emits (see its `to_cmake` and the golden tests
# beside it) -- if that emitter changes shape, these stop matching and this
# test is where it is noticed.
# ---------------------------------------------------------------------------
frag() {
    # frag <path> <package> ; then rows on stdin as: <type>|<state>|<tx>|<rx>|<reason>
    local path="$1" pkg="$2"
    {
        echo "set(NROS_MESSAGE_BOUNDS_SCHEMA_VERSION 1)"
        echo "list(APPEND NROS_MESSAGE_BOUND_PACKAGES \"$pkg\")"
        while IFS='|' read -r t state tx rx reason; do
            [ -z "$t" ] && continue
            local key
            key="$(echo "$t" | sed 's/[^A-Za-z0-9]/_/g')"
            echo "list(APPEND NROS_MESSAGE_BOUND_TYPES \"$t\")"
            echo "set(NROS_MESSAGE_BOUND_${key}_STATE \"$state\")"
            if [ "$state" = "bounded" ]; then
                echo "set(NROS_MESSAGE_BOUND_${key}_TX $tx)"
                echo "set(NROS_MESSAGE_BOUND_${key}_RX $rx)"
            else
                echo "set(NROS_MESSAGE_BOUND_${key}_REASON \"$reason\")"
            fi
        done
        echo "list(REMOVE_DUPLICATES NROS_MESSAGE_BOUND_PACKAGES)"
        echo "list(REMOVE_DUPLICATES NROS_MESSAGE_BOUND_TYPES)"
    } > "$path"
}

# Run the derivation. $1 = ";"-joined fragment list, rest = extra -D args.
# Captures stdout+stderr; the caller greps it.
derive() {
    local frags="$1"; shift
    cmake -DNROS_BOUNDS_FRAGMENTS="$frags" "$@" -P "$MODULE" 2>&1
}

T="$TEST_TMPDIR"

# ---------------------------------------------------------------------------
# A + B + G. The reference image's shape: a bounded closure over two packages,
# every type under the 2048 B split.
#
# The numbers are the island's own derived bounds (phase-403 W6/W7 measured
# them against /opt/ros/humble): Control 114, VelocityReport 108, Odometry 880,
# the rest 21-27. The hand-set answers for this closure were
# MAX_LARGE_SUBSCRIBERS=2 and SUBSCRIBER_LARGE_SIZE=2560, read off C++ headers
# that state an ESTIMATE; the derived answers are 0 and "not derived".
# ---------------------------------------------------------------------------
log_header "A/B/G -- a fully bounded closure derives, and answers ZERO large"
frag "$T/a1.cmake" "autoware_control_msgs" <<'ROWS'
autoware_control_msgs/msg/Control|bounded|78|114|
ROWS
frag "$T/a2.cmake" "nav_msgs" <<'ROWS'
nav_msgs/msg/Odometry|bounded|836|880|
autoware_vehicle_msgs/msg/VelocityReport|bounded|96|108|
autoware_planning_msgs/msg/RouteState|bounded|13|21|
ROWS
OUT="$(derive "$T/a1.cmake;$T/a2.cmake")"

check
if ! grep -q "NROS_MESSAGE_BOUNDS_STATUS=derived" <<< "$OUT"; then
    fail "A: a fully bounded closure did not derive:"; echo "$OUT"
fi
check
if ! grep -q "NROS_DERIVED_SUBSCRIPTION_BUFFER_SIZE=880" <<< "$OUT"; then
    fail "A: take buffer is not the largest bound in the closure (880):"; echo "$OUT"
fi
check
if ! grep -q "NROS_DERIVED_SUBSCRIBER_BUFFER_SIZE=880" <<< "$OUT"; then
    fail "A: small class is not the largest bound under the split (880):"; echo "$OUT"
fi
check
if ! grep -q "NROS_DERIVED_MAX_LARGE_SUBSCRIBERS=0" <<< "$OUT"; then
    fail "G: no type is over the split, so the large count must be 0:"; echo "$OUT"
fi
check
if grep -q "NROS_DERIVED_SUBSCRIBER_LARGE_SIZE=" <<< "$OUT"; then
    fail "G: the large SIZE must not be derived when the count is 0 (a size for a class that does not exist is an invented number):"; echo "$OUT"
fi
check
# B is the composition claim: the answer came from a1's package AND a2's. A
# reader that kept only the last fragment would say 880 too, so assert the
# type COUNT, which only composition can reach.
if ! grep -q "NROS_MESSAGE_BOUNDS_TYPE_COUNT=4" <<< "$OUT"; then
    fail "B: fragments did not compose -- expected 4 types across 2 packages:"; echo "$OUT"
fi

# ---------------------------------------------------------------------------
# A (large half). Add one type above the split and both large knobs appear.
# ---------------------------------------------------------------------------
log_header "A -- a type over the split drives the large class"
frag "$T/b1.cmake" "sensor_msgs" <<'ROWS'
sensor_msgs/msg/JointState|bounded|4204|4208|
sensor_msgs/msg/Imu|bounded|300|320|
sensor_msgs/msg/LaserScan|bounded|2500|2600|
ROWS
OUT="$(derive "$T/a2.cmake;$T/b1.cmake")"
check
if ! grep -q "NROS_DERIVED_MAX_LARGE_SUBSCRIBERS=2" <<< "$OUT"; then
    fail "A: two types exceed 2048, so the large count must be 2:"; echo "$OUT"
fi
check
if ! grep -q "NROS_DERIVED_SUBSCRIBER_LARGE_SIZE=4208" <<< "$OUT"; then
    fail "A: the large class must hold the largest type above the split (4208):"; echo "$OUT"
fi
check
if ! grep -q "NROS_DERIVED_SUBSCRIBER_BUFFER_SIZE=880" <<< "$OUT"; then
    fail "A: the small class must ignore the types routed large (880):"; echo "$OUT"
fi
check
if ! grep -q "NROS_DERIVED_SUBSCRIPTION_BUFFER_SIZE=4208" <<< "$OUT"; then
    fail "A: the take buffer is one global size and must hold the LARGEST type (4208):"; echo "$OUT"
fi

# The split is a POLICY input, so moving it must move the classification. A
# reader that hardcoded 2048 passes every assertion above.
log_header "A -- the class split is an input, not a constant"
OUT="$(derive "$T/a2.cmake;$T/b1.cmake" -DNROS_BOUNDS_CEILING=512)"
check
if ! grep -q "NROS_DERIVED_MAX_LARGE_SUBSCRIBERS=3" <<< "$OUT"; then
    fail "A: at a 512 B split, three types are large:"; echo "$OUT"
fi
check
if ! grep -q "NROS_DERIVED_SUBSCRIBER_BUFFER_SIZE=320" <<< "$OUT"; then
    fail "A: at a 512 B split, the small class is the largest bound <= 512 (320):"; echo "$OUT"
fi

# ---------------------------------------------------------------------------
# C. One unbounded type refuses the WHOLE derivation.
# ---------------------------------------------------------------------------
log_header "C -- one unbounded type refuses everything, and says which"
frag "$T/c1.cmake" "std_msgs" <<'ROWS'
std_msgs/msg/Header|unbounded|||unbounded member: frame_id (string)
ROWS
OUT="$(derive "$T/a2.cmake;$T/c1.cmake")"
check
if ! grep -q "NROS_MESSAGE_BOUNDS_STATUS=refused" <<< "$OUT"; then
    fail "C: an unbounded type in the closure must refuse the derivation:"; echo "$OUT"
fi
for v in NROS_DERIVED_SUBSCRIBER_BUFFER_SIZE NROS_DERIVED_SUBSCRIBER_LARGE_SIZE \
         NROS_DERIVED_MAX_LARGE_SUBSCRIBERS NROS_DERIVED_SUBSCRIPTION_BUFFER_SIZE; do
    check
    if grep -q "^-- $v=" <<< "$OUT"; then
        fail "C: $v was published despite an unbounded type -- deriving over the bounded subset publishes a maximum a real sample can exceed:"; echo "$OUT"
    fi
done
check
if ! grep -q "std_msgs/msg/Header" <<< "$OUT"; then
    fail "C: the refusal must NAME the type:"; echo "$OUT"
fi
check
if ! grep -q "frame_id" <<< "$OUT"; then
    fail "C: the refusal must name the MEMBER that costs the bound:"; echo "$OUT"
fi
check
if ! grep -qi "nros-codegen.toml" <<< "$OUT"; then
    fail "C: the refusal must name a remedy the user can act on:"; echo "$OUT"
fi
# Negative control: the same closure with that type bounded DOES derive. A
# refusal that fires unconditionally would pass every assertion above.
frag "$T/c2.cmake" "std_msgs" <<'ROWS'
std_msgs/msg/Header|bounded|84|92|
ROWS
OUT="$(derive "$T/a2.cmake;$T/c2.cmake")"
check
if ! grep -q "NROS_MESSAGE_BOUNDS_STATUS=derived" <<< "$OUT"; then
    fail "C control: bounding the offending type must let the derivation through:"; echo "$OUT"
fi

# ---------------------------------------------------------------------------
# D. `unresolved` is a different fact and licenses the same refusal.
# ---------------------------------------------------------------------------
log_header "D -- an unresolved type refuses too"
frag "$T/d1.cmake" "some_pkg" <<'ROWS'
some_pkg/msg/Thing|unresolved|||nested type `other_pkg/Widget` could not be resolved
ROWS
OUT="$(derive "$T/a2.cmake;$T/d1.cmake")"
check
if ! grep -q "NROS_MESSAGE_BOUNDS_STATUS=refused" <<< "$OUT"; then
    fail "D: an unresolved type must refuse the derivation:"; echo "$OUT"
fi
check
if ! grep -q "could not be resolved" <<< "$OUT"; then
    fail "D: the refusal must carry the unresolved reason verbatim:"; echo "$OUT"
fi

# ---------------------------------------------------------------------------
# E. Schema version. Both shapes.
# ---------------------------------------------------------------------------
log_header "E -- an unknown schema version is refused, not read anyway"
sed 's/SCHEMA_VERSION 1/SCHEMA_VERSION 99/' "$T/a1.cmake" > "$T/e1.cmake"
OUT="$(derive "$T/e1.cmake")"
check
if ! grep -q "CMake Error" <<< "$OUT"; then
    fail "E: schema version 99 must be a FATAL_ERROR:"; echo "$OUT"
fi
check
# `version 99` and not `schema version 99`: cmake hard-wraps a message() body,
# so an assertion on a phrase that spans the wrap point tests the wrap.
if ! grep -q "version 99" <<< "$OUT"; then
    fail "E: the error must state the version it found:"; echo "$OUT"
fi

grep -v "SCHEMA_VERSION" "$T/a1.cmake" > "$T/e2.cmake"
OUT="$(derive "$T/e2.cmake")"
check
if ! grep -q "CMake Error" <<< "$OUT"; then
    fail "E: a fragment stating NO schema version must be a FATAL_ERROR -- it is indistinguishable from a file that is not a fragment at all:"; echo "$OUT"
fi

# A MIXED-version set must fail on the bad one even though a good one precedes
# it. A reader that checked only the first fragment would pass E above.
cat "$T/a1.cmake" > "$T/e3.cmake"
OUT="$(derive "$T/e3.cmake;$T/e1.cmake")"
check
if ! grep -q "CMake Error" <<< "$OUT"; then
    fail "E: the version is checked PER fragment, not once:"; echo "$OUT"
fi

# ---------------------------------------------------------------------------
# F. A fragment that does not exist yet refuses; it does not fatal.
# ---------------------------------------------------------------------------
log_header "F -- a not-yet-written fragment refuses rather than fataling"
OUT="$(derive "$T/a1.cmake;$T/does-not-exist.cmake")"
check
if grep -q "CMake Error" <<< "$OUT"; then
    fail "F: a build-time fragment that has not been written must not break the configure -- the canonical lane emits it as a custom-command output:"; echo "$OUT"
fi
check
if ! grep -q "NROS_MESSAGE_BOUNDS_STATUS=refused" <<< "$OUT"; then
    fail "F: a missing fragment must refuse:"; echo "$OUT"
fi

# ---------------------------------------------------------------------------
# H. The output file: the answer, the provenance, and write-if-changed.
# ---------------------------------------------------------------------------
log_header "H -- the written answer carries provenance and is write-if-changed"
KNOBS="$T/knobs.cmake"
derive "$T/a1.cmake;$T/a2.cmake" -DNROS_BOUNDS_OUTPUT="$KNOBS" >/dev/null
check
if [ ! -f "$KNOBS" ]; then
    fail "H: OUTPUT_FILE was not written"
else
    check
    if ! grep -q "set(NROS_DERIVED_SUBSCRIPTION_BUFFER_SIZE 880)" "$KNOBS"; then
        fail "H: the written file must carry the derived value:"; cat "$KNOBS"
    fi
    check
    if ! grep -q "nav_msgs/msg/Odometry" "$KNOBS"; then
        fail "H: the written file must name the type the number came from -- a number a reader cannot account for is a number they will 'fix':"; cat "$KNOBS"
    fi
    check
    if ! grep -qi "UPPER BOUND" "$KNOBS"; then
        fail "H: the written file must state the over-approximation where a user reads it, not only in a report:"; cat "$KNOBS"
    fi
    check
    if ! grep -qi "DEFAULT" "$KNOBS"; then
        fail "H: the written file must say these are defaults a stated value overrides:"; cat "$KNOBS"
    fi
    BEFORE="$(stat -c '%Y.%y' "$KNOBS")"
    sleep 1
    derive "$T/a1.cmake;$T/a2.cmake" -DNROS_BOUNDS_OUTPUT="$KNOBS" >/dev/null
    AFTER="$(stat -c '%Y.%y' "$KNOBS")"
    check
    if [ "$BEFORE" != "$AFTER" ]; then
        fail "H: identical content rewrote the file -- a consumer registers it with CMAKE_CONFIGURE_DEPENDS, so this re-arms a reconfigure forever"
    fi
    # And a CHANGED answer must actually land, or the write-if-changed guard
    # would be indistinguishable from never writing.
    derive "$T/a2.cmake;$T/b1.cmake" -DNROS_BOUNDS_OUTPUT="$KNOBS" >/dev/null
    check
    if ! grep -q "set(NROS_DERIVED_SUBSCRIBER_LARGE_SIZE 4208)" "$KNOBS"; then
        fail "H: a changed answer must be written:"; cat "$KNOBS"
    fi
fi

# The refused file must be readable too: a consumer that includes it gets a
# status and a reason and NO numbers.
derive "$T/a2.cmake;$T/c1.cmake" -DNROS_BOUNDS_OUTPUT="$T/refused.cmake" >/dev/null 2>&1
check
if ! grep -q "set(NROS_MESSAGE_BOUNDS_STATUS \"refused\")" "$T/refused.cmake"; then
    fail "H: a refusal must still write a readable status:"; cat "$T/refused.cmake"
fi
check
if grep -q "set(NROS_DERIVED_" "$T/refused.cmake"; then
    fail "H: a refusal must publish no derived value at all:"; cat "$T/refused.cmake"
fi

# ---------------------------------------------------------------------------
# I. The consumer seam, in a REAL configure.
#
# The Zephyr knob resolver reads the composed answer through
# `CMAKE_CONFIGURE_DEPENDS`, and that is the one part of the wiring with a
# failure mode `cmake -P` cannot show: a ninja input that does not exist and
# has no rule producing it is `missing and no known rule to make it`, raised at
# LOAD before any rule runs, so the whole build dir is unusable. The Zephyr lane
# is not buildable on a bare host, but this seam is generic cmake+ninja and is.
#
# Asserts BOTH halves: the seeded placeholder makes the first configure and
# build work, and rewriting the file with a different answer actually re-runs
# cmake so the new value is picked up.
# ---------------------------------------------------------------------------
log_header "I -- the CONFIGURE_DEPENDS seam survives a real configure + build"
if ! command -v ninja >/dev/null 2>&1; then
    fail "I: ninja is not on PATH -- this assertion cannot report a verdict without it"
else
    P="$T/proj"
    mkdir -p "$P"
    cat > "$P/CMakeLists.txt" <<CMAKEEOF
cmake_minimum_required(VERSION 3.20)
project(nros_bounds_seam NONE)
include("$MODULE")
nros_message_bounds_knobs_file(_knobs)
nros_message_bounds_seed_knobs_file("\${_knobs}")
set_property(DIRECTORY APPEND PROPERTY CMAKE_CONFIGURE_DEPENDS "\${_knobs}")
include("\${_knobs}")
if(NOT DEFINED NROS_MESSAGE_BOUNDS_STATUS)
    message(FATAL_ERROR "the knobs file set no status")
endif()
file(WRITE "\${CMAKE_BINARY_DIR}/observed.txt"
     "\${NROS_MESSAGE_BOUNDS_STATUS}|\${NROS_DERIVED_SUBSCRIPTION_BUFFER_SIZE}")
add_custom_target(seam ALL COMMAND \${CMAKE_COMMAND} -E true)
CMAKEEOF
    B="$T/proj-build"
    check
    if ! cmake -G Ninja -S "$P" -B "$B" > "$T/i-configure.log" 2>&1; then
        fail "I: first configure failed:"; cat "$T/i-configure.log"
    elif ! ninja -C "$B" > "$T/i-build.log" 2>&1; then
        fail "I: first build failed -- a seeded-but-unproduced CONFIGURE_DEPENDS input is a ninja load error:"; cat "$T/i-build.log"
    fi
    check
    if [ "$(cat "$B/observed.txt" 2>/dev/null)" != "refused|" ]; then
        fail "I: the placeholder must read as refused with no derived value, got: $(cat "$B/observed.txt" 2>/dev/null)"
    fi
    # Now write a real answer into it, exactly as nros_find_interfaces() would,
    # and confirm the next bare `ninja` re-runs cmake and observes it.
    derive "$T/a1.cmake;$T/a2.cmake" -DNROS_BOUNDS_OUTPUT="$B/nros/message_bound_knobs.cmake" >/dev/null
    check
    if ! ninja -C "$B" > "$T/i-build2.log" 2>&1; then
        fail "I: the rebuild after the answer changed failed:"; cat "$T/i-build2.log"
    fi
    check
    if [ "$(cat "$B/observed.txt" 2>/dev/null)" != "derived|880" ]; then
        fail "I: a bare ninja did not re-configure and pick the derived answer up, got: $(cat "$B/observed.txt" 2>/dev/null)"
    fi
fi

# ---------------------------------------------------------------------------
# J. The REGISTRY path -- the one `nros_find_interfaces()` actually takes.
#
# The generators register each fragment as they emit it and the composer is
# then called with NO `FRAGMENTS`, so it reads the registry. Everything above
# passes fragments explicitly and would keep passing if the registry were
# broken. Exercised across an `add_subdirectory()` boundary because that is the
# shape a real workspace has (an interfaces package, then the entry) and it is
# what rules out a normal variable as the carrier.
# ---------------------------------------------------------------------------
log_header "K -- a first include from INSIDE a function frame is survivable"
if ! command -v ninja >/dev/null 2>&1; then
    fail "K: ninja is not on PATH -- this assertion cannot report a verdict without it"
else
    # `include_guard(GLOBAL)` plus an include that happens first from inside a
    # function frame is a live shape in this tree (NanoRosWorkspace.cmake
    # includes NanoRosCodegenCore.cmake inside `nros_resolve_cli`'s branch). A
    # plain file-scope `set()` would land in that frame, vanish when it pops,
    # and never come back, because the guard makes every later include a no-op.
    # The functions survive; only the variables go, so the failure lands on the
    # schema check of a perfectly well-formed fragment.
    K="$T/kfn"
    mkdir -p "$K"
    cat > "$K/CMakeLists.txt" <<CMAKEEOF
cmake_minimum_required(VERSION 3.20)
project(nros_bounds_frame NONE)
function(first_include_from_a_frame)
    include("$MODULE")
endfunction()
first_include_from_a_frame()
include("$MODULE")   # a no-op: the guard has already fired
nros_derive_message_bound_knobs(FRAGMENTS "$T/a1.cmake" "$T/a2.cmake" QUIET)
file(WRITE "\${CMAKE_BINARY_DIR}/observed.txt"
     "\${NROS_MESSAGE_BOUNDS_STATUS}|\${NROS_DERIVED_SUBSCRIPTION_BUFFER_SIZE}")
CMAKEEOF
    KB="$T/kfn-build"
    check
    if ! cmake -G Ninja -S "$K" -B "$KB" > "$T/k.log" 2>&1; then
        fail "K: configure failed -- the module's constants did not survive a first include from a function frame:"; cat "$T/k.log"
    fi
    check
    if [ "$(cat "$KB/observed.txt" 2>/dev/null)" != "derived|880" ]; then
        fail "K: expected derived|880, got: $(cat "$KB/observed.txt" 2>/dev/null)"
    fi
fi

log_header "J -- the composer reads the fragment registry, across a subdirectory"
if ! command -v ninja >/dev/null 2>&1; then
    fail "J: ninja is not on PATH -- this assertion cannot report a verdict without it"
else
    R="$T/reg"
    mkdir -p "$R/sub"
    cat > "$R/sub/CMakeLists.txt" <<CMAKEEOF
nros_message_bounds_register_fragment("$T/a1.cmake")
nros_message_bounds_register_fragment("$T/a2.cmake")
CMAKEEOF
    cat > "$R/CMakeLists.txt" <<CMAKEEOF
cmake_minimum_required(VERSION 3.20)
project(nros_bounds_registry NONE)
include("$MODULE")
add_subdirectory(sub)
# Registering the SAME fragment twice must not double-count it -- a package
# reached through two dependency paths is the normal case.
nros_message_bounds_register_fragment("$T/a1.cmake")
nros_derive_message_bound_knobs(OUTPUT_FILE "\${CMAKE_BINARY_DIR}/knobs.cmake")
file(WRITE "\${CMAKE_BINARY_DIR}/observed.txt"
     "\${NROS_MESSAGE_BOUNDS_STATUS}|\${NROS_MESSAGE_BOUNDS_TYPE_COUNT}|\${NROS_DERIVED_SUBSCRIPTION_BUFFER_SIZE}")
# A SECOND call, this time over a closure that refuses, from the same caller
# scope. Every derived value must be gone in both the returned variables and
# the written file -- a function inherits its caller's variables, so a stale
# one survives unless it is cleared locally as well as in the parent, and the
# result reads as a refusal carrying the previous run's numbers.
nros_derive_message_bound_knobs(
    FRAGMENTS "$T/a1.cmake" "$T/c1.cmake"
    OUTPUT_FILE "\${CMAKE_BINARY_DIR}/knobs2.cmake" QUIET)
file(WRITE "\${CMAKE_BINARY_DIR}/observed2.txt"
     "\${NROS_MESSAGE_BOUNDS_STATUS}|\${NROS_DERIVED_SUBSCRIPTION_BUFFER_SIZE}")
CMAKEEOF
    RB="$T/reg-build"
    check
    if ! cmake -G Ninja -S "$R" -B "$RB" > "$T/j.log" 2>&1; then
        fail "J: configure failed:"; cat "$T/j.log"
    fi
    check
    if [ "$(cat "$RB/observed.txt" 2>/dev/null)" != "derived|4|880" ]; then
        fail "J: the registry did not carry the fragments across the subdirectory (expected derived|4|880), got: $(cat "$RB/observed.txt" 2>/dev/null)"
    fi
    check
    if [ "$(cat "$RB/observed2.txt" 2>/dev/null)" != "refused|" ]; then
        fail "J: a refusal after a successful call left the previous derived value standing, got: $(cat "$RB/observed2.txt" 2>/dev/null)"
    fi
    check
    if grep -q "set(NROS_DERIVED_" "$RB/knobs2.cmake"; then
        fail "J: the refusal file carries a value from the previous call:"; cat "$RB/knobs2.cmake"
    fi
fi

# ---------------------------------------------------------------------------
log_header "Summary"
if [ "$FAILURES" -eq 0 ]; then
    log_success "$CHECKS assertions held"
    exit 0
fi
log_error "$FAILURES of $CHECKS assertions failed"
exit 1
