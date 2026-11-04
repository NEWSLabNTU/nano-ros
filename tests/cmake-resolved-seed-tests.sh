#!/usr/bin/env bash
# tests/cmake-resolved-seed-tests.sh -- phase-439 W2 (RFC-0094 D1), phase-530
# W4 (RFC-0120 D2/D5).
#
# WHAT THIS MEASURES
#
# `nros configure` resolves an image's derived counts BEFORE any host
# configure, as `<resolve dir>/resolved.cmake`, and a host configure locates
# that dir by the image's identity (`nros_configured_locate`, which this test
# stands in for with `nros_configured_publish` -- the retired
# `-DNROS_RESOLVED_DIR` path is gone). `nros_resolved_seed_entity_inventory()`
# is the reader: it seeds the entity-inventory fragment from that file where
# the build would otherwise have written a placeholder.
#
# The claim under test is about the PASS COUNT, and about what happens when the
# late producer disagrees with the seed (RFC-0120 D5's promise check):
#
#   A  no configure in front    0 re-configures; the readers used their
#                               defaults (a placeholder) and say so -- issue
#                               0991's fixed point is deleted (phase-530 W7)
#   X  no seed, and the readers read an EARLIER configure's answer that the
#      producer moves           the configure FAILS naming `nros configure`:
#                               that answer can be smaller than this one
#   Z  no seed, the readers used a placeholder, and the composed answer is
#      DERIVED                  FAILS: the two halves of the image disagree
#   Y  the CMake road's shape: seed, then the composer, then the reader (the
#      entity-facts flush)      a composer that disagrees with the seed FAILS
#                               although no reader ran before it
#   B  seed AGREES              0 re-configures, built with the real answer
#   C  seed DISAGREES           the configure FAILS, naming both files -- every
#                               reader used the seed, so nothing may build on it
#                               and nothing re-converges
#
# C is the safety property. A wrong seed must never reach a build silently: it
# used to lose to the producer through one more configure; it now stops the
# configure, because the two composers of one answer disagreeing is a bug.
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

PROMISE_MODULE="$PROJECT_ROOT/cmake/NanoRosPromise.cmake"
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

for _m in "$PROMISE_MODULE" "$RESOLVED_MODULE"; do
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

include("$RESOLVED_MODULE")
# The Zephyr hook's half: where configure put this image's answer.
if(DEFINED PROBE_RESOLVED)
    nros_configured_publish("\${PROBE_RESOLVED}")
endif()

set(_frag "\${CMAKE_BINARY_DIR}/frag.cmake")

# ---- the READER, early in the configure -------------------------------------
# Every configure, as `_nros_load_derived_entity_inventory` does since issue
# 1729: the function itself decides whether the resolve moved.
nros_resolved_seed_entity_inventory("\${_frag}")
if(NOT EXISTS "\${_frag}")
    # The production placeholders' marker line (nros_promise_placeholder_line).
    nros_promise_placeholder_line(_ph)
    file(WRITE "\${_frag}" "\${_ph}set(ANSWER placeholder)\n")
endif()
set_property(DIRECTORY APPEND PROPERTY CMAKE_CONFIGURE_DEPENDS "\${_frag}")
nros_promise_read("\${_frag}")
include("\${_frag}")
message(STATUS "PROBE_READ=\${ANSWER}")
# What the readers resolved, as the knob resolver caches it: the subject of
# `scripts/check-knob-one-pass.py`.
set(NROS_RESOLVED_PROBE_ANSWER "\${ANSWER}" CACHE INTERNAL "")

# ---- the PRODUCER, later in the same configure -------------------------------
# Absent on the generated Rust Zephyr entry's road (issue 1729), where the seed
# is the fragment's only producer: -DPROBE_NO_PRODUCER=ON.
if(NOT PROBE_NO_PRODUCER)
    if(NOT DEFINED PROBE_ANSWER)
        set(PROBE_ANSWER real)
    endif()
    nros_promise_snapshot("\${_frag}" _before)
    file(WRITE "\${_frag}" "set(ANSWER \${PROBE_ANSWER})\n")
    if(PROBE_DERIVED)
        file(APPEND "\${_frag}" "set(PROBE_STATUS \"derived\")\n")
    endif()
    nros_promise_check("\${_frag}" "\${_before}" LABEL "the probe answer")
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

SRC="$TEST_TMPDIR/src"
write_project "$SRC"

# ---------------------------------------------------------------------------
log_header "A. no configure in front: 0 re-configures, the readers used their defaults"
# ---------------------------------------------------------------------------

CONFIGURE_OUT="$(cmake -G Ninja -S "$SRC" -B "$TEST_TMPDIR/none-build" 2>&1)"
check
if nros_grep_q 'PROBE_READ=placeholder' <<<"$CONFIGURE_OUT"; then
    log_success "with nothing located the reader reads the placeholder"
else
    fail "expected the placeholder with no resolve phase:
$CONFIGURE_OUT"
fi
check
if nros_grep_q -F 'they used their defaults' <<<"$CONFIGURE_OUT"; then
    log_success "and the configure SAYS the readers used their defaults"
else
    fail "an unseeded reader of a placeholder left no status line:
$CONFIGURE_OUT"
fi

run_build "$TEST_TMPDIR/none-build"
check
if [ "$BUILD_USED" = "placeholder" ] && [ "$BUILD_RERUNS" -eq 0 ]; then
    log_success "no resolve phase: 0 re-configures (the fixed point is gone), defaults used"
else
    fail "an unseeded configure re-ran or changed its answer: used='$BUILD_USED' re-runs=$BUILD_RERUNS (want placeholder / 0).
  Issue 0991's fixed point is deleted (phase-530 W7): nothing may re-run cmake
  to converge an answer.
$BUILD_OUT"
fi
run_build "$TEST_TMPDIR/none-build"
check
if [ "$BUILD_RERUNS" -eq 0 ]; then
    log_success "and a no-op build after it re-runs nothing"
else
    fail "a no-op build re-configured (re-runs=$BUILD_RERUNS):
$BUILD_OUT"
fi

# ---------------------------------------------------------------------------
log_header "X. no seed, an earlier configure's answer the producer moves: FATAL"
# ---------------------------------------------------------------------------
#
# The one unseeded case that can UNDER-size: the readers read an answer an
# earlier configure composed, and this configure composes another. Nothing
# re-converges; the configure stops and names what to run.
X_RC=0
CONFIGURE_OUT="$(cmake -G Ninja -S "$SRC" -B "$TEST_TMPDIR/none-build" \
    -DPROBE_ANSWER=moved 2>&1)" || X_RC=$?
check
if nros_grep_q 'PROBE_READ=real' <<<"$CONFIGURE_OUT"; then
    log_success "the reader read the earlier configure's answer"
else
    fail "case X did not set up:
$CONFIGURE_OUT"
fi
check
if [ "$X_RC" -ne 0 ] && nros_grep_q -F 'can be smaller than this one' <<<"$CONFIGURE_OUT" \
   && nros_grep_q -F 'nros configure' <<<"$CONFIGURE_OUT"; then
    log_success "the producer moved an answer the readers used: the configure STOPPED, naming nros configure"
else
    fail "AN UNSEEDED READER OF A MOVED ANSWER DID NOT STOP THE CONFIGURE (rc=$X_RC):
$CONFIGURE_OUT"
fi
X_RC=0
CONFIGURE_OUT="$(cmake -G Ninja -S "$SRC" -B "$TEST_TMPDIR/none-build" \
    -DPROBE_ANSWER=moved 2>&1)" || X_RC=$?
check
if [ "$X_RC" -eq 0 ] && nros_grep_q 'PROBE_READ=moved' <<<"$CONFIGURE_OUT"; then
    log_success "and the configure after it reads the new answer and passes"
else
    fail "the configure after the refusal did not read the new answer (rc=$X_RC):
$CONFIGURE_OUT"
fi

# ---------------------------------------------------------------------------
log_header "Z. no seed, a placeholder read, a DERIVED composed answer: FATAL"
# ---------------------------------------------------------------------------
Z_RC=0
CONFIGURE_OUT="$(cmake -G Ninja -S "$SRC" -B "$TEST_TMPDIR/derived-build" \
    -DPROBE_DERIVED=ON 2>&1)" || Z_RC=$?
check
if [ "$Z_RC" -ne 0 ] && nros_grep_q -F 'The composed answer is DERIVED' <<<"$CONFIGURE_OUT"; then
    log_success "defaults read, derived answer composed: the configure STOPPED"
else
    fail "an unseeded placeholder read under a DERIVED answer did not stop the configure (rc=$Z_RC):
$CONFIGURE_OUT"
fi
check
if ! nros_grep_q -F 'never smaller' <<<"$CONFIGURE_OUT"; then
    log_success "and no message claims the defaults are never too small"
else
    fail "a message still claims defaults never under-size:
$CONFIGURE_OUT"
fi
WARN_OUT="$(cmake -G Ninja -S "$SRC" -B "$TEST_TMPDIR/warn-build" 2>&1)"
check
if nros_grep_q -F 'CMake Warning' <<<"$WARN_OUT" && nros_grep_q -F 'they used their defaults' <<<"$WARN_OUT"; then
    log_success "a non-derived unseeded answer is a WARNING"
else
    fail "a non-derived unseeded answer did not warn:
$WARN_OUT"
fi

# ---------------------------------------------------------------------------
log_header "Y. the CMake road's order: seed -> composer -> reader; a disagreeing composer FAILS"
# ---------------------------------------------------------------------------
#
# Phase-530 W7 review I3 (Ruling R49). On the CMake roads every reader of the
# entity fragment runs in the flush, AFTER the composer, so a read-recorded
# basis is the composer's own answer. The seed is the basis instead.
YSRC="$TEST_TMPDIR/ysrc"
mkdir -p "$YSRC"
cat > "$YSRC/CMakeLists.txt" <<YEOF
cmake_minimum_required(VERSION 3.20)
project(nros_seed_road_probe NONE)
include("$RESOLVED_MODULE")
nros_configured_publish("\${PROBE_RESOLVED}")
set(_frag "\${CMAKE_BINARY_DIR}/frag.cmake")
nros_resolved_seed_entity_inventory("\${_frag}")
# the composer: no reader has run
nros_promise_snapshot("\${_frag}" _before)
file(WRITE "\${_frag}" "set(ANSWER \${PROBE_ANSWER})\n")
nros_promise_check("\${_frag}" "\${_before}" LABEL "the probe answer")
# the flush, deferred: it reads after the composer
function(_y_flush)
    nros_promise_read("\${CMAKE_BINARY_DIR}/frag.cmake")
endfunction()
cmake_language(DEFER DIRECTORY "\${CMAKE_SOURCE_DIR}" CALL _y_flush)
YEOF
mkdir -p "$TEST_TMPDIR/y-resolved"
printf 'set(ANSWER real)\n' > "$TEST_TMPDIR/y-resolved/resolved.cmake"
Y_RC=0
CONFIGURE_OUT="$(cmake -G Ninja -S "$YSRC" -B "$TEST_TMPDIR/y-ok" \
    "-DPROBE_RESOLVED=$TEST_TMPDIR/y-resolved" -DPROBE_ANSWER=real 2>&1)" || Y_RC=$?
check
if [ "$Y_RC" -eq 0 ]; then
    log_success "a composer that agrees with the seed configures"
else
    fail "an agreeing composer failed (rc=$Y_RC):
$CONFIGURE_OUT"
fi
Y_RC=0
CONFIGURE_OUT="$(cmake -G Ninja -S "$YSRC" -B "$TEST_TMPDIR/y-bad" \
    "-DPROBE_RESOLVED=$TEST_TMPDIR/y-resolved" -DPROBE_ANSWER=other 2>&1)" || Y_RC=$?
check
if [ "$Y_RC" -ne 0 ] && nros_grep_q -F "over the same inputs" <<<"$CONFIGURE_OUT"; then
    log_success "a composer that disagrees with the seed STOPPED the configure, with no early reader"
else
    fail "A COMPOSER DISAGREEING WITH THE SEED PASSED on the CMake road's order (rc=$Y_RC):
$CONFIGURE_OUT"
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
    "-DPROBE_RESOLVED=$RESOLVED_OK" 2>&1)"
check
if nros_grep_q 'PROBE_READ=real' <<<"$CONFIGURE_OUT"; then
    log_success "the first pass reads the resolve phase's answer, not a placeholder"
else
    fail "the seed did not reach the first reader:
$CONFIGURE_OUT"
fi

check
if nros_grep_q 'seeded from `nros configure`' <<<"$CONFIGURE_OUT"; then
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
run_build "$TEST_TMPDIR/ok-build"
check
if [ "$BUILD_RERUNS" -eq 0 ]; then
    log_success "a no-op build after it re-runs cmake 0 times"
else
    fail "a no-op build re-ran cmake (re-runs=$BUILD_RERUNS):
$BUILD_OUT"
fi
# phase-530 W7 review M4 -- the one-pass gate's REAL assertion, over a dir this
# lane configured itself: a second configure with nothing changed moves no
# resolved knob.
ONE_PASS_RC=0
ONE_PASS_OUT="$(python3 "$PROJECT_ROOT/scripts/check-knob-one-pass.py" "$TEST_TMPDIR/ok-build" \
    --configure "cmake '$TEST_TMPDIR/ok-build'" 2>&1)" || ONE_PASS_RC=$?
check
if [ "$ONE_PASS_RC" -eq 0 ] && nros_grep_q -F 'one configure settled it' <<<"$ONE_PASS_OUT"; then
    log_success "check-knob-one-pass: a second configure moved no resolved knob"
else
    fail "check-knob-one-pass failed on a seeded dir (rc=$ONE_PASS_RC):
$ONE_PASS_OUT"
fi

# ---------------------------------------------------------------------------
log_header "C. the seed DISAGREES: the configure stops, naming both files"
# ---------------------------------------------------------------------------
#
# The safety property (RFC-0120 D5). Every reader of the configure used the
# seed, so a producer that composes something else means the two composers of
# one answer disagree: no build may proceed on either, and no second pass may
# quietly pick whichever came last.

RESOLVED_BAD="$TEST_TMPDIR/resolved-bad"
mkdir -p "$RESOLVED_BAD"
printf 'set(ANSWER stale_seed)\n' > "$RESOLVED_BAD/resolved.cmake"

BAD_RC=0
CONFIGURE_OUT="$(cmake -G Ninja -S "$SRC" -B "$TEST_TMPDIR/bad-build" \
    "-DPROBE_RESOLVED=$RESOLVED_BAD" 2>&1)" || BAD_RC=$?
check
if nros_grep_q 'PROBE_READ=stale_seed' <<<"$CONFIGURE_OUT"; then
    log_success "the first reader read the (wrong) seed, so the disagreement is real"
else
    fail "case C did not set up: the seed never reached the reader:
$CONFIGURE_OUT"
fi
check
if [ "$BAD_RC" -ne 0 ] && nros_grep_q -F "over the same inputs" <<<"$CONFIGURE_OUT" \
   && nros_grep_q -F "$RESOLVED_BAD/resolved.cmake" <<<"$CONFIGURE_OUT" \
   && nros_grep_q -F "$TEST_TMPDIR/bad-build/frag.cmake" <<<"$CONFIGURE_OUT"; then
    log_success "the producer disagreed: the configure STOPPED, naming the seed and the fragment"
else
    fail "A WRONG SEED DID NOT STOP THE CONFIGURE (rc=$BAD_RC).
  Every reader used the seed; a producer that composes something else must fail
  the configure naming both files (RFC-0120 D5).
$CONFIGURE_OUT"
fi
check
if nros_grep_q -F "changed after the readers" <<<"$CONFIGURE_OUT"; then
    fail "a seeded configure armed the fixed point: it must fail, never re-converge
$CONFIGURE_OUT"
else
    log_success "and nothing armed a re-configure"
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
    "-DPROBE_RESOLVED=$RESOLVED_REFUSED" 2>&1)"
check
if nros_grep_q 'PROBE_READ=placeholder' <<<"$CONFIGURE_OUT"; then
    log_success "a refused resolve seeds nothing; the placeholder stands"
else
    fail "a refusal must not seed anything:
$CONFIGURE_OUT"
fi

run_build "$TEST_TMPDIR/refused-build"
check
if [ "$BUILD_USED" = "placeholder" ] && [ "$BUILD_RERUNS" -eq 0 ]; then
    log_success "and the build is byte-for-byte case A's outcome"
else
    fail "a refused resolve changed the baseline: used='$BUILD_USED' re-runs=$BUILD_RERUNS.
$BUILD_OUT"
fi

# ---------------------------------------------------------------------------
log_header "E. no producer: a resolve that MOVES reaches the build (issue 1729)"
# ---------------------------------------------------------------------------
#
# The generated Rust Zephyr entry runs no `nano_ros_entry()`, so the seed is the
# fragment's ONLY producer. Before issue 1729 it seeded only an absent fragment
# and `resolved.cmake` was no configure dependency, so the first configure's
# answer was the image's answer forever: `nros build` re-resolved a derived-tier
# image from three publisher slots to four, ninja ran no configure, and the
# image failed `PublisherCreationFailed` at boot.

RESOLVED_MOVING="$TEST_TMPDIR/resolved-moving"
mkdir -p "$RESOLVED_MOVING"
printf 'set(ANSWER first)\n' > "$RESOLVED_MOVING/resolved.cmake"
cmake -G Ninja -S "$SRC" -B "$TEST_TMPDIR/moving-build" \
    "-DPROBE_RESOLVED=$RESOLVED_MOVING" -DPROBE_NO_PRODUCER=ON >/dev/null 2>&1
run_build "$TEST_TMPDIR/moving-build"
check
if [ "$BUILD_USED" = "first" ] && [ "$BUILD_RERUNS" -eq 0 ]; then
    log_success "no producer: the first build uses the seed, with no extra configure"
else
    fail "case E did not set up: used='$BUILD_USED' re-runs=$BUILD_RERUNS (want first / 0).
$BUILD_OUT"
fi

# The resolve phase re-runs and its answer moves. A later mtime than the
# configure's outputs, as a real rewrite has.
sleep 1
printf 'set(ANSWER second)\n' > "$RESOLVED_MOVING/resolved.cmake"
run_build "$TEST_TMPDIR/moving-build"
check
if [ "$BUILD_USED" = "second" ] && [ "$BUILD_RERUNS" -ge 1 ]; then
    log_success "a moved resolve re-configured the build and reached it"
else
    fail "A MOVED RESOLVE DID NOT REACH THE BUILD: used='$BUILD_USED' re-runs=$BUILD_RERUNS (want second / >=1).
  This is issue 1729: the image keeps the first configure's pool sizes.
$BUILD_OUT"
fi

run_build "$TEST_TMPDIR/moving-build"
check
if [ "$BUILD_RERUNS" -eq 0 ]; then
    log_success "and an unchanged resolve costs no further configure"
else
    fail "an unchanged resolve re-configured again (re-runs=$BUILD_RERUNS): a loop.
$BUILD_OUT"
fi

# ---------------------------------------------------------------------------
log_header "F. the seed moved to the producer's answer: one configure, no failure"
# ---------------------------------------------------------------------------
#
# After case C the fix is to make configure's answer right (here: the seed
# itself), and the same build dir then configures once and builds -- the
# fragment the failed configure left behind is not what the readers see,
# because the seed is copied in by CONTENT on every configure.

sleep 1
printf 'set(ANSWER real)\n' > "$RESOLVED_BAD/resolved.cmake"
F_RC=0
CONFIGURE_OUT="$(cmake -G Ninja -S "$SRC" -B "$TEST_TMPDIR/bad-build" \
    "-DPROBE_RESOLVED=$RESOLVED_BAD" 2>&1)" || F_RC=$?
check
if [ "$F_RC" -eq 0 ] && nros_grep_q 'PROBE_READ=real' <<<"$CONFIGURE_OUT"; then
    log_success "the corrected seed configures cleanly"
else
    fail "the corrected seed did not configure (rc=$F_RC):
$CONFIGURE_OUT"
fi
run_build "$TEST_TMPDIR/bad-build"
check
if [ "$BUILD_RERUNS" -eq 0 ] && [ "$BUILD_USED" = "real" ]; then
    log_success "and builds with it, no further configure"
else
    fail "used='$BUILD_USED' re-runs=$BUILD_RERUNS (want real / 0).
$BUILD_OUT"
fi

# ---------------------------------------------------------------------------
log_header "G. a configure-time header heal runs after the cargo re-key (issue 1746)"
# ---------------------------------------------------------------------------
#
# The same two-step shape one seam over. `<build>/cargo` is keyed provisionally
# at `nros-c`'s scope and completed by the deferred `_nros_entity_facts_flush`
# (issue 1700). The sizes-header heal ran at `nros-cpp`'s scope, in between, so
# on a re-configure it copied the PROVISIONAL key's header over the right one
# and stamped it newer than the archive -- the first build linked
# `undefined reference to nros_config_variant_sz_*`. The heal is queued now
# (`nros_config_header_heal`) and runs after the re-key. This runs the REAL
# flush and the REAL mirror script; only the re-key itself is stood in for.
# The immediate run beside it is the pre-fix shape, kept as the control that
# shows this miniature can see the defect at all.
#
# Issue 1824 -- the mirror reads only a PLACED source now (`-` gen-subdir),
# never `<build>/cargo/*/`, so the provisional key no longer reaches the heal
# through the cargo link. The queue is still the heal's road, and this case
# still pins its ORDER: the stand-in re-key rewrites the placed copy, so a
# heal that ran before it reads PROVISIONAL.

FACTS_MODULE="$PROJECT_ROOT/cmake/NanoRosEntityFacts.cmake"
MIRROR_SH="$PROJECT_ROOT/scripts/build/mirror-generated-header.sh"
HEAL_ROOT="$TEST_TMPDIR/heal"
mkdir -p "$HEAL_ROOT/src"
cat > "$HEAL_ROOT/src/CMakeLists.txt" <<EOF
cmake_minimum_required(VERSION 3.20)
project(nros_header_heal_probe NONE)
include("$FACTS_MODULE")

# nros-c's scope: the placed copy as the PROVISIONAL key's cargo run left it.
file(WRITE "\${CMAKE_BINARY_DIR}/placed/h.h" "PROVISIONAL\n")

# The deferred flush's re-key, stood in for (the real one needs a whole
# Corrosion import); the flush that CALLS it is the real one.
function(_nros_entity_facts_rekey_shared_cargo_dir)
    file(WRITE "\${CMAKE_BINARY_DIR}/placed/h.h" "FINAL\n")
endfunction()
# nros-cpp's scope: queue the heal, exactly as its CMakeLists does.
nros_config_header_heal("$MIRROR_SH" "\${CMAKE_BINARY_DIR}/placed/h.h"
    "\${CMAKE_BINARY_DIR}" - h.h "\${CMAKE_BINARY_DIR}/include/nros/h.h")

# An entry registers LAST in a real configure, so the facts flush is scheduled
# AFTER the heal was queued: its deferred call runs first and must wait.
nros_entity_facts_env_deferred(nros_heal_probe_target)

# The pre-fix shape, for contrast: run it NOW.
execute_process(COMMAND bash "$MIRROR_SH" "\${CMAKE_BINARY_DIR}/placed/h.h"
    "\${CMAKE_BINARY_DIR}" - h.h "\${CMAKE_BINARY_DIR}/immediate/h.h")
EOF
CONFIGURE_OUT="$(cmake -G Ninja -S "$HEAL_ROOT/src" -B "$HEAL_ROOT/build" 2>&1)"
check
if [ "$(cat "$HEAL_ROOT/build/immediate/h.h" 2>/dev/null)" = "PROVISIONAL" ]; then
    log_success "control: an immediate heal reads the provisional copy (the defect is visible here)"
else
    fail "control did not reproduce the defect, so the case below proves nothing:
  immediate='$(cat "$HEAL_ROOT/build/immediate/h.h" 2>/dev/null)'
$CONFIGURE_OUT"
fi
check
if [ "$(cat "$HEAL_ROOT/build/include/nros/h.h" 2>/dev/null)" = "FINAL" ]; then
    log_success "the queued heal ran after the re-key and mirrored the FINAL placed header"
else
    fail "THE HEAL READ A PROVISIONAL KEY (issue 1746): got
  '$(cat "$HEAL_ROOT/build/include/nros/h.h" 2>/dev/null)', want FINAL.
$CONFIGURE_OUT"
fi

# ---------------------------------------------------------------------------
log_header "Case T -- the producer READS the resolve only over its own inputs (phase-439 W7)"
# ---------------------------------------------------------------------------
#
# `nros_resolved_entity_take` is what lets the mid-configure producer copy
# stage 3.5's answer instead of recomposing (issue 1228). Its safety property is
# case C's, one level up: an answer composed over DIFFERENT inputs must never be
# taken. So: a match copies byte for byte, and each input moved alone declines
# and names that input. The registered-key rule (dedup on pkg::component with
# the last row winning, the class-prefix package fallback, sorted) is pinned to
# a literal here AND in `resolve::tests::entity_inputs_key_the_registered_rows`,
# so the Rust writer and this reader cannot drift apart unseen.
TAKE_ROOT="$TEST_TMPDIR/take"
mkdir -p "$TAKE_ROOT/resolve" "$TAKE_ROOT/ws/src" "$TAKE_ROOT/model"
printf 'set(ANSWER resolved)\n' > "$TAKE_ROOT/resolve/resolved.cmake"
printf '{"source":"resolve"}\n' > "$TAKE_ROOT/resolve/resolved-entity-inventory.json"
: > "$TAKE_ROOT/model/system_model.yaml"
REG='b_pkg::sub::b_pkg::Sub2;core::bare::core::Bare;z_pkg::talker::z_pkg::Talker'
write_take_record() {
    cat > "$TAKE_ROOT/resolve/resolved-entity-inputs.cmake" <<EOF
set(NROS_RESOLVED_ENTITY_INPUTS_SCHEMA ${1:-1})
set(NROS_RESOLVED_ENTITY_REGISTERED [==[${2:-$REG}]==])
set(NROS_RESOLVED_ENTITY_MODEL [==[$TAKE_ROOT/model/system_model.yaml]==])
set(NROS_RESOLVED_ENTITY_MODEL_SHA256 $(sha256sum "$TAKE_ROOT/model/system_model.yaml" | cut -d' ' -f1))
set(NROS_RESOLVED_ENTITY_WORKSPACE [==[$TAKE_ROOT/ws]==])
set(NROS_RESOLVED_ENTITY_RX_ARGS [==[${3:-}]==])
EOF
}
# Three rows plus a duplicate of b_pkg::sub (the later one wins), and one row
# with no `pkg` (its package is the class prefix).
cat > "$TAKE_ROOT/metadata.json" <<'EOF'
{"components": [
  {"name": "talker", "pkg": "z_pkg", "class": "z_pkg::Talker"},
  {"name": "sub", "pkg": "b_pkg", "class": "b_pkg::Sub1"},
  {"name": "bare", "class": "core::Bare"},
  {"name": "sub", "pkg": "b_pkg", "class": "b_pkg::Sub2"}
]}
EOF
mkdir -p "$TAKE_ROOT/src"
cat > "$TAKE_ROOT/src/CMakeLists.txt" <<EOF
cmake_minimum_required(VERSION 3.20)
project(nros_take_probe NONE)
include("$RESOLVED_MODULE")
nros_configured_publish("$TAKE_ROOT/resolve")
if(PROBE_CONFIGURED)
    set_property(GLOBAL PROPERTY NROS_CONFIGURED_CURRENT TRUE)
endif()
if(NOT PROBE_MODELS)
    set(PROBE_MODELS "$TAKE_ROOT/model/system_model.yaml")
endif()
foreach(_m IN LISTS PROBE_MODELS)
    set_property(GLOBAL APPEND PROPERTY NROS_ENTITY_INVENTORY_MODELS "\${_m}")
endforeach()
if(NOT DEFINED PROBE_WS)
    set(PROBE_WS "$TAKE_ROOT/ws")
endif()
set_property(GLOBAL PROPERTY NROS_ENTITY_INVENTORY_WORKSPACE "\${PROBE_WS}")
_nros_resolved_registered_keys("$TAKE_ROOT/metadata.json" _keys _kwhy)
message(STATUS "PROBE_KEYS=\${_keys}")
nros_resolved_entity_take("$TAKE_ROOT/metadata.json" "\${CMAKE_BINARY_DIR}/frag.cmake"
    "\${CMAKE_BINARY_DIR}/inv.json" _taken)
message(STATUS "PROBE_TAKEN=\${_taken}")
EOF
take_run() { # <label> [cmake -D args...]
    rm -rf "$TAKE_ROOT/build"
    # The record's digest in the environment, as `nros build` hands it to the
    # configures it starts; TAKE_NO_ENV=1 is a configure it did not start.
    local env_sha=""
    if [ -z "${TAKE_NO_ENV:-}" ]; then
        env_sha="$(sha256sum "$TAKE_ROOT/resolve/resolved-entity-inputs.cmake" | cut -d' ' -f1)"
    fi
    TAKE_OUT="$(NROS_RESOLVED_ENTITY_RECORD_SHA256="$env_sha" \
        cmake -S "$TAKE_ROOT/src" -B "$TAKE_ROOT/build" "${@:2}" 2>&1)"
}
write_take_record
take_run match
check
if nros_grep_q -F "PROBE_KEYS=$REG" <<<"$TAKE_OUT"; then
    log_success "registered keys: deduplicated last-wins, class-prefix fallback, sorted"
else
    fail "the registered-key rule moved; want PROBE_KEYS=$REG
$TAKE_OUT"
fi
check
if nros_grep_q -F "PROBE_TAKEN=TRUE" <<<"$TAKE_OUT" \
   && cmp -s "$TAKE_ROOT/resolve/resolved.cmake" "$TAKE_ROOT/build/frag.cmake" \
   && cmp -s "$TAKE_ROOT/resolve/resolved-entity-inventory.json" "$TAKE_ROOT/build/inv.json"; then
    log_success "inputs match: the resolve's fragment and JSON are taken byte for byte"
else
    fail "a matching record was not taken verbatim:
$TAKE_OUT"
fi

take_declines() { # <what> <expected reason fragment> [cmake -D args...]
    take_run "$1" "${@:3}"
    check
    if nros_grep_q -F "PROBE_TAKEN=FALSE" <<<"$TAKE_OUT" && nros_grep_q -F "$2" <<<"$TAKE_OUT" \
       && [ ! -e "$TAKE_ROOT/build/frag.cmake" ]; then
        log_success "$1: declined, naming it"
    else
        fail "$1: an answer composed over other inputs was taken, or the reason is missing
  (want '$2'):
$TAKE_OUT"
    fi
}
write_take_record 1 'b_pkg::sub::b_pkg::Sub2;z_pkg::talker::z_pkg::Talker'
take_declines "a component registered since the resolve" "re-run \`nros configure\`"
write_take_record
: > "$TAKE_ROOT/model/other.yaml"
take_declines "a second model (a shared runtime)" "composes over 2 models" \
    "-DPROBE_MODELS=$TAKE_ROOT/model/system_model.yaml;$TAKE_ROOT/model/other.yaml"
take_declines "another model" "it was composed over" "-DPROBE_MODELS=$TAKE_ROOT/model/other.yaml"
take_declines "no workspace" "its workspace is" "-DPROBE_WS="
write_take_record 1 "$REG" "--board-rx-frame-cost-us;40"
take_declines "another board receive cost" "board receive cost"
write_take_record
TAKE_NO_ENV=1 take_declines "a configure no current \`nros configure\` is in front of" \
    "no current \`nros configure\` wrote"
# phase-530 W4 -- the module hook ran `nros configure` in THIS configure: the
# record is current, with no digest in the environment.
TAKE_NO_ENV=1 take_run hook -DPROBE_CONFIGURED=ON
check
if nros_grep_q -F "PROBE_TAKEN=TRUE" <<<"$TAKE_OUT"; then
    log_success "a configure the module hook configured takes the record with no environment digest"
else
    fail "the hook's current configure did not count as current:
$TAKE_OUT"
fi
write_take_record 2
take_declines "an unknown record schema" "schema this reader does not know"
# The model re-resolved IN PLACE (same path, new content) -- what a configure
# `nros build` did not start does after a contract edit.
write_take_record
printf 'edited\n' > "$TAKE_ROOT/model/system_model.yaml"
take_declines "the model edited in place" "changed since \`nros build\` resolved it"
: > "$TAKE_ROOT/model/system_model.yaml"

# ---------------------------------------------------------------------------
log_header "Case U -- a seeded message-bound answer is held to, not re-converged (phase-439 W8)"
# ---------------------------------------------------------------------------
#
# RFC-0120 D5: the readers of the message-bound fragment read configure's seed;
# a producer that composes a different FINAL answer stops the configure. Over
# the SAME inputs (tables, entity inventory, ceiling --
# `resolved-message-bounds-inputs.cmake`) it names both files as the two
# composers' disagreement; over DIFFERENT inputs it names the input that moved,
# which is a declaration configure could not see. Never re-converged.
PROM_ROOT="$TEST_TMPDIR/promise"
mkdir -p "$PROM_ROOT/resolve" "$PROM_ROOT/src"
printf 'set(BOUND seeded)\n' > "$PROM_ROOT/resolve/resolved-message-bounds.cmake"
printf '{"types": []}\n' > "$PROM_ROOT/table.json"
printf 'set(ENTITY x)\n' > "$PROM_ROOT/entity.cmake"
write_prom_record() { # [table sha] [ceiling] [tables override]
    local tsha esha
    tsha="${1:-$(sha256sum "$PROM_ROOT/table.json" | cut -d' ' -f1)}"
    esha="$(sha256sum "$PROM_ROOT/entity.cmake" | cut -d' ' -f1)"
    cat > "$PROM_ROOT/resolve/resolved-message-bounds-inputs.cmake" <<EOF
set(NROS_RESOLVED_BOUNDS_INPUTS_SCHEMA 1)
set(NROS_RESOLVED_BOUNDS_TABLES [==[${3:-std_msgs=$tsha}]==])
set(NROS_RESOLVED_BOUNDS_ENTITY_SHA256 $esha)
set(NROS_RESOLVED_BOUNDS_CEILING ${2:-2048})
EOF
}
cat > "$PROM_ROOT/src/CMakeLists.txt" <<EOF
cmake_minimum_required(VERSION 3.20)
project(nros_promise_probe NONE)
include("$PROJECT_ROOT/cmake/NanoRosEntityInventory.cmake")
include("$PROJECT_ROOT/cmake/NanoRosMessageBounds.cmake")
include("$RESOLVED_MODULE")
nros_configured_publish("$PROM_ROOT/resolve")
nros_entity_inventory_knobs_file(_e)
configure_file("$PROM_ROOT/entity.cmake" "\${_e}" COPYONLY)
nros_message_bounds_knobs_file(_frag)
# ---- the READER (the Zephyr module's order) ----
nros_resolved_seed_message_bounds("\${_frag}")
nros_promise_read("\${_frag}")
# ---- one interface package registers its table ----
set(_t "\${CMAKE_BINARY_DIR}/codegen/std_msgs")
file(MAKE_DIRECTORY "\${_t}")
file(WRITE "\${_t}/nros_message_bounds.cmake" "")
configure_file("$PROM_ROOT/table.json" "\${_t}/nros_message_bounds.json" COPYONLY)
nros_message_bounds_register_fragment("\${_t}/nros_message_bounds.cmake")
foreach(_n IN LISTS PROBE_EXTRA_TABLES)
    set(_t2 "\${CMAKE_BINARY_DIR}/codegen/\${_n}")
    file(MAKE_DIRECTORY "\${_t2}")
    file(WRITE "\${_t2}/nros_message_bounds.cmake" "")
    configure_file("$PROM_ROOT/table.json" "\${_t2}/nros_message_bounds.json" COPYONLY)
    nros_message_bounds_register_fragment("\${_t2}/nros_message_bounds.cmake")
endforeach()
# The entity fragment the producer reads; PROBE_ENTITY_LAG composes over an
# older answer that the entity producer then replaces, later in the configure.
if(PROBE_ENTITY_LAG)
    file(WRITE "\${_e}" "set(ENTITY older)\n")
endif()
# ---- the PRODUCER ----
nros_message_bounds_note_composition("\${_e}" 2048)
nros_promise_snapshot("\${_frag}" _before)
file(WRITE "\${_frag}" "set(BOUND \${PROBE_ANSWER})\n")
nros_promise_check("\${_frag}" "\${_before}" LABEL "the probe bounds")
# ---- the ENTITY producer, after the last bounds call ----
configure_file("$PROM_ROOT/entity.cmake" "\${_e}" COPYONLY)
EOF
prom_run() { # [cmake -D args...]
    rm -rf "$PROM_ROOT/build"
    PROM_RC=0
    PROM_OUT="$(cmake -S "$PROM_ROOT/src" -B "$PROM_ROOT/build" "$@" 2>&1)" || PROM_RC=$?
}
write_prom_record
prom_run -DPROBE_ANSWER=seeded
check
if [ "$PROM_RC" -eq 0 ] && ! nros_grep_q -F "changed after the readers" <<<"$PROM_OUT"; then
    log_success "producer agrees with the seed: one pass, nothing armed"
else
    fail "an agreeing producer did not configure cleanly in one pass (rc=$PROM_RC):
$PROM_OUT"
fi
prom_run -DPROBE_ANSWER=other
check
if [ "$PROM_RC" -ne 0 ] && nros_grep_q -F "over the same inputs" <<<"$PROM_OUT"; then
    log_success "same inputs, different answer: the configure STOPS naming both files"
else
    fail "a disagreement over the same inputs did not stop the configure (rc=$PROM_RC):
$PROM_OUT"
fi
write_prom_record "0000000000000000000000000000000000000000000000000000000000000000"
prom_run -DPROBE_ANSWER=other
check
if [ "$PROM_RC" -ne 0 ] && nros_grep_q -F "could not see" <<<"$PROM_OUT" \
   && nros_grep_q -F "configure composed over the bound tables" <<<"$PROM_OUT" \
   && ! nros_grep_q -F "changed after the readers" <<<"$PROM_OUT"; then
    log_success "a moved table: the configure stops naming the table set, never re-converges"
else
    fail "a table configure did not see did not stop the configure by name (rc=$PROM_RC):
$PROM_OUT"
fi
write_prom_record "" 4096
prom_run -DPROBE_ANSWER=other
check
if [ "$PROM_RC" -ne 0 ] && nros_grep_q -F "small-class ceiling is 4096" <<<"$PROM_OUT"; then
    log_success "another ceiling: stops, naming the ceiling"
else
    fail "a ceiling difference was not named (rc=$PROM_RC):
$PROM_OUT"
fi
write_prom_record
prom_run -DPROBE_ANSWER=other -DPROBE_ENTITY_LAG=ON
check
if [ "$PROM_RC" -ne 0 ] && nros_grep_q -F "entity inventory is not the one" <<<"$PROM_OUT"; then
    log_success "bounds composed over another entity answer: stops, naming the entity inventory"
else
    fail "the entity-inventory input was not named (rc=$PROM_RC):
$PROM_OUT"
fi
# Two tables whose names sort differently from their formatted rows (`foo`
# before `foo2` by name, `foo2=` before `foo=` as text): the record is written
# in the row order `write_west_seed` uses, and the FATAL must stay reachable.
_tsha="$(sha256sum "$PROM_ROOT/table.json" | cut -d' ' -f1)"
write_prom_record "" "" "foo2=$_tsha;foo=$_tsha;std_msgs=$_tsha"
prom_run -DPROBE_ANSWER=other "-DPROBE_EXTRA_TABLES=foo;foo2"
check
if [ "$PROM_RC" -ne 0 ] && nros_grep_q -F "over the same inputs" <<<"$PROM_OUT"; then
    log_success "tables foo + foo2: the record's row order matches, and a disagreement still stops the configure"
else
    fail "with tables foo and foo2 the record never matches, so the check cannot fire (rc=$PROM_RC):
$PROM_OUT"
fi
rm -f "$PROM_ROOT/resolve/resolved-message-bounds-inputs.cmake"
prom_run -DPROBE_ANSWER=other
check
if [ "$PROM_RC" -ne 0 ] && nros_grep_q -F "over the same inputs" <<<"$PROM_OUT"; then
    log_success "no input record: still a promise (the readers read the seed), named as a disagreement"
else
    fail "a seeded configure with no input record was not held to the seed (rc=$PROM_RC):
$PROM_OUT"
fi

# A table list in the FATAL keeps its separators (`foo2=..;foo=..`), so a
# reader can tell three tables from one long name.
write_prom_record "" "" "foo=$_tsha;std_msgs=$_tsha"
prom_run -DPROBE_ANSWER=other "-DPROBE_EXTRA_TABLES=foo;foo2"
check
if [ "$PROM_RC" -ne 0 ] && nros_grep_q -F "[foo=$_tsha;std_msgs=$_tsha]" <<<"$PROM_OUT" \
   && nros_grep_q -F "[foo2=$_tsha;foo=$_tsha;std_msgs=$_tsha]" <<<"$PROM_OUT"; then
    log_success "the FATAL names both table sets with their separators"
else
    fail "the table sets lost their separators in the FATAL (rc=$PROM_RC):
$PROM_OUT"
fi

# ---------------------------------------------------------------------------
log_header "S. the cmake road locates a STALE configure: no seed, no promise"
# ---------------------------------------------------------------------------
#
# Phase-530 review I3 (Ruling R33). `nros build` passes the image's IDENTITY
# (`-DNROS_CONFIGURED_PROJECT/IMAGE`), and the cache keeps it. An edit to the
# project (a new `find_package(<msg>)`) then `cmake --build` re-runs cmake with
# no `nros configure` in front: the stamp is STALE, and a seed composed before
# the edit must not be promised. `nros locate configured` says so
# (`NROS_LOCATE_CURRENT`), and the configure falls to the unseeded road: its
# readers use their defaults, and it says what to run -- never a re-converge
# (the fixed point is deleted, phase-530 W7).
FAKE_CLI="$TEST_TMPDIR/fake-nros"
cat > "$FAKE_CLI" <<'EOF'
#!/usr/bin/env bash
# Stands in for `nros`: logs its argv; answers `configure` and `locate`.
printf '%s\n' "$*" >> "$FAKE_NROS_LOG"
case "$1" in
    configure) echo "configure: up to date (1 image(s))" ;;
    locate)
        echo "set(NROS_LOCATE_CONFIGURED \"$FAKE_NROS_RESOLVED\")"
        echo "set(NROS_LOCATE_RESOLVED \"$FAKE_NROS_RESOLVED\")"
        echo "set(NROS_LOCATE_CURRENT $FAKE_NROS_CURRENT)"
        echo "set(NROS_LOCATE_STALE \"inputs changed\")"
        ;;
esac
EOF
chmod +x "$FAKE_CLI"
export NROS_CLI="$FAKE_CLI" FAKE_NROS_LOG="$TEST_TMPDIR/fake-nros.log"
RESOLVED_STALE="$TEST_TMPDIR/resolved-stale"
mkdir -p "$RESOLVED_STALE"
printf 'set(ANSWER stale_seed)\n' > "$RESOLVED_STALE/resolved.cmake"
export FAKE_NROS_RESOLVED="$RESOLVED_STALE"

FAKE_NROS_CURRENT=FALSE
export FAKE_NROS_CURRENT
STALE_RC=0
CONFIGURE_OUT="$(cmake -G Ninja -S "$SRC" -B "$TEST_TMPDIR/stale-build" \
    -DNROS_CONFIGURED_PROJECT=/proj -DNROS_CONFIGURED_IMAGE=img 2>&1)" || STALE_RC=$?
check
if [ "$STALE_RC" -eq 0 ] && nros_grep_q 'PROBE_READ=placeholder' <<<"$CONFIGURE_OUT" \
   && nros_grep_q -F 'nros configure /proj --image img' <<<"$CONFIGURE_OUT"; then
    log_success "a stale configure seeds nothing, and the configure says what to run"
else
    fail "A STALE CONFIGURE WAS PROMISED (rc=$STALE_RC): the cmake road must seed only
  from a CURRENT stamp, and name \`nros configure\` when it is not.
$CONFIGURE_OUT"
fi
run_build "$TEST_TMPDIR/stale-build"
check
if [ "$BUILD_USED" = "placeholder" ] && [ "$BUILD_RC" -eq 0 ] && [ "$BUILD_RERUNS" -eq 0 ]; then
    log_success "and the build uses the defaults with 0 re-configures"
else
    fail "a stale configure re-ran or failed: used='$BUILD_USED' rc=$BUILD_RC re-runs=$BUILD_RERUNS (want placeholder / 0 / 0).
$BUILD_OUT"
fi

# Control: the same seed from a CURRENT stamp is promised, and fails as C did.
FAKE_NROS_CURRENT=TRUE
export FAKE_NROS_CURRENT
CUR_RC=0
CONFIGURE_OUT="$(cmake -G Ninja -S "$SRC" -B "$TEST_TMPDIR/current-build" \
    -DNROS_CONFIGURED_PROJECT=/proj -DNROS_CONFIGURED_IMAGE=img 2>&1)" || CUR_RC=$?
check
if [ "$CUR_RC" -ne 0 ] && nros_grep_q 'PROBE_READ=stale_seed' <<<"$CONFIGURE_OUT"; then
    log_success "control: a CURRENT stamp's seed is read and promised"
else
    fail "control: a current stamp's seed was not promised (rc=$CUR_RC):
$CONFIGURE_OUT"
fi

# ---------------------------------------------------------------------------
log_header "W. the Zephyr hook's project: a leaf under a \`src/\` directory is itself"
# ---------------------------------------------------------------------------
#
# Phase-530 review I1 (Ruling R31). RFC-0085 D14's walk-up to `<ws>` is a
# property of the GENERATED application (it states NROS_IMAGE_SYSTEM_DIR), not
# of a directory's name: a standalone leaf at `~/ros_ws/src/my_leaf/` is its
# own project.
HOOK_WS="$TEST_TMPDIR/hook-ws"
mkdir -p "$HOOK_WS/src/my_leaf" "$HOOK_WS/src/bringup"
# The hook runs only for a dir holding a system.toml (a guest is unchanged --
# `nros_configure_hook`, phase-530 W5), as both of these do in a real tree.
touch "$HOOK_WS/src/my_leaf/system.toml" "$HOOK_WS/src/bringup/system.toml"
FAKE_NROS_CURRENT=TRUE
export FAKE_NROS_CURRENT
hook_project() { # <dir> <generated> -> the project `nros configure` was run on
    : > "$FAKE_NROS_LOG"
    cat > "$TEST_TMPDIR/hook-project.cmake" <<EOF
include("$PROJECT_ROOT/zephyr/cmake/nros_image_kconfig.cmake")
_nros_image_kconfig_configure("$FAKE_CLI" "\${DIR}" img "\${GEN}")
EOF
    cmake -DDIR="$1" -DGEN="$2" -P "$TEST_TMPDIR/hook-project.cmake" >/dev/null 2>&1
    sed -n 's/^configure \([^ ]*\) --image img$/\1/p' "$FAKE_NROS_LOG" | head -1
}
check
got="$(hook_project "$HOOK_WS/src/my_leaf" FALSE)"
if [ "$got" = "$HOOK_WS/src/my_leaf" ]; then
    log_success "a standalone leaf under src/ configures itself"
else
    fail "a standalone leaf under src/ configured '$got', not itself ($HOOK_WS/src/my_leaf)"
fi
check
got="$(hook_project "$HOOK_WS/src/bringup" TRUE)"
if [ "$got" = "$HOOK_WS" ]; then
    log_success "a generated application's bringup configures its workspace"
else
    fail "a generated application's bringup configured '$got', not its workspace ($HOOK_WS)"
fi
unset NROS_CLI


# ---------------------------------------------------------------------------
log_header "R. the Zephyr hook applies the image's declared ROS edition"
# ---------------------------------------------------------------------------
#
# Phase-530 review round 2 (Ruling R34). `nros configure` seeds a leaf's bound
# tables for its DECLARED `ros_edition`; the Zephyr road must build the host
# for the same edition, or the current stamp's seed disagrees with the host
# and the promise FATALs with a hint nobody can follow. The module hook (the
# real one, `zephyr/modules/modules.cmake`, against a stand-in CLI) applies it
# as `NROS_ROS_EDITION`, and refuses a `-D` that names another (issue 2087's
# rule, the cmake road's). The same harness holds item 2: the hook marks its
# configure CURRENT only when `nros locate configured` said so.
ED_ROOT="$TEST_TMPDIR/edition"
mkdir -p "$ED_ROOT/app"
cat > "$ED_ROOT/fake-nros" <<'EOF'
#!/usr/bin/env bash
case "$1 $2 $3" in
    "dev image leaf-system")
        while [ $# -gt 0 ]; do
            case "$1" in --kconfig-defaults-out) : > "$2"; shift ;; esac
            shift
        done
        echo "NROS_LEAF_IMAGE=img"
        echo "NROS_LEAF_ROS_EDITION=$FAKE_EDITION"
        exit 0 ;;
esac
case "$1" in
    configure) echo "configure: up to date (1 image(s))" ;;
    locate)
        echo 'set(NROS_LOCATE_RESOLVED "")'
        echo "set(NROS_LOCATE_CURRENT $FAKE_NROS_CURRENT)" ;;
esac
EOF
chmod +x "$ED_ROOT/fake-nros"
printf '[system]\nname = "x"\n[image.img]\nboard = "native_sim"\n' > "$ED_ROOT/app/system.toml"
cat > "$ED_ROOT/app/CMakeLists.txt" <<EOF
cmake_minimum_required(VERSION 3.20)
project(nros_hook_edition_probe NONE)
set(APPLICATION_SOURCE_DIR "\${CMAKE_CURRENT_SOURCE_DIR}")
set(APPLICATION_BINARY_DIR "\${CMAKE_BINARY_DIR}")
# Where zephyr_module.cmake includes it: the application's own scope.
include("$PROJECT_ROOT/zephyr/modules/modules.cmake")
include("$PROJECT_ROOT/cmake/NanoRosRosEdition.cmake")
_nros_resolve_ros_edition("" _e)
message(STATUS "HOST_EDITION=\${_e}")
get_property(_c GLOBAL PROPERTY NROS_CONFIGURED_CURRENT)
message(STATUS "HOOK_CURRENT=\${_c}")
EOF
ed_run() { # <declared> <current> [cmake -D args...]
    local declared="$1" current="$2"
    shift 2
    rm -rf "$ED_ROOT/build"
    ED_RC=0
    ED_OUT="$(NROS_CLI="$ED_ROOT/fake-nros" FAKE_EDITION="$declared" \
        FAKE_NROS_CURRENT="$current" \
        cmake -S "$ED_ROOT/app" -B "$ED_ROOT/build" -DNROS_IMAGE=img "$@" 2>&1)" || ED_RC=$?
}
ed_run jazzy TRUE
check
if [ "$ED_RC" -eq 0 ] && nros_grep_q -F 'HOST_EDITION=jazzy' <<<"$ED_OUT"; then
    log_success "a declared jazzy image builds its host for jazzy"
else
    fail "a declared jazzy image did not set the host edition (rc=$ED_RC):
$ED_OUT"
fi
ed_run jazzy TRUE -DNROS_ROS_EDITION=humble
check
if [ "$ED_RC" -ne 0 ] && nros_grep_q -F '`jazzy`' <<<"$ED_OUT" \
   && nros_grep_q -F 'NROS_ROS_EDITION=humble' <<<"$ED_OUT" \
   && nros_grep_q -F 'issue 2087' <<<"$ED_OUT"; then
    log_success "a -D naming another edition is refused, naming both"
else
    fail "a disagreeing -DNROS_ROS_EDITION was not refused by name (rc=$ED_RC):
$ED_OUT"
fi
ed_run jazzy TRUE -DNROS_ROS_EDITION=jazzy
check
if [ "$ED_RC" -eq 0 ] && nros_grep_q -F 'HOST_EDITION=jazzy' <<<"$ED_OUT"; then
    log_success "an agreeing -D is accepted"
else
    fail "an agreeing -DNROS_ROS_EDITION was refused (rc=$ED_RC):
$ED_OUT"
fi
ed_run "" TRUE
check
if [ "$ED_RC" -eq 0 ] && nros_grep_q -F 'HOST_EDITION=humble' <<<"$ED_OUT" \
   && nros_grep_q -F 'HOOK_CURRENT=TRUE' <<<"$ED_OUT"; then
    log_success "no declaration: the default; a current locate marks the configure current"
else
    fail "an undeclared image did not keep the default, or a current locate was not marked (rc=$ED_RC):
$ED_OUT"
fi
ed_run "" FALSE
check
if [ "$ED_RC" -eq 0 ] && nros_grep_q -F 'HOOK_CURRENT=' <<<"$ED_OUT" \
   && ! nros_grep_q -F 'HOOK_CURRENT=TRUE' <<<"$ED_OUT"; then
    log_success "a stale locate does not mark the hook's configure current"
else
    fail "the hook marked its configure CURRENT over a stale locate (rc=$ED_RC):
$ED_OUT"
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
