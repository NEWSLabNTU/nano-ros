#!/usr/bin/env bash
# check-px4-archive-header-pairing.sh — issues 1046 / 1050, phase-424.
#
# TWO claims, and the second is the one that rots:
#
#   1. WIRING — `NanoRosPx4Module.cmake` asserts the generated headers PAIR with
#      the archive it links, and does not guard a generated header with a
#      directory test. (Static; a grep.)
#
#   2. THE PREDICATE ACTUALLY FIRES — the pairing function rejects a mismatched
#      pair, a missing header, a stampless header, and the exact shape issue 1046
#      was: a generated DIRECTORY that survived a build which wrote nothing into
#      it. (Dynamic; runs the real cmake function against synthetic fixtures.)
#
# Claim 2 is the whole point. Issue 1046 was a guard whose MESSAGE was a correct
# and complete diagnosis and whose PREDICATE could not observe the failure — it
# passed on exactly the tree it existed to reject. A gate for that class which is
# itself never shown to fail would be the same bug wearing a gate's costume. So
# every case below runs on the normal path, positive and negative, and a fixture
# that stops failing fails THIS script.
#
# Needs no PX4 tree, no cargo build, no toolchain: `cmake -P` plus files this
# script writes. Measured cost: 0.12 s for all eleven cases.
#
# NOTE on `scan_consumer <f> | grep -q <pat>`: don't. Under `set -o pipefail`
# that idiom is BACKWARDS — `grep -q` exits at the first match, the writer dies
# of SIGPIPE (141), and pipefail reports the pipeline as FAILED precisely when
# the pattern WAS found. Two of the wiring cases below "failed" that way while
# the rule was working, and the two that passed did so by luck (one emits its
# finding late enough to finish writing; one matches nothing, so grep returns 1
# and the writer survives). Capture into a variable, then match. Same conflation
# `check-grep-q-error-conflation` gates elsewhere in this tree.

set -uo pipefail

# issue 0726 — `grep -q` cannot distinguish a tool ERROR (exit >=2) from a
# NON-MATCH (exit 1), and the two natural spellings fail in OPPOSITE directions.
# `nros_grep_q` exits 2 on a tool failure rather than reporting a finding, which
# for a gate whose entire output is findings is the difference between a claim
# and a guess. Caught by `check-grep-q-error-conflation` on this very file.
# shellcheck source=scripts/lib/grep-q.sh
. "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/lib/grep-q.sh"

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
MODULE="$ROOT/integrations/px4/NanoRosArchivePairing.cmake"
CONSUMER="$ROOT/integrations/px4/NanoRosPx4Module.cmake"

fail=0
note() { printf '  %s\n' "$*"; }
bad() { printf 'FAIL: %s\n' "$*" >&2; fail=1; }

# ---------------------------------------------------------------------------
# The harness: run the real function, report only pass/fail.
# ---------------------------------------------------------------------------
#
# A subprocess because the predicate's failure mode is `message(FATAL_ERROR)`,
# which cannot be caught in-process. That is deliberate in the module — a
# recoverable warning is how "configure said something" becomes "nobody read it".
run_case() { # run_case <archive> <header> <prefix>  -> rc 0 pass / non-zero fail
    cmake \
        -DNAP_MODULE="$MODULE" \
        -DCASE_ARCHIVE="$1" \
        -DCASE_HEADER="$2" \
        -DCASE_PREFIX="$3" \
        -P "$TMP/run_case.cmake" >"$TMP/out.txt" 2>&1
}

write_harness() {
    cat >"$TMP/run_case.cmake" <<'EOF'
include("${NAP_MODULE}")
nros_assert_archive_pairs_with_header(
    ARCHIVE       "${CASE_ARCHIVE}"
    HEADER        "${CASE_HEADER}"
    SYMBOL_PREFIX "${CASE_PREFIX}"
    BUILD_HINT    "cargo build -p nros-cpp --release")
EOF
}

# A header shaped like the real one (nros-build-helpers, issue 0360).
write_header() { # write_header <path> <symbol>
    mkdir -p "$(dirname "$1")"
    cat >"$1" <<EOF
/* synthetic fixture — mirrors nros_cpp_config_generated.h */
#define NROS_CPP_CONFIG_VARIANT "$2"
extern const unsigned char nros_cpp_config_variant_$2;
__attribute__((used, unused))
static const unsigned char *const nros__cpp_config_variant_anchor =
    &nros_cpp_config_variant_$2;
EOF
}

# An "archive" is only ever byte-scanned, so a file containing the symbol is a
# faithful stand-in. The scan is validated against a REAL 25 MB libnros_cpp.a in
# the module's own header comment; what is under test here is the predicate.
write_archive() { # write_archive <path> <symbol...>
    local out="$1"; shift
    mkdir -p "$(dirname "$out")"
    : >"$out"
    printf 'ELF\0\0\0garbage\0' >>"$out"
    for s in "$@"; do printf 'nros_cpp_config_variant_%s\0' "$s" >>"$out"; done
    printf '\0\0more binary noise\0' >>"$out"
}

# ---------------------------------------------------------------------------
# Claim 2 — the negative control, on the normal path.
# ---------------------------------------------------------------------------
self_test() {
    TMP="$(mktemp -d)"
    trap 'rm -rf "$TMP"' RETURN
    write_harness

    local P="nros_cpp_config_variant_"
    local rc=0

    # (a) POSITIVE. Without this the whole file could be `exit 1` and read green
    #     in every negative case — a gate that only ever fails is as useless as
    #     one that only ever passes.
    write_header "$TMP/ok/nros_cpp_config_generated.h" "alloc_env_platform_posix_rmw_cffi_std"
    write_archive "$TMP/ok/lib.a" "alloc_env_platform_posix_rmw_cffi_std"
    if ! run_case "$TMP/ok/lib.a" "$TMP/ok/nros_cpp_config_generated.h" "$P"; then
        bad "a MATCHING header/archive pair was rejected"
        sed 's/^/      /' "$TMP/out.txt" >&2
        rc=1
    else
        note "[ok] a matching pair passes"
    fi

    # (b) THE 1050 MECHANISM: header and archive from different builds. This is
    #     the mismatch that produced `undefined reference to
    #     nros_cpp_config_variant_..._rmw_zenoh_cffi_...` ten minutes into a PX4
    #     build.
    write_header "$TMP/mix/nros_cpp_config_generated.h" "alloc_env_platform_posix_rmw_cffi_std"
    write_archive "$TMP/mix/lib.a" "alloc_default_env_panic_platform_rmw_cffi_rmw_zenoh_cffi_ros_humble_std"
    if run_case "$TMP/mix/lib.a" "$TMP/mix/nros_cpp_config_generated.h" "$P"; then
        bad "a MISMATCHED header/archive pair was ACCEPTED — the 1050 mechanism is unguarded"
        rc=1
    else
        nros_grep_q "DIFFERENT builds" "$TMP/out.txt" \
            || { bad "mismatch rejected, but the message does not name the cause"; rc=1; }
        note "[ok] a mismatched pair is rejected"
    fi

    # (c) THE 1046 SHAPE, exactly: the generated DIRECTORY exists and the header
    #     does not. This is what `IS_DIRECTORY` could not see, and it is not
    #     hypothetical — measured in the shared checkout on 2026-09-04, where
    #     target/nros-cpp-generated/ existed with the archive long gone.
    mkdir -p "$TMP/dironly/nros-cpp-generated/nros"
    write_archive "$TMP/dironly/lib.a" "alloc_env_platform_posix_rmw_cffi_std"
    if run_case "$TMP/dironly/lib.a" \
                "$TMP/dironly/nros-cpp-generated/nros/nros_cpp_config_generated.h" "$P"; then
        bad "a SURVIVING generated DIRECTORY with no header was ACCEPTED — this is issue 1046 verbatim"
        rc=1
    else
        note "[ok] a surviving generated dir with no header is rejected (the 1046 shape)"
    fi

    # (d) A header that exists and carries NO stamp. The pre-0360 header, and the
    #     checked-in stub. Accepting it would mean the pairing silently degrades
    #     to "a file is there" — issue 1046 one refinement down, which is how the
    #     0088-family keeps coming back.
    mkdir -p "$TMP/nostamp"
    printf '/* a header with no variant stamp */\n#define NROS_PUBLISHER_SIZE 64\n' \
        >"$TMP/nostamp/nros_cpp_config_generated.h"
    write_archive "$TMP/nostamp/lib.a" "alloc_env_platform_posix_rmw_cffi_std"
    if run_case "$TMP/nostamp/lib.a" "$TMP/nostamp/nros_cpp_config_generated.h" "$P"; then
        bad "a header with NO variant stamp was ACCEPTED — the pairing degraded to an existence test"
        rc=1
    else
        note "[ok] a header with no variant stamp is rejected"
    fi

    # (e) A missing archive, with a good header. The resolve step in
    #     NanoRosPx4Module.cmake catches this first in the real flow; the
    #     function must not depend on that, or moving it would open a hole.
    write_header "$TMP/noar/nros_cpp_config_generated.h" "alloc_env_platform_posix_rmw_cffi_std"
    if run_case "$TMP/noar/nope.a" "$TMP/noar/nros_cpp_config_generated.h" "$P"; then
        bad "a MISSING archive was ACCEPTED"
        rc=1
    else
        note "[ok] a missing archive is rejected"
    fi

    # (f) The header path is a DIRECTORY. `EXISTS` alone is true for a directory,
    #     so a bare existence test would pass this — the same conflation of
    #     *present* with *current* one level finer.
    mkdir -p "$TMP/isdir/nros_cpp_config_generated.h"
    write_archive "$TMP/isdir/lib.a" "alloc_env_platform_posix_rmw_cffi_std"
    if run_case "$TMP/isdir/lib.a" "$TMP/isdir/nros_cpp_config_generated.h" "$P"; then
        bad "a header path that is a DIRECTORY was ACCEPTED"
        rc=1
    else
        note "[ok] a header path that is a directory is rejected"
    fi

    return $rc
}

# ---------------------------------------------------------------------------
# Claim 1 — the wiring.
# ---------------------------------------------------------------------------
#
# CODE, not prose. The first version of this scanned raw lines and went red on
# its OWN fix — the comment explaining why `IS_DIRECTORY` is wrong for a
# generated path contains both words, so the rule fired on the sentence
# describing the rule. Left in, it would have taught the next person that
# documenting the reasoning breaks the build, which is how a rule ends up
# undocumented. Case (h) below is that exact line, kept as a control.
strip_cmake_comments() { sed -e 's/[[:space:]]*#.*$//' "$1"; }

# The rule, as a pure function of a file, so it can be run against fixtures as
# well as against the real consumer. A rule that can only be run on the one file
# it was written for cannot be shown to fire.
scan_consumer() { # scan_consumer <file> -> prints findings, one per line
    local f="$1" code
    code="$(strip_cmake_comments "$f")"

    nros_grep_q 'nros_assert_archive_pairs_with_header' <<<"$code" \
        || echo "does not call nros_assert_archive_pairs_with_header — the generated headers are unpaired again (issues 1046/1050)"

    # BOTH generated headers, not just the C++ one. Checking one is the
    # issue-0196 shape: coverage narrower than the rule it enforces. The C stamp
    # is a size hash and moves for reasons the C++ feature slug never sees.
    nros_grep_q 'nros_cpp_config_generated\.h' <<<"$code" \
        || echo "does not pair nros_cpp_config_generated.h"
    nros_grep_q -E '(^|[^_])nros_config_generated\.h' <<<"$code" \
        || echo "does not pair nros_config_generated.h"

    # THE INVARIANT #1050 IS ABOUT: the archive that is PAIRED must be the
    # archive that is LINKED.
    #
    # A first draft of this rule banned `IS_DIRECTORY` near a generated path
    # instead, and the self-test below refused it twice — once because the two
    # tokens sit on different LINES in the shape it was meant to catch, and once
    # because the rule was simply wrong: that loop is not the defect. Guarding
    # the generated dirs as directories is harmless and INSUFFICIENT; what makes
    # them safe is the pairing assertion existing beside it. Banning the loop
    # would have been a rule about the symptom, and it could not fire.
    #
    # Pairing against some OTHER archive is the failure that would restore #1050
    # in full: the check would pass while the link took a different file. So the
    # rule is that every ARCHIVE argument names the same variable the link uses.
    local archives n_arch
    archives="$(grep -E '^[[:space:]]*ARCHIVE[[:space:]]+' <<<"$code")"
    n_arch="$(grep -c . <<<"${archives:-}")"
    [ -z "$archives" ] && n_arch=0

    if [ "$n_arch" -lt 2 ]; then
        echo "has $n_arch pairing ARCHIVE argument(s); both generated headers must be paired"
    fi
    # `grep -qv` carries the same conflation. Ask the positive question instead:
    # "is there a line that does NOT mention it" becomes "count lines, count
    # matches", which has no error/non-match ambiguity at all.
    _n_lines="$(printf '%s\n' "$archives" | grep -c . || true)"
    _n_ours="$(printf '%s\n' "$archives" | grep -c '_NROS_PX4_CPP_A' || true)"
    if [ -n "$archives" ] && [ "$_n_lines" -ne "$_n_ours" ]; then
        echo "pairs against an archive other than \${_NROS_PX4_CPP_A} — the check would then pass while the link used a different file, which is issue 1050 restored"
    fi
    # ...and that variable must really be the one on the link line.
    nros_grep_q '_nros_px4_link_archives.*_NROS_PX4_CPP_A' <<<"$code" \
        || echo "\${_NROS_PX4_CPP_A} is no longer the archive on the link line; the pairing now describes a file nothing links"
}

check_wiring() {
    [ -f "$MODULE" ] || { bad "missing $MODULE"; return; }
    [ -f "$CONSUMER" ] || { bad "missing $CONSUMER"; return; }

    local findings
    findings="$(scan_consumer "$CONSUMER")"
    if [ -n "$findings" ]; then
        while IFS= read -r line; do
            [ -n "$line" ] && bad "NanoRosPx4Module.cmake $line"
        done <<<"$findings"
    fi

    # The tool choice is load-bearing and invisible at the call site (issue 1046:
    # system nm reports 0 for a symbol a byte scan finds 3 times, and does not
    # fail while doing it). A future edit "modernising" this to nm would be a new
    # instance of the very issue.
    if nros_grep_q -E '(execute_process|COMMAND)[^)]*\bnm\b' <(strip_cmake_comments "$MODULE"); then
        bad "NanoRosArchivePairing.cmake shells out to nm — system nm cannot read rust LLVM objects and reports absence it cannot observe (issue 1046). Keep the byte scan."
    fi
    nros_grep_q 'file(STRINGS' <(strip_cmake_comments "$MODULE") \
        || bad "NanoRosArchivePairing.cmake no longer byte-scans; the pairing cannot be observed"

    [ $fail -eq 0 ] && note "[ok] wiring: both generated headers are paired, by byte scan"
}

# The wiring rule's own negative control. Same obligation as the predicate's:
# a static rule nobody has seen fail is a comment.
self_test_wiring() {
    local d rc=0 found
    d="$(mktemp -d)"

    # A compliant consumer, reduced to the parts the rule reads. Case (h) needs
    # a baseline that is genuinely silent, or "no findings" proves nothing.
    local good='include(NanoRosArchivePairing.cmake)
nros_assert_archive_pairs_with_header(
    ARCHIVE "${_NROS_PX4_CPP_A}"
    HEADER  "${NANO_ROS_ROOT}/target/nros-cpp-generated/nros/nros_cpp_config_generated.h")
nros_assert_archive_pairs_with_header(
    ARCHIVE "${_NROS_PX4_CPP_A}"
    HEADER  "${NANO_ROS_ROOT}/target/nros-c-generated/nros/nros_config_generated.h")
set(_nros_px4_link_archives "${_NROS_PX4_CPP_A}" "${_NROS_PX4_PLATFORM_A}")'

    # (g) THE 1050 RESTORATION: pairs a real header against an archive that is
    #     not the one linked. Every filename check still passes, so only this
    #     rule stands between the fix and a check that describes a different file
    #     than the link uses.
    cat >"$d/wrong-archive.cmake" <<'EOF'
include(NanoRosArchivePairing.cmake)
nros_assert_archive_pairs_with_header(
    ARCHIVE "${NANO_ROS_ROOT}/target/debug/libnros_cpp.a"
    HEADER  "${NANO_ROS_ROOT}/target/nros-cpp-generated/nros/nros_cpp_config_generated.h")
nros_assert_archive_pairs_with_header(
    ARCHIVE "${NANO_ROS_ROOT}/target/debug/libnros_cpp.a"
    HEADER  "${NANO_ROS_ROOT}/target/nros-c-generated/nros/nros_config_generated.h")
set(_nros_px4_link_archives "${_NROS_PX4_CPP_A}" "${_NROS_PX4_PLATFORM_A}")
EOF
    found="$(scan_consumer "$d/wrong-archive.cmake")"
    if nros_grep_q 'other than' <<<"$found"; then
        note "[ok] pairing against an archive other than the linked one is caught"
    else
        bad "a consumer pairing a DIFFERENT archive than it links read as compliant — issue 1050 would be back"; rc=1
    fi

    # (h) The REAL comment from the fix. Prose describing the rule must not trip
    #     it. Not hypothetical tidiness — it is the false positive this script
    #     actually produced, on its own fix, before comments were stripped.
    { printf '%s\n' \
        '# for the two generated dirs. Issue 1046: `IS_DIRECTORY` cannot tell *present*' \
        '# from *current*, and a generated dir outlives every build that wrote into it.' \
        '# Do not pair against target/debug/libnros_cpp.a; use the linked archive.'
      printf '%s\n' "$good"
    } >"$d/prose.cmake"
    found="$(scan_consumer "$d/prose.cmake")"
    if [ -n "$found" ]; then
        bad "the wiring rule fires on a COMMENT — it reads prose, not code"
        sed 's/^/      /' <<<"$found" >&2
        rc=1
    else
        note "[ok] prose describing the rule does not trip it"
    fi

    # (i) A consumer that pairs only the C++ header. Narrower than the rule is
    #     issue 0196; it must not read as compliant.
    cat >"$d/half.cmake" <<'EOF'
nros_assert_archive_pairs_with_header(
    ARCHIVE "${_NROS_PX4_CPP_A}"
    HEADER  "${NANO_ROS_ROOT}/target/nros-cpp-generated/nros/nros_cpp_config_generated.h")
set(_nros_px4_link_archives "${_NROS_PX4_CPP_A}" "${_NROS_PX4_PLATFORM_A}")
EOF
    found="$(scan_consumer "$d/half.cmake")"
    if nros_grep_q 'does not pair nros_config_generated' <<<"$found"; then
        note "[ok] pairing only the C++ header is not accepted"
    else
        bad "a consumer pairing only nros_cpp_config_generated.h read as compliant"; rc=1
    fi

    # (j) The pairing calls deleted outright — the plainest regression.
    cat >"$d/none.cmake" <<'EOF'
set(_nros_px4_link_archives "${_NROS_PX4_CPP_A}" "${_NROS_PX4_PLATFORM_A}")
target_link_libraries(${NPX_MODULE} PUBLIC ${_nros_px4_link_archives})
EOF
    found="$(scan_consumer "$d/none.cmake")"
    if nros_grep_q 'does not call' <<<"$found"; then
        note "[ok] removing the pairing calls is caught"
    else
        bad "a consumer with NO pairing call read as compliant"; rc=1
    fi

    rm -rf "$d"
    return $rc
}

command -v cmake >/dev/null 2>&1 || { echo "SKIP: cmake not on PATH" >&2; exit 0; }

echo "check-px4-archive-header-pairing: the predicate must fire (issues 1046/1050)"
self_test || fail=1
self_test_wiring || fail=1
check_wiring

if [ $fail -ne 0 ]; then
    echo "check-px4-archive-header-pairing: FAILED" >&2
    exit 1
fi
echo "check-px4-archive-header-pairing: OK"
