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
# Case J -- one line per invocation, so a case can read what the LAST call got.
if [ -n "${NROS_STUB_ARGV:-}" ]; then printf '%s\n' "$*" >> "$NROS_STUB_ARGV"; fi
exit "${NROS_STUB_RC:-0}"
STUB_EOF
chmod +x "$STUB"

DERIVED_BODY="$TEST_TMPDIR/derived.cmake"
cat > "$DERIVED_BODY" <<'EOF'
set(NROS_ENTITY_INVENTORY_SCHEMA_VERSION 6)
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
set(NROS_ENTITY_INVENTORY_SCHEMA_VERSION 6)
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
# split the depth table by kind, to 4/5, phase-454 W3, which added the
# other three QoS policies, to 5/6, and phase-454 W8, which gave the depth rows
# a provenance, to 6/7.
BAD_SCHEMA_BODY="$TEST_TMPDIR/bad-schema.cmake"
sed 's/SCHEMA_VERSION 6/SCHEMA_VERSION 7/' "$DERIVED_BODY" > "$BAD_SCHEMA_BODY"

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
if ! nros_grep_q "states entity-inventory schema version 7" <<<"$OUT"; then
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
# The declared QoS DEPTH's cmake crossing (`_nros_qos_depth_env`, phase-412 W3)
# is RETIRED (issue 1655): the guarded MAX is one function in the descriptor
# crate, `nros_sizing_descriptor::max_subscription_depth`, and its cases --
# the maximum, one silent subscription refusing the whole answer, a publisher
# neither entering nor suppressing it, no rows at all -- are that crate's unit
# test `max_subscription_depth_is_the_guarded_max_over_subscriptions`. The
# gate `check-knob-single-reader` refuses the carrier coming back.
FACTS="$PROJECT_ROOT/cmake/NanoRosEntityFacts.cmake"

# ---------------------------------------------------------------------------
log_header "a PARTIAL params: declaration is a refusal, not a default (phase-460 W2)"

# The inventory writes `NROS_PARAM_DECLARATION_STATUS` in three states, and
# `_nros_param_store_env` (NanoRosEntityFacts.cmake) is the crossing that hands
# the store's numbers to nros-params' build script as `NROS_DECLARED_*`
# defaults. Until phase-460 W2 the crossing `return()`ed on anything but
# `declared`, so a REFUSED declaration -- some nodes declare `params:`, the
# rest do not, and the inventory says which -- reached the crate as no
# declaration at all and the store took the crate defaults (issue 1421,
# measured on the island: 25/35/0/0/0 became 32/64/256/32/256 with no
# build-time line). Each state gets a case:
#
#   declared  crosses, every number, unchanged.
#   absent    crosses nothing and configures -- no node declares, and an image
#             with no contract is sized by its board. NEGATIVE CONTROL.
#   refused   is FATAL, and the fatal names the node the reason names.
#   (none)    a fragment that predates the field, or the seed placeholder a
#             refusal leaves for CMAKE_CONFIGURE_DEPENDS, crosses nothing and
#             configures: only the producer's own verdict is fatal.
param_env() {
    # param_env <fragment body> -> cmake's whole output; the return code is
    # cmake's, so a FATAL_ERROR is visible as rc and not only as text.
    local dir="$TEST_TMPDIR/param"
    rm -rf "$dir"; mkdir -p "$dir/nros"
    printf '%s\n' "$1" > "$dir/nros/entity_inventory.cmake"
    cat > "$dir/run.cmake" <<EOF
include("$MODULE")
include("$FACTS")
_nros_param_store_env(_out)
message(STATUS "PARAM=[\${_out}]")
EOF
    (cd "$dir" && cmake -P run.cmake 2>&1)
}

DECLARED_PARAMS='set(NROS_PARAM_DECLARATION_STATUS "declared")
set(NROS_PARAM_DECLARED_COUNT 21)
set(NROS_DERIVED_MAX_PARAMETERS 25)
set(NROS_DERIVED_MAX_PARAM_NAME_LEN 35)
set(NROS_DERIVED_MAX_STRING_VALUE_LEN 0)
set(NROS_DERIVED_MAX_ARRAY_LEN 0)
set(NROS_PARAM_NEEDS_MAX_BYTE_ARRAY_LEN "/system/diag_aggregator:blob:byte_array")
set(NROS_PARAM_SERVICE_SHAPE "8:170:1:17:0:0:0:0:0")'
# The reason is the producer's own sentence (`ParamDeclarations::from_model`),
# with the island's node in it.
REFUSED_PARAMS='set(NROS_PARAM_DECLARATION_STATUS "refused")
set(NROS_PARAM_DECLARATION_REASON "1 of 4 nodes in this image declare no `params:` in their contract: /system/stop_mode_operator. The parameter store holds every node'"'"'s parameters, and sizing it from the nodes that did declare would give the rest no slots. Declare `params:` on every node, or on none; until then the store knobs keep their configured values.")'

OUT="$(param_env "$DECLARED_PARAMS")"
RC=$?
check
if [ "$RC" -ne 0 ]; then
    fail "param: a DECLARED store did not configure -- $OUT"
fi
check
# Issue 1649 -- only the board CAPACITIES cross on this road now: the NEEDS
# rows and the service shape ride the sizing descriptor's `[params]`, so a
# fragment that states them must NOT produce a carrier for them.
if ! nros_grep_q "PARAM=\[NROS_DECLARED_MAX_STRING_VALUE_LEN=0;NROS_DECLARED_MAX_ARRAY_LEN=0\]" <<<"$OUT"; then
    fail "param: a DECLARED store did not cross with exactly the capacities -- $OUT"
fi

OUT="$(param_env 'set(NROS_PARAM_DECLARATION_STATUS "absent")')"
RC=$?
check
if [ "$RC" -ne 0 ]; then
    fail "param: an ABSENT declaration did not configure -- an image with no \
contract is sized by its board, and that must stay a configure -- $OUT"
fi
check
if ! nros_grep_q "PARAM=\[\]" <<<"$OUT"; then
    fail "param: an ABSENT declaration carried something -- $OUT"
fi
check
if nros_grep_q -i "CMake Error" <<<"$OUT"; then
    fail "param: an ABSENT declaration raised an error -- $OUT"
fi

OUT="$(param_env "$REFUSED_PARAMS" | flat)"
RC=$?
check
if [ "$RC" -eq 0 ]; then
    fail "param: a REFUSED declaration configured -- the store would be sized from \
the crate defaults with no build-time line, which is issue 1421 -- $OUT"
fi
check
if ! nros_grep_q -i "CMake Error" <<<"$OUT"; then
    fail "param: a REFUSED declaration did not raise a FATAL_ERROR -- $OUT"
fi
check
if ! nros_grep_q "stop_mode_operator" <<<"$OUT"; then
    fail "param: the fatal does not name the node the inventory named -- $OUT"
fi
check
if ! nros_grep_q "REFUSED the contract" <<<"$OUT"; then
    fail "param: the fatal does not say the inventory refused -- $OUT"
fi
check
if ! nros_grep_q "on every node" <<<"$OUT"; then
    fail "param: the fatal does not name the remedy -- $OUT"
fi
check
if nros_grep_q "PARAM=\[" <<<"$OUT"; then
    fail "param: a REFUSED declaration reached the crossing's output -- $OUT"
fi

OUT="$(param_env 'set(NROS_ENTITY_INVENTORY_STATUS "refused")')"
RC=$?
check
if [ "$RC" -ne 0 ]; then
    fail "param: a fragment with NO declaration status did not configure -- the \
seed placeholder and every pre-phase-446 fragment look like this -- $OUT"
fi
check
if ! nros_grep_q "PARAM=\[\]" <<<"$OUT"; then
    fail "param: a fragment with no declaration status carried something -- $OUT"
fi

# ---------------------------------------------------------------------------
# I. Issue 1480 -- an ABSENT inbox size writes its reason as a COMMENT, and the
#    fragment it is appended to must still PARSE.
#
#    `_nros_entity_inbox_bytes` returns a MULTI-LINE reason when a request type
#    carries no derived bound: one line per unbounded type. A `#` comment in
#    CMake runs to the end of ONE line, so writing that reason after a single
#    `#` left every later line at a COMMAND position, and the fragment became
#    `Parse error. Expected a command name, got unquoted argument with text
#    "example_interfaces/srv/AddTwoInts_Request"` on the NEXT include -- which
#    is `_nros_entity_budget_env`, not this function, so nothing here failed
#    and every `Build workspace fixtures` run died at cmake configure.
#
#    The assertion is the re-include, not the text: an appendix that reads
#    correctly and does not parse is exactly the state that shipped.
# ---------------------------------------------------------------------------
log_info "I. an absent inbox size leaves a fragment that still parses (issue 1480)"

BOUNDS_FRAG="$TEST_TMPDIR/i-bounds.cmake"
cat > "$BOUNDS_FRAG" <<'EOF'
# A bound inventory that knows one type and NOT the service request type below.
set(NROS_MESSAGE_BOUND_std_msgs_msg_Int32_STATE "bounded")
set(NROS_MESSAGE_BOUND_std_msgs_msg_Int32_RX 32)
EOF

UNBOUNDED_BODY="$TEST_TMPDIR/i-derived.cmake"
cp "$DERIVED_BODY" "$UNBOUNDED_BODY"
cat >> "$UNBOUNDED_BODY" <<EOF
set(NROS_ENTITY_SERVICE_REQUEST_TYPES_STATUS "resolved")
set(NROS_ENTITY_SERVICE_REQUEST_TYPES "example_interfaces/srv/AddTwoInts_Request")
set_property(GLOBAL PROPERTY NROS_MESSAGE_BOUNDS_FRAGMENTS "$BOUNDS_FRAG")
EOF

I_FRAG="$TEST_TMPDIR/i.cmake"
OUT="$(derive "$UNBOUNDED_BODY" 0 "$META" "$I_FRAG")"
check
if ! nros_grep_q "NROS_DERIVED_SERVICE_INBOX_BYTES is ABSENT" "$I_FRAG"; then
    fail "I: the appendix did not record the absent service inbox size -- \
the case cannot assert anything about a reason it never wrote -- $OUT"
fi
check
if nros_grep_q "^set(NROS_DERIVED_SERVICE_INBOX_BYTES" "$I_FRAG"; then
    fail "I: an unbounded request type published a number anyway; absence is \
the answer here (RFC-0100 D6)"
fi
# The regression itself. Include the fragment the way `_nros_entity_budget_env`
# does; a stray line at a command position is a hard parse error.
I_INCLUDE="$TEST_TMPDIR/i-include.cmake"
cat > "$I_INCLUDE" <<EOF
include("$I_FRAG")
message(STATUS "I: fragment re-included")
EOF
check
if ! I_OUT="$(cmake -P "$I_INCLUDE" 2>&1)"; then
    fail "I: the appended fragment does not parse on re-include -- this is \
issue 1480, and it takes every cmake workspace image down at configure -- $I_OUT"
fi

# ---------------------------------------------------------------------------
# J. Issue 1600 -- MODEL accumulates across the calls of one configure.
#
# A multi-entry configure calls the derivation once per entry, each with its
# own model, into ONE fragment, and links ONE runtime into every entry. When
# each call passed only its own model, the fragment the readers found after
# the entries was the LAST entry's, and `examples/workspaces/cpp`'s
# `native_entry` (two callbacks) got the service server's MAX_CBS of 1 and died
# `ExecutorFull` at boot. The last call must name EVERY model, each once.
# ---------------------------------------------------------------------------
log_info "J. every entry's model reaches the last derivation, each once"
J_A="$TEST_TMPDIR/j-model-a.yaml"; : > "$J_A"
J_B="$TEST_TMPDIR/j-model-b.yaml"; : > "$J_B"
J_ARGV="$TEST_TMPDIR/j-argv.txt"; : > "$J_ARGV"
J_SEQ="$TEST_TMPDIR/j-seq.cmake"
cat > "$J_SEQ" <<EOF
include("$MODULE")
foreach(_m "$J_A" "$J_B" "$J_A" "$TEST_TMPDIR/j-model-missing.yaml")
    nros_derive_entity_inventory_knobs(CLI "$STUB" METADATA "$META" MODEL "\${_m}"
        OUTPUT_FILE "$TEST_TMPDIR/j.cmake" QUIET)
endforeach()
EOF
OUT="$(NROS_STUB_BODY="$DERIVED_BODY" NROS_STUB_ARGV="$J_ARGV" cmake -P "$J_SEQ" 2>&1)"
J_LAST="$(tail -n 1 "$J_ARGV")"
check
if [ "$(wc -l < "$J_ARGV")" -ne 4 ]; then
    fail "J: expected four derivations, the stub saw $(wc -l < "$J_ARGV") -- $OUT"
fi
check
if [ "$(grep -o -- "--model $J_A" <<<"$J_LAST" | wc -l)" -ne 1 ] ||
   [ "$(grep -o -- "--model $J_B" <<<"$J_LAST" | wc -l)" -ne 1 ]; then
    fail "J: the last derivation must name BOTH models exactly once (a repeated \
model is one image, not two) -- it got: $J_LAST"
fi
check
if nros_grep_q -- "j-model-missing" <<<"$J_LAST"; then
    fail "J: a model path that does not exist reached the verb, where \
'--model <missing>' is an error: $J_LAST"
fi
check
if [ "$(head -n 1 "$J_ARGV" | grep -o -- "--model" | wc -l)" -ne 1 ]; then
    fail "J: the FIRST call must still pass exactly its own model -- $(head -n 1 "$J_ARGV")"
fi

# ---------------------------------------------------------------------------
if [ "$FAILURES" -eq 0 ]; then
    log_success "cmake-entity-inventory: $CHECKS assertion(s) held"
    exit 0
fi
log_error "cmake-entity-inventory: $FAILURES of $CHECKS assertion(s) failed"
exit 1
