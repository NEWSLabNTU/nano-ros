#!/bin/bash
# tests/cmake-entity-inventory-tests.sh -- phase-403 W9 (issue 0965)
#
# Exercises the READER for the entity inventory: `cmake/NanoRosEntityInventory.
# cmake`, driven in `cmake -P` script mode. Sibling of
# `tests/cmake-message-bounds-tests.sh`, and shaped like it.
#
# THE CLI IS STUBBED, DELIBERATELY. The derivation itself -- the counting rule,
# the per-kind slot cost, the refusal -- lives in
# `nros_cli_core::entity_inventory` and is unit-tested there, including the
# island's 33-entities-19-slots case. What CMAKE owns is a different set of
# facts, and every one of them is a failure mode that has bitten this tree
# before: does a refusal publish NO number, does an unknown schema FATAL instead
# of being read field-by-field, does a missing input refuse rather than break a
# clean-tree configure, and does the answer reach the CALLER's scope at all.
# A stub that emits fragments lets each of those be a case; requiring a built
# `nros` would make this gate need a cargo build, which is exactly what
# `check-lane-contracts` forbids on an affordability lane.
#
# WHAT IS ASSERTED:
#
#   A. A DERIVED fragment reaches the caller's scope: status, component count,
#      entity total, per-kind counts and NROS_DERIVED_EXECUTOR_MAX_CBS. The
#      scope half is the one that reads as working while being broken --
#      `include()` inside a function keeps its `set()`s in that frame, and
#      publishing to one of the two scopes is the bug `_nros_bounds_publish`
#      was written to prevent.
#
#   B. A REFUSED fragment publishes the reason and NO number. This is the
#      wave's central requirement: a consumer either reads a value the
#      inventory derived or reads nothing, because a slot count composed over
#      part of an image is SMALLER than the image needs and a short MAX_CBS
#      fails entity creation at boot.
#
#   C. A REFUSAL AFTER A DERIVATION leaves no stale number standing. The
#      function is called twice in one script; the second call must clear what
#      the first published, in both scopes.
#
#   D. An unknown SCHEMA VERSION is a FATAL_ERROR, and so is a fragment that
#      states none. Reading fields that may have moved is how two green tools
#      come to disagree.
#
#   E. A MISSING metadata file refuses rather than fataling -- an image that
#      has registered no component is the state every build was in before this
#      wave, and it is the permanent state of a launch-only image.
#
#   F. A MISSING CLI refuses rather than fataling, for the same reason.
#
#   G. A CLI that EXITS NON-ZERO is fatal. That is a broken declaration -- an
#      unknown entity kind, a component claiming NONE beside real entities --
#      and it names a component the user can fix. Distinguishing it from E/F is
#      the whole point: "you have not declared" and "what you declared is
#      wrong" license different actions.
#
#   A2. The JOIN KEY crosses the same boundary (phase-403 step 1). The
#      subscribed-type set and the wider received set are what
#      `nros_derive_message_bound_knobs` narrows the payload classes with, so
#      an absent list is not "nothing published" -- it reads as "this image
#      receives nothing" and derives a class over an empty set.
#
#   A3. The DECLARED DEPTHS cross the same boundary (phase-403 step 2), and so
#      does the count of endpoints that declared NONE. Depth multiplies the
#      type bound, so an absent list read as "every endpoint is depth 0" sizes
#      an arena an order of magnitude short; and a list read WITHOUT its
#      undeclared count sizes an image from the subset of it that happened to
#      be annotated. Asserted in A (published), B (refused, so absent) and C
#      (cleared by a later refusal).
#
#   H. SEEDING. A refusal writes a placeholder fragment, because the consumer
#      registers the path with CMAKE_CONFIGURE_DEPENDS and a ninja input with
#      no producing rule is `missing and no known rule to make it` at LOAD,
#      before any rule runs. And seeding never overwrites an existing answer.
#
# PRECONDITIONS ARE HARD FAILURES. This script never prints-and-returns: a
# green here is read as "the reader is sound". Skipping belongs in the `just`
# recipe's check ledger, which is a different claim from "it passed".
#
# Usage: ./tests/cmake-entity-inventory-tests.sh
# Exit:  0 all assertions held; 1 otherwise.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# shellcheck source=lib/common.sh
source "$SCRIPT_DIR/lib/common.sh"

MODULE="$PROJECT_ROOT/cmake/NanoRosEntityInventory.cmake"

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

init_test_tmpdir "nros-entity-inventory"
trap 'cleanup_test_tmpdir' EXIT

# ---------------------------------------------------------------------------
# A stub `nros`. It writes the fragment named by --output-cmake, copying a body
# this script supplies through NROS_STUB_BODY, and exits with NROS_STUB_RC.
#
# The bodies below are byte-for-byte the shape
# `nros_cli_core::entity_inventory::EntityInventory::to_cmake` emits (see its
# unit tests) -- if that emitter changes shape, these stop matching and this
# test is where it is noticed.
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

DERIVED_BODY="$TEST_TMPDIR/derived.cmake"
cat > "$DERIVED_BODY" <<'EOF'
set(NROS_ENTITY_INVENTORY_SCHEMA_VERSION 5)
# No NROS_ENTITY_INVENTORY_SOURCE: `to_cmake` stopped emitting it (issue 1228 --
# it is composer-dependent content in a file whose bytes decide whether cmake
# runs again), and this fixture mirrors what the producer writes.
set(NROS_ENTITY_INVENTORY_STATUS "derived")
set(NROS_ENTITY_INVENTORY_COMPONENT_COUNT 4)
set(NROS_ENTITY_INVENTORY_ENTITY_TOTAL 33)
set(NROS_ENTITY_COUNT_PUBLISHER 14)
set(NROS_ENTITY_COUNT_SUBSCRIPTION 11)
set(NROS_ENTITY_COUNT_TIMER 4)
set(NROS_ENTITY_COUNT_SERVICE_SERVER 2)
set(NROS_ENTITY_COUNT_SERVICE_CLIENT 2)
set(NROS_DERIVED_EXECUTOR_MAX_CBS 19)
# phase-412 W2 / issue 1130 -- the liveliness pool and the per-kind cell
# registry bound. The cell bound is deliberately ZERO here: an image whose
# components create none of the five cell kinds derives 0, and the registries
# are Rust arrays where 0 is an empty registry rather than a `#error`. A
# publish that treats 0 as "nothing to publish" drops it silently and the
# consumer compiles the builtin 8 instead.
set(NROS_DERIVED_MAX_LIVELINESS 15)
set(NROS_DERIVED_RUNTIME_MAX_CELL_ENTITIES 0)
set(NROS_ENTITY_SUBSCRIBED_TYPES_STATUS "resolved")
set(NROS_ENTITY_SUBSCRIBED_TYPES "nav_msgs/msg/Odometry;std_msgs/msg/Int32")
set(NROS_ENTITY_SUBSCRIBED_TYPE_COUNTS "nav_msgs/msg/Odometry=1;std_msgs/msg/Int32=2")
set(NROS_ENTITY_SUBSCRIBED_ENTITY_COUNT 3)
set(NROS_ENTITY_RECEIVED_TYPES_STATUS "resolved")
set(NROS_ENTITY_RECEIVED_TYPES "demo/srv/Op_Request;nav_msgs/msg/Odometry;std_msgs/msg/Int32")
set(NROS_ENTITY_RECEIVED_TYPE_COUNTS "demo/srv/Op_Request=1;nav_msgs/msg/Odometry=1;std_msgs/msg/Int32=2")
set(NROS_ENTITY_RECEIVED_ENTITY_COUNT 4)
set(NROS_ENTITY_DECLARED_DEPTH_STATUS "resolved")
set(NROS_ENTITY_DECLARED_DEPTHS "nav_msgs/msg/Odometry|/localization/kinematic_state=1;std_msgs/msg/Int32|/chatter=10")
set(NROS_ENTITY_DECLARED_DEPTH_COUNT 2)
set(NROS_ENTITY_UNDECLARED_DEPTH_COUNT 3)
set(NROS_ENTITY_UNDECLARED_DEPTH_COUNT_SUBSCRIPTION 0)
set(NROS_ENTITY_DECLARED_DEPTHS_PUBLISHER "std_msgs/msg/Int32|/chatter=8")
set(NROS_ENTITY_DECLARED_DEPTH_COUNT_PUBLISHER 1)
set(NROS_ENTITY_UNDECLARED_DEPTH_COUNT_PUBLISHER 3)
# phase-454 W3 (issue 1256) -- the other three QoS policies. `history` is
# deliberately resolved here WITH a keep_last on the subscription side: the
# depth table above is what `keep_all` refuses, and this block keeps resolving
# either way (RFC-0100 D6 -- refusal is per fact, never global).
set(NROS_ENTITY_DECLARED_QOS_STATUS "resolved")
set(NROS_ENTITY_DECLARED_RELIABILITY "std_msgs/msg/Int32|/chatter=best_effort")
set(NROS_ENTITY_DECLARED_RELIABILITY_PUBLISHER "std_msgs/msg/Int32|/chatter=reliable")
set(NROS_ENTITY_UNDECLARED_RELIABILITY_COUNT_SUBSCRIPTION 10)
set(NROS_ENTITY_UNDECLARED_RELIABILITY_COUNT_PUBLISHER 13)
set(NROS_ENTITY_DECLARED_DURABILITY "")
set(NROS_ENTITY_DECLARED_DURABILITY_PUBLISHER "std_msgs/msg/Int32|/chatter=transient_local")
set(NROS_ENTITY_UNDECLARED_DURABILITY_COUNT_SUBSCRIPTION 11)
set(NROS_ENTITY_UNDECLARED_DURABILITY_COUNT_PUBLISHER 13)
set(NROS_ENTITY_DECLARED_HISTORY "std_msgs/msg/Int32|/chatter=keep_last")
set(NROS_ENTITY_DECLARED_HISTORY_PUBLISHER "")
set(NROS_ENTITY_UNDECLARED_HISTORY_COUNT_SUBSCRIPTION 10)
set(NROS_ENTITY_UNDECLARED_HISTORY_COUNT_PUBLISHER 14)
set(NROS_PARAM_DECLARATION_STATUS "declared")
set(NROS_PARAM_DECLARED_COUNT 21)
set(NROS_DERIVED_MAX_PARAMETERS 25)
set(NROS_DERIVED_MAX_PARAM_NAME_LEN 35)
set(NROS_DERIVED_MAX_STRING_VALUE_LEN 0)
set(NROS_DERIVED_MAX_ARRAY_LEN 0)
set(NROS_PARAM_NEEDS_MAX_BYTE_ARRAY_LEN "/system/diag_aggregator:blob:byte_array")
set(NROS_PARAM_SERVICE_SHAPE "8:170:1:17:0:0:0:0:0")
EOF

REFUSED_BODY="$TEST_TMPDIR/refused.cmake"
cat > "$REFUSED_BODY" <<'EOF'
set(NROS_ENTITY_INVENTORY_SCHEMA_VERSION 5)
set(NROS_ENTITY_INVENTORY_STATUS "refused")
set(NROS_ENTITY_INVENTORY_COMPONENT_COUNT 4)
set(NROS_ENTITY_INVENTORY_REASON "1 of 4 components in this image declare no entities:\n    demo::legacy (demo::Legacy)")
set(NROS_ENTITY_SUBSCRIBED_TYPES_STATUS "refused")
set(NROS_ENTITY_SUBSCRIBED_TYPES_REASON "the entity inventory itself did not compose")
set(NROS_ENTITY_RECEIVED_TYPES_STATUS "refused")
set(NROS_ENTITY_RECEIVED_TYPES_REASON "the entity inventory itself did not compose")
set(NROS_ENTITY_DECLARED_DEPTH_STATUS "refused")
set(NROS_ENTITY_DECLARED_DEPTH_REASON "the entity inventory itself did not compose")
set(NROS_ENTITY_DECLARED_QOS_STATUS "refused")
set(NROS_ENTITY_DECLARED_QOS_REASON "the entity inventory itself did not compose")
EOF

# A schema this reader does NOT understand. Written as "one past supported"
# rather than a literal, because the literal was `2` until phase-403 step 1
# made 2 the supported version -- at which point the case silently stopped
# testing anything it claimed to. Step 2 moved it to 3/4, phase-454 W2, which
# split the depth table by kind, to 4/5, and phase-454 W3, which added the
# other three QoS policies, to 5/6.
BAD_SCHEMA_BODY="$TEST_TMPDIR/bad-schema.cmake"
sed 's/SCHEMA_VERSION 5/SCHEMA_VERSION 6/' "$DERIVED_BODY" > "$BAD_SCHEMA_BODY"

NO_SCHEMA_BODY="$TEST_TMPDIR/no-schema.cmake"
grep -v SCHEMA_VERSION "$DERIVED_BODY" > "$NO_SCHEMA_BODY"

META="$TEST_TMPDIR/nros-metadata.json"
echo '{"components": []}' > "$META"

# Run the derivation through the module's `cmake -P` entry point.
#   derive <body-or-empty> <rc> <metadata> <output> [extra env]
derive() {
    local body="$1" rc="$2" meta="$3" out="$4" cli="${5:-$STUB}"
    NROS_STUB_BODY="$body" NROS_STUB_RC="$rc" \
        cmake -DNROS_ENTITY_CLI="$cli" \
              -DNROS_ENTITY_METADATA="$meta" \
              -DNROS_ENTITY_OUTPUT="$out" \
              -P "$MODULE" 2>&1
}

# ---------------------------------------------------------------------------
# A. A derived fragment reaches the caller's scope, with every field.
# ---------------------------------------------------------------------------
log_info "A. a derived inventory publishes its numbers"
OUT="$(derive "$DERIVED_BODY" 0 "$META" "$TEST_TMPDIR/a.cmake")"
check
if ! nros_grep_q "NROS_ENTITY_INVENTORY_STATUS=derived" <<<"$OUT"; then
    fail "A: status not derived -- $OUT"
fi
check
if ! nros_grep_q "NROS_DERIVED_EXECUTOR_MAX_CBS=19" <<<"$OUT"; then
    fail "A: MAX_CBS did not reach the caller's scope -- $OUT"
fi
# phase-412 W2 -- the liveliness pool crosses the same boundary. It sizes a
# fixed C array in `zpico.c`, and a value lost here leaves the image on the
# zpico literal 16 with no diagnostic: the entities are created and WORK, and
# are simply absent from `ros2 node list`.
check
if ! nros_grep_q "NROS_DERIVED_MAX_LIVELINESS=15" <<<"$OUT"; then
    fail "A: the liveliness pool did not reach the caller's scope -- $OUT"
fi
# Issue 1130 -- and so does the cell registry bound, AT ZERO. Zero is a derived
# ANSWER here (no component creates a publisher, service or action), not an
# absence, and it is the value most likely to be dropped by a publish that
# tests truthiness instead of DEFINED.
check
if ! nros_grep_q "NROS_DERIVED_RUNTIME_MAX_CELL_ENTITIES=0" <<<"$OUT"; then
    fail "A: a derived ZERO cell-registry bound did not reach the caller -- $OUT"
fi
check
if ! nros_grep_q "NROS_ENTITY_INVENTORY_ENTITY_TOTAL=33" <<<"$OUT"; then
    fail "A: the entity total did not reach the caller's scope -- $OUT"
fi
check
# The entity total and the slot demand are DIFFERENT numbers and both are
# published. Collapsing them is the island's 33-vs-19 error.
if ! nros_grep_q "NROS_ENTITY_COUNT_PUBLISHER=14" <<<"$OUT"; then
    fail "A: per-kind counts did not reach the caller's scope -- $OUT"
fi
check
if ! nros_grep_q "publishers, which claim no callback slot" <<<"$OUT"; then
    fail "A: the status line does not say publishers claim no slot -- $OUT"
fi
# phase-403 step 1. The JOIN KEY has to cross the same function boundary the
# counts do; it is the input `nros_derive_message_bound_knobs` narrows the
# payload classes with, and an absent list there reads as "receives nothing".
check
if ! nros_grep_q "NROS_ENTITY_SUBSCRIBED_TYPES_STATUS=resolved" <<<"$OUT"; then
    fail "A: the subscribed-type status did not reach the caller's scope -- $OUT"
fi
check
if ! nros_grep_q "NROS_ENTITY_SUBSCRIBED_TYPE_COUNTS=nav_msgs/msg/Odometry=1;std_msgs/msg/Int32=2" <<<"$OUT"; then
    fail "A: the per-type ENTITY counts did not reach the caller's scope -- $OUT"
fi
# The two views are different sets and both travel. Collapsing them would
# either price a service request against a pool it never allocates from, or
# leave the arena blind to four receiving kinds.
check
if ! nros_grep_q "NROS_ENTITY_RECEIVED_TYPES=demo/srv/Op_Request;" <<<"$OUT"; then
    fail "A: the wider RECEIVED set did not reach the caller's scope -- $OUT"
fi
# phase-403 step 2. Depth MULTIPLIES the type bound above, so the arena cannot
# be derived without it, and it has to cross the same boundary the rest does.
check
if ! nros_grep_q "NROS_ENTITY_DECLARED_DEPTHS=nav_msgs/msg/Odometry|/localization/kinematic_state=1;" <<<"$OUT"; then
    fail "A: the declared depths did not reach the caller's scope -- $OUT"
fi
# The one that is easy to leave out and expensive to leave out. An image where
# three endpoints stated no depth is not an image with three depth-0 endpoints,
# and a consumer that reads only the LIST would size it from the two that
# happened to be annotated.
check
if ! nros_grep_q "NROS_ENTITY_UNDECLARED_DEPTH_COUNT=3" <<<"$OUT"; then
    fail "A: the UNDECLARED depth count did not reach the caller's scope. \
Without it a consumer cannot tell a fully-declared image from a partly-declared \
one, which is the whole reason the count is published -- $OUT"
fi
# phase-454 W2. The PUBLISHER half crosses too, in its own names -- and the
# separation is the point: a publisher's depth must never be an element of the
# list a subscription term counts against its subscription count, because that
# equality IS `subs_arena`'s guard and breaking it drops every declaring image
# back to the worst case silently.
check
if ! nros_grep_q "NROS_ENTITY_DECLARED_DEPTHS_PUBLISHER=std_msgs/msg/Int32|/chatter=8" <<<"$OUT"; then
    fail "A: the declared PUBLISHER depths did not reach the caller's scope -- $OUT"
fi
check
if ! nros_grep_q "NROS_ENTITY_UNDECLARED_DEPTH_COUNT_PUBLISHER=3" <<<"$OUT"; then
    fail "A: the publisher-scoped UNDECLARED count did not reach the caller's \
scope. A publisher-side consumer must refuse on its OWN kind's silence, not on \
a count spanning kinds it does not price -- $OUT"
fi
check
if nros_grep_q "NROS_ENTITY_DECLARED_DEPTHS=.*=8" <<<"$OUT"; then
    fail "A: a publisher depth reached the SUBSCRIPTION sizing list -- $OUT"
fi

# phase-454 W3 (issue 1256). The other three policies cross the same boundary,
# in the same per-kind shape and with the same per-POLICY undeclared counts.
# Losing one at the function boundary is the drift this module's own comment
# lists four instances of ("the symbol loaded here, died at the function
# boundary, and the consumer fell back to a default that looked deliberate") --
# and here the default a consumer would fall back to is RELIABLE, which is two
# 64 KiB buffers per XRCE session that the image said it did not want.
check
if ! nros_grep_q "NROS_ENTITY_DECLARED_QOS_STATUS=resolved" <<<"$OUT"; then
    fail "A: the declared-QoS status did not reach the caller's scope -- $OUT"
fi
check
if ! nros_grep_q "NROS_ENTITY_DECLARED_RELIABILITY=std_msgs/msg/Int32|/chatter=best_effort" <<<"$OUT"; then
    fail "A: the declared reliability did not reach the caller's scope -- $OUT"
fi
check
if ! nros_grep_q "NROS_ENTITY_DECLARED_RELIABILITY_PUBLISHER=std_msgs/msg/Int32|/chatter=reliable" <<<"$OUT"; then
    fail "A: the PUBLISHER reliability did not reach the caller's scope -- $OUT"
fi
check
if ! nros_grep_q "NROS_ENTITY_DECLARED_DURABILITY_PUBLISHER=std_msgs/msg/Int32|/chatter=transient_local" <<<"$OUT"; then
    fail "A: the declared durability did not reach the caller's scope -- $OUT"
fi
check
if ! nros_grep_q "NROS_ENTITY_DECLARED_HISTORY=std_msgs/msg/Int32|/chatter=keep_last" <<<"$OUT"; then
    fail "A: the declared history did not reach the caller's scope -- $OUT"
fi
# An EMPTY list is a published fact and must survive the boundary too: it says
# "this image stated none of this policy", which is different from the variable
# being absent, and the count beside it is what quantifies the gap.
check
if ! nros_grep_q "NROS_ENTITY_DECLARED_DURABILITY=$" <<<"$OUT"; then
    fail "A: an EMPTY policy list did not reach the caller's scope. Absent and \
empty are different claims and only one of them is this image's -- $OUT"
fi
# Per POLICY, not one count for all three. An image can state reliability on
# every endpoint and durability on none; one number would pin the reliability
# consumer on its worst case for a gap that is not its own.
check
if ! nros_grep_q "NROS_ENTITY_UNDECLARED_RELIABILITY_COUNT_SUBSCRIPTION=10" <<<"$OUT"; then
    fail "A: the per-policy UNDECLARED count did not reach the caller's scope -- $OUT"
fi
check
if ! nros_grep_q "NROS_ENTITY_UNDECLARED_HISTORY_COUNT_PUBLISHER=14" <<<"$OUT"; then
    fail "A: the per-policy, per-kind UNDECLARED count did not reach the \
caller's scope -- $OUT"
fi
# And no policy value leaks into the DEPTH list, which is the list whose length
# is `subs_arena`'s guard.
check
if nros_grep_q "NROS_ENTITY_DECLARED_DEPTHS=.*best_effort" <<<"$OUT"; then
    fail "A: a QoS policy value reached the SUBSCRIPTION depth list -- $OUT"
fi

# phase-446 W4. The parameter store's numbers cross the same function boundary,
# and so does the NEEDS fact: a capacity a declared type uses carries the name
# of the parameter instead of a number, and nros-params' build script refuses
# on it. Losing it at the boundary would build the crate default silently.
check
if ! nros_grep_q "NROS_DERIVED_MAX_PARAMETERS=25" <<<"$OUT"; then
    fail "A: the derived parameter-store slot count did not reach the caller -- $OUT"
fi
check
if ! nros_grep_q "NROS_DERIVED_MAX_STRING_VALUE_LEN=0" <<<"$OUT"; then
    fail "A: a derived ZERO capacity did not reach the caller -- $OUT"
fi
check
if ! nros_grep_q "NROS_PARAM_NEEDS_MAX_BYTE_ARRAY_LEN=/system/diag_aggregator:blob:byte_array" <<<"$OUT"; then
    fail "A: the parameter that NEEDS a board capacity did not reach the caller -- $OUT"
fi
# phase-446 F3 -- the parameter services' shape crosses the same boundary;
# losing it would leave nros-node on its configured buffer silently.
check
if ! nros_grep_q "NROS_PARAM_SERVICE_SHAPE=8:170:1:17:0:0:0:0:0" <<<"$OUT"; then
    fail "A: the parameter services' declaration shape did not reach the caller -- $OUT"
fi

# ---------------------------------------------------------------------------
# B. A refusal publishes the reason and NO number.
# ---------------------------------------------------------------------------
log_info "B. a refusal publishes no number"
OUT="$(derive "$REFUSED_BODY" 0 "$META" "$TEST_TMPDIR/b.cmake")"
check
if nros_grep_q "NROS_DERIVED_MAX_PARAMETERS=" <<<"$OUT"; then
    fail "B: a fragment with no parameter declaration published a store size -- $OUT"
fi
check
if ! nros_grep_q "NROS_ENTITY_INVENTORY_STATUS=refused" <<<"$OUT"; then
    fail "B: status not refused -- $OUT"
fi
check
if nros_grep_q "NROS_DERIVED_EXECUTOR_MAX_CBS=" <<<"$OUT"; then
    fail "B: a refusal published a MAX_CBS -- $OUT"
fi
# phase-412 W2 / issue 1130 -- and neither of the two counts this wave added.
# Both size pools whose exhaustion is survivable-but-silent-ish, which makes a
# number published over a PARTIAL image worse than no number at all.
check
if nros_grep_q "NROS_DERIVED_MAX_LIVELINESS=" <<<"$OUT"; then
    fail "B: a refusal published a liveliness pool size -- $OUT"
fi
check
if nros_grep_q "NROS_DERIVED_RUNTIME_MAX_CELL_ENTITIES=" <<<"$OUT"; then
    fail "B: a refusal published a cell-registry bound -- $OUT"
fi
# phase-454 W3 -- and no policy list. A partial one reads as "these are the
# only endpoints that asked for best_effort", which is how an XRCE image stops
# paying for buffers endpoints nobody enumerated still need.
check
if nros_grep_q "NROS_ENTITY_DECLARED_RELIABILITY=" <<<"$OUT"; then
    fail "B: a refusal published a reliability list -- $OUT"
fi
check
if ! nros_grep_q "NROS_ENTITY_DECLARED_QOS_STATUS=refused" <<<"$OUT"; then
    fail "B: the declared-QoS refusal did not reach the caller -- $OUT"
fi
check
if ! nros_grep_q "declare no entities" <<<"$OUT"; then
    fail "B: the refusal reason did not reach the caller -- $OUT"
fi
# phase-403 step 2 -- and a refusal publishes NO depth list either. An absent
# list read as "every endpoint is depth 0" would size an arena an order of
# magnitude short, which is the same failure the absent TYPE list would cause
# one line up.
check
if nros_grep_q "NROS_ENTITY_DECLARED_DEPTHS=" <<<"$OUT"; then
    fail "B: a refusal published a depth list -- $OUT"
fi
check
if ! nros_grep_q "NROS_ENTITY_DECLARED_DEPTH_STATUS=refused" <<<"$OUT"; then
    fail "B: the depth view published no status, so a reader cannot tell \
\"refused\" from \"this fragment predates the field\" -- $OUT"
fi

# ---------------------------------------------------------------------------
# C. A refusal AFTER a derivation leaves no stale number standing.
# ---------------------------------------------------------------------------
log_info "C. a second, refusing call clears the first call's answer"
SEQ="$TEST_TMPDIR/seq.cmake"
cat > "$SEQ" <<EOF
include("$MODULE")
nros_derive_entity_inventory_knobs(CLI "$STUB" METADATA "$META"
    OUTPUT_FILE "$TEST_TMPDIR/c1.cmake" QUIET)
message(STATUS "first=\${NROS_DERIVED_EXECUTOR_MAX_CBS}")
set(ENV{NROS_STUB_BODY} "$REFUSED_BODY")
nros_derive_entity_inventory_knobs(CLI "$STUB" METADATA "$META"
    OUTPUT_FILE "$TEST_TMPDIR/c2.cmake" QUIET)
message(STATUS "second=[\${NROS_DERIVED_EXECUTOR_MAX_CBS}]")
message(STATUS "second_depths=[\${NROS_ENTITY_DECLARED_DEPTHS}]")
EOF
OUT="$(NROS_STUB_BODY="$DERIVED_BODY" cmake -P "$SEQ" 2>&1)"
check
if ! nros_grep_q "first=19" <<<"$OUT"; then
    fail "C: the first call did not publish -- $OUT"
fi
check
if ! nros_grep_q "second=\[\]" <<<"$OUT"; then
    fail "C: a refusal left the previous number standing -- $OUT"
fi
# phase-403 step 2 -- the depth list is cleared by the same rule. A stale depth
# table is worse than a stale count: it names topics that may no longer be
# subscribed, at depths the current declaration never stated.
check
if ! nros_grep_q "second_depths=\[\]" <<<"$OUT"; then
    fail "C: a refusal left the previous DEPTH LIST standing -- $OUT"
fi

# ---------------------------------------------------------------------------
# D. An unrecognised schema is FATAL, both shapes.
# ---------------------------------------------------------------------------
log_info "D. an unrecognised schema refuses to be read"
# cmake re-wraps a `message(FATAL_ERROR)` body at ~72 columns, so the phrases
# below are matched against the output with newlines squashed. Grepping the raw
# text passes only by accident of where the wrap lands.
flat() { tr '\n' ' ' | tr -s ' '; }
OUT="$(derive "$BAD_SCHEMA_BODY" 0 "$META" "$TEST_TMPDIR/d1.cmake" | flat)"
check
if ! nros_grep_q "states entity-inventory schema version 6" <<<"$OUT"; then
    fail "D: a future schema was read rather than refused -- $OUT"
fi
check
if ! nros_grep_q -qi "CMake Error" <<<"$OUT"; then
    fail "D: a future schema did not raise a FATAL_ERROR -- $OUT"
fi
OUT="$(derive "$NO_SCHEMA_BODY" 0 "$META" "$TEST_TMPDIR/d2.cmake" | flat)"
check
if ! nros_grep_q "sets no NROS_ENTITY_INVENTORY_SCHEMA_VERSION" <<<"$OUT"; then
    fail "D: a fragment with no schema was read rather than refused -- $OUT"
fi

# ---------------------------------------------------------------------------
# E. A missing metadata file refuses; it does not break the configure.
# ---------------------------------------------------------------------------
log_info "E. a missing metadata file refuses rather than fataling"
OUT="$(derive "$DERIVED_BODY" 0 "$TEST_TMPDIR/absent.json" "$TEST_TMPDIR/e.cmake")"
check
if nros_grep_q -qi "CMake Error" <<<"$OUT"; then
    fail "E: a missing metadata file was fatal -- $OUT"
fi
check
if ! nros_grep_q "NROS_ENTITY_INVENTORY_STATUS=refused" <<<"$OUT"; then
    fail "E: a missing metadata file did not refuse -- $OUT"
fi
check
if ! nros_grep_q "nothing in this image called nano_ros_node_register" <<<"$OUT"; then
    fail "E: the refusal does not say what is missing -- $OUT"
fi

# ---------------------------------------------------------------------------
# F. A missing CLI refuses too.
# ---------------------------------------------------------------------------
log_info "F. a missing CLI refuses rather than fataling"
OUT="$(derive "$DERIVED_BODY" 0 "$META" "$TEST_TMPDIR/f.cmake" "$TEST_TMPDIR/no-such-nros")"
check
if nros_grep_q -qi "CMake Error" <<<"$OUT"; then
    fail "F: a missing CLI was fatal -- $OUT"
fi
check
if ! nros_grep_q "NROS_ENTITY_INVENTORY_STATUS=refused" <<<"$OUT"; then
    fail "F: a missing CLI did not refuse -- $OUT"
fi

# ---------------------------------------------------------------------------
# G. A CLI that fails is FATAL -- a broken declaration, not an absent one.
# ---------------------------------------------------------------------------
log_info "G. a broken declaration is fatal and names the fix"
OUT="$(NROS_STUB_STDERR="component \`demo::n\` declares \`publsher\`" \
       derive "" 1 "$META" "$TEST_TMPDIR/g.cmake")"
check
if ! nros_grep_q "not readable" <<<"$OUT"; then
    fail "G: a failing CLI was not fatal -- $OUT"
fi
check
if ! nros_grep_q "publsher" <<<"$OUT"; then
    fail "G: the CLI's own message was swallowed -- $OUT"
fi

# ---------------------------------------------------------------------------
# H. Seeding: a refusal leaves a well-formed fragment, and never clobbers one.
# ---------------------------------------------------------------------------
log_info "H. a refusal seeds a fragment, and seeding never clobbers an answer"
SEED="$TEST_TMPDIR/h.cmake"
derive "$DERIVED_BODY" 0 "$TEST_TMPDIR/absent.json" "$SEED" >/dev/null
check
if [ ! -f "$SEED" ]; then
    fail "H: no fragment was seeded -- a CMAKE_CONFIGURE_DEPENDS input with no rule is a ninja load error"
fi
check
if ! nros_grep_q "NROS_ENTITY_INVENTORY_SCHEMA_VERSION" "$SEED"; then
    fail "H: the seeded fragment states no schema version, so reading it later FATALs"
fi
check
if ! nros_grep_q 'NROS_ENTITY_INVENTORY_STATUS "refused"' "$SEED"; then
    fail "H: the seeded fragment does not read as a refusal"
fi
KEEP="$TEST_TMPDIR/h-keep.cmake"
cp "$DERIVED_BODY" "$KEEP"
CMD="$TEST_TMPDIR/h-seed.cmake"
cat > "$CMD" <<EOF
include("$MODULE")
nros_entity_inventory_seed_knobs_file("$KEEP")
EOF
cmake -P "$CMD" >/dev/null 2>&1
check
if ! nros_grep_q "NROS_DERIVED_EXECUTOR_MAX_CBS 19" "$KEEP"; then
    fail "H: seeding overwrote an existing answer"
fi

# ---------------------------------------------------------------------------
log_header "the declared QoS DEPTH crosses the lane boundary (phase-412 W3)"

# The inventory publishes the depth table and, before this, nothing read it:
# `nros-node/build.rs` said so itself — "reach cmake and stop there, so this
# lane has nothing better to read yet". `_nros_qos_depth_env` in
# `cmake/NanoRosEntityFacts.cmake` is the crossing, and it is tested beside the
# writer.
#
# The GUARDS are the whole safety argument and each gets a case. A table over
# the endpoints that happened to be annotated sizes an image from a subset of
# itself, so one unannotated endpoint must mean "no answer" — the max would
# otherwise be a lower bound presented as a bound, which is the under-size
# direction the arena cannot survive.
#
# phase-454 W2 — the count guarding this is `..._COUNT_SUBSCRIPTION`, not the
# broad one, so `depth_env`'s second argument writes that name. The list is the
# SUBSCRIPTION depths and a guard must range over the same set as the thing it
# guards; and since a publisher can now declare a depth, a guard on the broad
# count would let a publisher's contract resize a subscription. The last case
# below is that regression: publisher facts in the fragment, no effect here.
FACTS="$PROJECT_ROOT/cmake/NanoRosEntityFacts.cmake"

depth_env() {
    # depth_env <status> <undeclared-subscription-count> <depths…>
    #
    # NROS_DEPTH_ENV_EXTRA, when set, is appended to the fragment verbatim —
    # used to put publisher-side facts in it and assert they change nothing.
    local dir="$TEST_TMPDIR/depth"
    rm -rf "$dir"; mkdir -p "$dir/nros"
    {
        printf 'set(NROS_ENTITY_DECLARED_DEPTH_STATUS "%s")\n' "$1"
        [ "$2" != "-" ] && printf 'set(NROS_ENTITY_UNDECLARED_DEPTH_COUNT_SUBSCRIPTION %s)\n' "$2"
        shift 2
        [ "$#" -gt 0 ] && printf 'set(NROS_ENTITY_DECLARED_DEPTHS "%s")\n' "$*"
        [ -n "${NROS_DEPTH_ENV_EXTRA:-}" ] && printf '%s\n' "$NROS_DEPTH_ENV_EXTRA"
    } > "$dir/nros/entity_inventory.cmake"
    cat > "$dir/run.cmake" <<EOF
include("$MODULE")
include("$FACTS")
_nros_qos_depth_env(_out)
message(STATUS "DEPTH=\${_out}")
EOF
    # `cmake -P` resolves CMAKE_BINARY_DIR to the CWD, and the carrier finds the
    # fragment through `nros_entity_inventory_knobs_file()`.
    (cd "$dir" && cmake -P run.cmake 2>&1) | sed -n 's/^-- DEPTH=//p'
}

_want() {
    local label="$1" want="$2" got="$3"
    if [ "$got" != "$want" ]; then
        fail "depth: $label -- wanted '${want:-<empty>}', got '${got:-<empty>}'"
    fi
    check
}

_want "a fully declared table crosses as its MAXIMUM" \
    "NROS_DECLARED_MAX_QOS_DEPTH=10" \
    "$(depth_env resolved 0 'a|/t1=1;b|/t2=10;c|/t3=5')"
_want "a single endpoint at depth 1 crosses as 1" \
    "NROS_DECLARED_MAX_QOS_DEPTH=1" \
    "$(depth_env resolved 0 'a|/t1=1')"
# The guard the producer's own doc demands.
_want "ONE undeclared endpoint carries nothing" \
    "" \
    "$(depth_env resolved 3 'a|/t1=1;b|/t2=10')"
_want "a refused status carries nothing" \
    "" \
    "$(depth_env refused 0 'a|/t1=1')"
_want "no depth table carries nothing" \
    "" \
    "$(depth_env resolved 0)"
_want "a missing undeclared COUNT carries nothing" \
    "" \
    "$(depth_env resolved - 'a|/t1=1')"
# A topic containing `=` must not shift the depth field.
_want "the depth is what follows the LAST '='" \
    "NROS_DECLARED_MAX_QOS_DEPTH=7" \
    "$(depth_env resolved 0 'a|/odd=name=7')"

# phase-454 W2 — a PUBLISHER's declaration is invisible here, in both
# directions. Nothing prices a publisher's depth yet, so a publisher that
# declares must not change a subscription number, and a publisher that stays
# SILENT must not suppress one. The second half is the defect issue 1227
# measured for `subs_arena` and this function was the last consumer still
# carrying: the broad count is 18 on the reference island against 11
# subscriptions that all declare.
_want "a declaring publisher does not enter the subscription maximum" \
    "NROS_DECLARED_MAX_QOS_DEPTH=1" \
    "$(NROS_DEPTH_ENV_EXTRA=$'set(NROS_ENTITY_DECLARED_DEPTHS_PUBLISHER "p|/t9=50")\nset(NROS_ENTITY_UNDECLARED_DEPTH_COUNT_PUBLISHER 0)\nset(NROS_ENTITY_UNDECLARED_DEPTH_COUNT 0)' depth_env resolved 0 'a|/t1=1')"
_want "a silent publisher does not suppress the subscription maximum" \
    "NROS_DECLARED_MAX_QOS_DEPTH=1" \
    "$(NROS_DEPTH_ENV_EXTRA=$'set(NROS_ENTITY_UNDECLARED_DEPTH_COUNT_PUBLISHER 4)\nset(NROS_ENTITY_UNDECLARED_DEPTH_COUNT 4)' depth_env resolved 0 'a|/t1=1')"

# ---------------------------------------------------------------------------
if [ "$FAILURES" -eq 0 ]; then
    log_success "cmake-entity-inventory: $CHECKS assertion(s) held"
    exit 0
fi
log_error "cmake-entity-inventory: $FAILURES of $CHECKS assertion(s) failed"
exit 1
