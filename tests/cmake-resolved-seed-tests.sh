#!/usr/bin/env bash
# tests/cmake-resolved-seed-tests.sh -- phase-439 W2 (RFC-0094 D1).
#
# WHAT THIS MEASURES
#
# `nros build` gained a resolve phase (stage 3.5) that writes an image's derived
# counts BEFORE any configure, as `<NROS_RESOLVED_DIR>/resolved.cmake`.
# `nros_resolved_seed_entity_inventory()` is the configure-side reader: it seeds
# the entity-inventory fragment from that file where the build would otherwise
# have written a placeholder.
#
# The claim under test is about the PASS COUNT, not about a number. The
# producer-after-reader chain (`NanoRosReconfigure.cmake`) costs one extra
# configure per link, because the earliest reader of pass 1 sees a placeholder
# and issue 0991's future-mtime arm buys the pass in which it does not. A seed
# written before the configure removes exactly that pass -- IF the producer then
# agrees with it.
#
# So three cases, and the third is the one that makes this a test rather than a
# demonstration:
#
#   A  no resolve phase         1 re-configure, built with the real answer
#                               (today's behaviour, and it must not change)
#   B  resolve AGREES           0 re-configures, built with the real answer
#                               (the pass this phase removes)
#   C  resolve DISAGREES        the producer still overwrites and still arms,
#                               and the build uses the PRODUCER's answer
#
# C is the safety property. The seed and the mid-configure producer do not read
# the same inputs -- the producer also reads `nros-metadata.json`, whose
# component set is what makes `derive()` refuse when a registered component is
# absent from the launch declaration -- so a seed that is wrong must lose. A
# green A and B with a red C would mean this phase can silently ship a number
# nothing stood behind, which is the exact defect RFC-0094 exists to remove.
#
# PRECONDITIONS ARE HARD FAILURES. This script never prints-and-returns: a green
# here is read as "the resolve seed works and cannot override a producer".
# Skipping belongs in the `just` recipe's check ledger, which is a different
# claim from "it passed".
#
# Usage: ./tests/cmake-resolved-seed-tests.sh
# Exit:  0 all assertions held; 1 otherwise.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# shellcheck source=lib/common.sh
source "$SCRIPT_DIR/lib/common.sh"
# `nros_grep_q` -- 0 match / 1 no-match / exit 2 when grep could not run, so a
# tool failure never becomes a finding (issue 0726). HERE-STRING, never a pipe
# (issue 1077).
# shellcheck source=../scripts/lib/grep-q.sh
source "$PROJECT_ROOT/scripts/lib/grep-q.sh"

RECONFIGURE_MODULE="$PROJECT_ROOT/cmake/NanoRosReconfigure.cmake"
RESOLVED_MODULE="$PROJECT_ROOT/cmake/NanoRosResolved.cmake"

FAILURES=0
CHECKS=0

fail() {
    log_error "$*"
    FAILURES=$((FAILURES + 1))
}

check() {
    CHECKS=$((CHECKS + 1))
}

for _m in "$RECONFIGURE_MODULE" "$RESOLVED_MODULE"; do
    if [ ! -f "$_m" ]; then
        fail "module not found: $_m"
        exit 1
    fi
done
for _t in cmake ninja touch; do
    if ! command -v "$_t" >/dev/null 2>&1; then
        fail "$_t is not on PATH -- this test cannot report a verdict without it"
        exit 1
    fi
done

init_test_tmpdir "nros-resolved-seed"
trap 'cleanup_test_tmpdir' EXIT

# ---------------------------------------------------------------------------
# The project under test: the reader/producer chain in miniature.
#
# The READER seeds (resolve first, placeholder second -- the production order in
# `_nros_load_derived_entity_inventory`), registers the fragment as a configure
# dependency, settles, and reads. The PRODUCER, later in the SAME configure,
# writes `real` and arms on change.
#
# The `go` target echoes the answer the configure that generated it read, so
# what the BUILD used is observable in ninja's output rather than inferred from
# the fragment on disk. That distinction is the whole subject: the fragment is
# always eventually correct, and what was wrong is the answer the build was
# sized from.
# ---------------------------------------------------------------------------
write_project() {
    local dir="$1"
    mkdir -p "$dir"
    cat > "$dir/CMakeLists.txt" <<EOF
cmake_minimum_required(VERSION 3.20)
project(nros_resolved_seed_probe NONE)

include("$RECONFIGURE_MODULE")
include("$RESOLVED_MODULE")

set(_frag "\${CMAKE_BINARY_DIR}/frag.cmake")

# ---- the READER, early in the configure -------------------------------------
if(NOT EXISTS "\${_frag}")
    nros_resolved_seed_entity_inventory("\${_frag}")
endif()
if(NOT EXISTS "\${_frag}")
    file(WRITE "\${_frag}" "set(ANSWER placeholder)\n")
endif()
set_property(DIRECTORY APPEND PROPERTY CMAKE_CONFIGURE_DEPENDS "\${_frag}")
nros_reconfigure_settle("\${_frag}")
include("\${_frag}")
message(STATUS "PROBE_READ=\${ANSWER}")

# ---- the PRODUCER, later in the same configure -------------------------------
nros_reconfigure_snapshot("\${_frag}" _before)
file(WRITE "\${_frag}" "set(ANSWER real)\n")
nros_reconfigure_on_change("\${_frag}" "\${_before}" LABEL "the probe answer")

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

SRC="$TEST_TMPDIR/src"
write_project "$SRC"

# ---------------------------------------------------------------------------
log_header "A. no resolve phase: today's behaviour, one extra configure"
# ---------------------------------------------------------------------------

CONFIGURE_OUT="$(cmake -G Ninja -S "$SRC" -B "$TEST_TMPDIR/none-build" 2>&1)"
check
if nros_grep_q 'PROBE_READ=placeholder' <<<"$CONFIGURE_OUT"; then
    log_success "with no NROS_RESOLVED_DIR the first pass reads the placeholder"
else
    fail "expected the placeholder on pass 1 with no resolve phase:
$CONFIGURE_OUT"
fi

run_build "$TEST_TMPDIR/none-build"
check
if [ "$BUILD_USED" = "real" ] && [ "$BUILD_RERUNS" -eq 1 ]; then
    log_success "no resolve phase: 1 re-configure, built with the real answer (baseline)"
else
    fail "the baseline moved: used='$BUILD_USED' re-runs=$BUILD_RERUNS (want real / 1).
  This case asserts that a lane which ran NO resolve phase is unchanged. If it
  fails, the seed is reaching a lane that never asked for it.
$BUILD_OUT"
fi

# ---------------------------------------------------------------------------
log_header "B. the resolve phase AGREES: the extra configure is gone"
# ---------------------------------------------------------------------------

RESOLVED_OK="$TEST_TMPDIR/resolved-ok"
mkdir -p "$RESOLVED_OK"
# Byte-for-byte what the producer writes. That is the real condition: the seed
# saves a pass only when the two composers agree, which is why case C exists.
printf 'set(ANSWER real)\n' > "$RESOLVED_OK/resolved.cmake"

CONFIGURE_OUT="$(cmake -G Ninja -S "$SRC" -B "$TEST_TMPDIR/ok-build" \
    "-DNROS_RESOLVED_DIR=$RESOLVED_OK" 2>&1)"
check
if nros_grep_q 'PROBE_READ=real' <<<"$CONFIGURE_OUT"; then
    log_success "the first pass reads the resolve phase's answer, not a placeholder"
else
    fail "the seed did not reach the first reader:
$CONFIGURE_OUT"
fi

check
if nros_grep_q 'seeded from the resolve phase' <<<"$CONFIGURE_OUT"; then
    log_success "and it SAYS so -- a silent seed is not auditable"
else
    fail "the seed left no status line; a build must be able to say where its
  numbers came from (RFC-0094 D2, [provenance]):
$CONFIGURE_OUT"
fi

run_build "$TEST_TMPDIR/ok-build"
check
if [ "$BUILD_USED" = "real" ] && [ "$BUILD_RERUNS" -eq 0 ]; then
    log_success "resolve agrees: 0 re-configures, built with the real answer (one pass saved)"
else
    fail "the seed did not remove the pass: used='$BUILD_USED' re-runs=$BUILD_RERUNS (want real / 0).
$BUILD_OUT"
fi

# ---------------------------------------------------------------------------
log_header "C. the resolve phase DISAGREES: the producer still wins, and still arms"
# ---------------------------------------------------------------------------
#
# The safety property. The two composers read different inputs, so a seed that
# is wrong must lose to the producer -- otherwise stage 3.5 could ship a number
# nothing stood behind, which is the defect this whole RFC exists to remove.

RESOLVED_BAD="$TEST_TMPDIR/resolved-bad"
mkdir -p "$RESOLVED_BAD"
printf 'set(ANSWER stale_seed)\n' > "$RESOLVED_BAD/resolved.cmake"

CONFIGURE_OUT="$(cmake -G Ninja -S "$SRC" -B "$TEST_TMPDIR/bad-build" \
    "-DNROS_RESOLVED_DIR=$RESOLVED_BAD" 2>&1)"
check
if nros_grep_q 'PROBE_READ=stale_seed' <<<"$CONFIGURE_OUT"; then
    log_success "pass 1 read the (wrong) seed, so the disagreement is real"
else
    fail "case C did not set up: the seed never reached the reader:
$CONFIGURE_OUT"
fi

run_build "$TEST_TMPDIR/bad-build"
check
if [ "$BUILD_USED" = "real" ] && [ "$BUILD_RERUNS" -eq 1 ]; then
    log_success "resolve disagrees: the producer overwrote it, armed, and the build used the PRODUCER's answer"
else
    fail "A WRONG SEED REACHED THE BUILD: used='$BUILD_USED' re-runs=$BUILD_RERUNS (want real / 1).
  The seed must never be able to override the mid-configure producer -- they do
  not read the same inputs, and the producer is the one that sees the component
  set that makes a derivation REFUSE.
$BUILD_OUT"
fi

# ---------------------------------------------------------------------------
log_header "D. a refused resolve writes no projection, so nothing is seeded"
# ---------------------------------------------------------------------------
#
# `nros build` writes `resolved.toml` for every image and `resolved.cmake` only
# for one that DERIVED. A directory holding just the toml must therefore behave
# exactly like case A -- an empty projection would publish "every count is
# zero", a substituted default wearing an answer's clothes.

RESOLVED_REFUSED="$TEST_TMPDIR/resolved-refused"
mkdir -p "$RESOLVED_REFUSED"
printf 'status = "refused"\n' > "$RESOLVED_REFUSED/resolved.toml"

CONFIGURE_OUT="$(cmake -G Ninja -S "$SRC" -B "$TEST_TMPDIR/refused-build" \
    "-DNROS_RESOLVED_DIR=$RESOLVED_REFUSED" 2>&1)"
check
if nros_grep_q 'PROBE_READ=placeholder' <<<"$CONFIGURE_OUT"; then
    log_success "a refused resolve seeds nothing; the placeholder stands"
else
    fail "a refusal must not seed anything:
$CONFIGURE_OUT"
fi

run_build "$TEST_TMPDIR/refused-build"
check
if [ "$BUILD_USED" = "real" ] && [ "$BUILD_RERUNS" -eq 1 ]; then
    log_success "and the build is byte-for-byte case A's outcome"
else
    fail "a refused resolve changed the baseline: used='$BUILD_USED' re-runs=$BUILD_RERUNS.
$BUILD_OUT"
fi

# ---------------------------------------------------------------------------
log_header "Summary"
# ---------------------------------------------------------------------------

if [ "$FAILURES" -eq 0 ]; then
    log_success "$CHECKS checks passed"
    exit 0
fi
log_error "$FAILURES of $CHECKS checks failed"
exit 1
