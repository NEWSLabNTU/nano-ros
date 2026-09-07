#!/usr/bin/env bash
#
# Two images differing ONLY in a derived knob must not share a cargo directory —
# RFC-0094 D4 / phase-439 W1, acceptance A4.
#
# # The failure this prevents
#
# `nros_shared_cargo_dir()` hashes the fields its caller hands it, and cargo
# uplifts the final archive to an UNHASHED name (`libnros_c.a`). So two
# configurations that hash the same overwrite each other's artifact and one
# silently links the other's numbers — issue 0616's shape, diagnosed a long way
# from its cause. Until phase-439 the `nros-c` and NuttX keys carried
# `features, rmw, board, caps, profile, target` and **no knob values**, while a
# pool size is exactly what changes the compiled archive: issue 0528 measured
# the same omission on the Zephyr lane, where two leaves disagreeing on
# `CONFIG_NROS_EXECUTOR_MAX_CBS` shared a probe dir and one compiled against a
# constant sized for the other.
#
# RFC-0094's resolve phase makes per-image knob divergence the NORMAL case, so
# this is a precondition for it rather than a follow-up.
#
# # Why it drives cmake instead of reading it
#
# The probe (`scripts/lib/shared-cargo-key-probe.cmake`) calls the PRODUCTION
# `nros_knob_key_fields()` and the PRODUCTION normalise-and-hash. A gate that
# re-derived the key in bash would pass against its own copy of the rule, which
# is the defect and not the test.
#
# # What it asserts
#
#   1. no knob stated       -> the key text is byte-identical to the pre-439
#                              base key, so no warm cargo directory in the tree
#                              is invalidated for nothing
#   2. a knob stated        -> a DIFFERENT directory from (1)
#   3. the same knob twice  -> the SAME directory (a key is a function of the
#                              configuration, not of when it was computed)
#   4. a different VALUE    -> a different directory (presence is not enough)
#   5. a knob from another
#      family (executor)    -> also separates
#   6. NEGATIVE CONTROL     -> with the knob fields removed from the key, (2)
#                              COLLIDES with (1). This is the mutation the
#                              acceptance asks for, run on the normal path: if
#                              it ever stops colliding, this gate is measuring
#                              something other than the knob half and its green
#                              means nothing.
#   7. the inventory is the AUTHORED one -- names harvested from the resolver's
#      own `_nros_resolve_knob()` call sites, not a second list in cmake.

set -euo pipefail
cd "$(dirname "$0")/.."

# issue 0726 — a `grep -q` conditional cannot tell a tool ERROR from a NON-MATCH,
# and the two branch the same way while meaning opposite things. `nros_grep_q`
# exits 2 on a failure to run rather than reporting a finding.
# shellcheck source=scripts/lib/grep-q.sh
source scripts/lib/grep-q.sh

PROBE="scripts/lib/shared-cargo-key-probe.cmake"
[ -f "$PROBE" ] || { echo "[FAIL] missing $PROBE" >&2; exit 1; }

command -v cmake >/dev/null 2>&1 || {
    echo "[FAIL] cmake not on PATH — this gate computes the key with the" >&2
    echo "       production cmake code and cannot answer without it." >&2
    exit 1
}

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

fail=0

# probe <mode> <field> [ENV=VAL ...]
# Echoes the DIR= or KEY= line's value for one probe run.
probe() {
    local mode="$1" field="$2"
    shift 2
    local out
    if ! out="$(env "$@" cmake -DNROS_PROBE_ROOT="$TMP/store" \
        -DNROS_PROBE_MODE="$mode" -P "$PROBE" 2>&1)"; then
        echo "[FAIL] probe ($mode, $*) did not run:" >&2
        printf '%s\n' "$out" >&2
        exit 1
    fi
    local line
    line="$(printf '%s\n' "$out" | sed -n "s/^${field}=//p")"
    if [ -z "$line" ]; then
        echo "[FAIL] probe ($mode, $*) printed no ${field}= line:" >&2
        printf '%s\n' "$out" >&2
        exit 1
    fi
    printf '%s' "$line"
}

expect_ne() {
    local what="$1" a="$2" b="$3"
    if [ "$a" = "$b" ]; then
        echo "[FAIL] $what — both resolved to $a" >&2
        fail=1
    fi
}

expect_eq() {
    local what="$1" a="$2" b="$3"
    if [ "$a" != "$b" ]; then
        echo "[FAIL] $what" >&2
        echo "         first:  $a" >&2
        echo "         second: $b" >&2
        fail=1
    fi
}

# The negative control, run unconditionally on the normal path: with the knob
# fields taken back out of the key — the pre-phase-439 shape, which is what
# reverting the change produces — the two images of assertion 2 must COLLIDE.
#
# A gate whose discriminator is broken (a probe that never ran, an env that
# never reached cmake, a mode flag that stopped being read) would report every
# comparison as "different" and look green. This is the control that refuses
# that: it demands the probe DEMONSTRATE the collision it exists to prevent.
self_test() {
    local base collide
    base="$(probe base-only DIR)"
    collide="$(probe base-only DIR ZPICO_MAX_QUERYABLES=2)"
    if [ "$base" != "$collide" ]; then
        echo "[FAIL] negative control: with the knob fields removed from the" >&2
        echo "       key, two images differing in ZPICO_MAX_QUERYABLES were" >&2
        echo "       expected to COLLIDE and did not. Something other than the" >&2
        echo "       knob half is moving this hash, so a green verdict from the" >&2
        echo "       assertions below would not be about phase-439 W1." >&2
        echo "         no knob: $base" >&2
        echo "         knob=2:  $collide" >&2
        return 1
    fi
    return 0
}

self_test || fail=1

base_key="$(probe base-only KEY)"
plain_key="$(probe with-knobs KEY)"
expect_eq "an image stating no knob must keep the key it already had (a warm cargo directory is not invalidated for nothing)" \
    "$base_key" "$plain_key"

plain="$(probe with-knobs DIR)"
knob2="$(probe with-knobs DIR ZPICO_MAX_QUERYABLES=2)"
expect_ne "two images differing ONLY in ZPICO_MAX_QUERYABLES share a cargo directory (RFC-0094 D4)" \
    "$plain" "$knob2"

knob2_again="$(probe with-knobs DIR ZPICO_MAX_QUERYABLES=2)"
expect_eq "the same configuration computed twice must give the same directory" \
    "$knob2" "$knob2_again"

knob4="$(probe with-knobs DIR ZPICO_MAX_QUERYABLES=4)"
expect_ne "two images differing only in the VALUE of ZPICO_MAX_QUERYABLES share a cargo directory" \
    "$knob2" "$knob4"

cbs="$(probe with-knobs DIR NROS_EXECUTOR_MAX_CBS=8)"
expect_ne "two images differing only in NROS_EXECUTOR_MAX_CBS share a cargo directory" \
    "$plain" "$cbs"

# The inventory is READ from the resolver's own call sites, so a knob that gains
# a `_nros_resolve_knob()` line joins the key with no second edit. Spot-check
# both families rather than the whole list: the point is that the harvest works,
# not to restate 43 names here.
inventory="$(env cmake -DNROS_PROBE_MODE=inventory -P "$PROBE" 2>&1 | sed -n 's/^KNOB=//p')"
for want in ZPICO_MAX_QUERYABLES NROS_EXECUTOR_MAX_CBS NROS_XRCE_MAX_SUBSCRIBERS; do
    nros_grep_q -x "$want" <<<"$inventory" && continue
    echo "[FAIL] $want is not in the harvested knob inventory — the key" >&2
    echo "       cannot separate two images that differ only in it." >&2
    fail=1
done

if [ "$fail" != 0 ]; then
    echo "" >&2
    echo "  Every cargo-directory key carries the resolve digest (RFC-0094 D4)." >&2
    echo "  The knob half has ONE spelling — nros_knob_key_fields() in" >&2
    echo "  cmake/NanoRosSharedCargoDir.cmake — and every caller of" >&2
    echo "  nros_shared_cargo_dir() / nros_share_corrosion_cargo_dir() appends" >&2
    echo "  it. Do not write a per-caller knob list (issue 1025)." >&2
    exit 1
fi

# Counted in the shell rather than with `grep -c`: same 0/1/>=2 conflation, and
# a tool failure there would print a plausible number rather than a missing one.
read -r -a _knob_names <<<"$(printf '%s' "$inventory" | tr '\n' ' ')"
echo "cargo-dir-knob-key OK — ${#_knob_names[@]} knob(s) in the key, negative control collided as required."
