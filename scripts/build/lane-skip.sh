# Lane skip protocol — issue 0599.
#
# A platform fixture lane that cannot run because a PRECONDITION is missing (no
# Zephyr workspace, no `arm-none-eabi-gcc`, no PX4 tree) is not a failure. It was
# also not a success, and every such site used to `exit 0`, so the driver
# recorded `== zephyr == OK` for a lane that built nothing.
#
# What that cost, concretely: the four west-owned compile-check fixtures never
# got built, and the operator learned it twenty minutes later from `_lane-gate`,
# as four missing `.inputsig` files, with a remedy (`just build-test-fixtures`)
# naming the command that had just "succeeded". A skip invisible at the point of
# decision surfaces as an artifact error at a distance from its cause.
#
# So: a third verdict. `nros_lane_skip "<reason>"` prints a machine-readable
# marker and exits 78 (sysexits' EX_CONFIG — "configuration error", which is
# exactly what a missing SDK is). The driver in `justfile`'s
# `build-test-fixtures-leaves` treats 78 as SKIPPED, prints the reason, and does
# NOT fail the build.
#
# ONE spelling, because six sites across three lanes had the same `exit 0` and
# fixing one would have left five. Add new skip sites through this function.

NROS_LANE_SKIP_RC=78

# One source line at a call site, not two: the partial-skip helpers below need
# `nros_build_dir`, and every caller forgetting the second source would be a
# silent unbound-command in a `just` recipe.
if ! command -v nros_build_dir >/dev/null 2>&1; then
    # shellcheck source=scripts/build/build-root.sh
    . "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/build-root.sh"
fi

# nros_lane_skip <reason…>
#
# Exits the calling lane with the SKIPPED verdict. The reason is printed twice
# on purpose: once as prose for whoever reads the lane log directly, once as the
# `NROS_LANE_SKIP:` marker the driver greps out of that log to put in the
# summary. Keep the reason short and name the remedy — it is what the operator
# sees instead of "OK".
nros_lane_skip() {
    local reason="$*"
    echo "NROS_LANE_SKIP: ${reason}"
    echo "lane skipped: ${reason}"
    exit "${NROS_LANE_SKIP_RC}"
}

# ---------------------------------------------------------------------------
# Partial skips — issue 0650
#
# `nros_lane_skip` above answers "this lane cannot run at all". It does not fit
# a lane whose STEPS have separate preconditions: nuttx builds arm and riscv,
# and a host with one toolchain and not the other should still get the half it
# can build. Those sites therefore wrote `echo "… skip: …"; exit 0` — 21 of
# them across five lanes — and the lane's terminal recipe then printed
# "<platform> test fixtures built.", exit 0, having built nothing.
#
# That is the same defect 0599 named, one level down, and it is worse here
# because the lane REPORTS SUCCESS in its own words. It is how a platform's
# entire fixture set silently went unbuilt on this host, and how a source
# divergence in six riscv64 examples reached main through a lane that "passed".
#
# So a step NOTES its skip and carries on; the lane FLUSHES at the end. If any
# step skipped, the lane exits 78 (SKIPPED, with every reason) instead of
# claiming it built fixtures. A file is the channel because each step runs as
# its own `just` invocation — no shell state survives between them.
#
# Usage, per lane:
#   nros_lane_skip_reset  <lane>              # at the start of `build-fixtures`
#   nros_lane_skip_note   <lane> "<reason>"   # at a step's precondition, then exit 0
#   nros_lane_skip_flush  <lane> "<success line>"   # instead of the success echo

_nros_lane_skip_file() {
    local lane="${1:?_nros_lane_skip_file: lane}"
    printf '%s/%s.skips' "$(nros_build_dir "$NROS_KIND_LANE_SKIPS")" "$lane"
}

nros_lane_skip_reset() {
    local f
    f="$(_nros_lane_skip_file "${1:?nros_lane_skip_reset: lane}")"
    mkdir -p "$(dirname "$f")"
    : > "$f"
}

nros_lane_skip_note() {
    local lane="${1:?nros_lane_skip_note: lane}"
    shift
    local reason="$*"
    local f
    f="$(_nros_lane_skip_file "$lane")"
    mkdir -p "$(dirname "$f")"
    printf '%s\n' "$reason" >> "$f"
    echo "${lane} skip: ${reason}"
}

# nros_lane_skip_flush <lane> <success-line>
#
# The ONLY place a lane says it built its fixtures. Prints the success line when
# nothing was skipped; otherwise reports every skipped step and exits 78, so the
# driver records SKIPPED and the operator learns it here rather than twenty
# minutes later from a missing artifact (0599's lesson, which is why the reasons
# are repeated in full).
nros_lane_skip_flush() {
    local lane="${1:?nros_lane_skip_flush: lane}"
    shift
    local success="$*"
    local f
    f="$(_nros_lane_skip_file "$lane")"
    if [ ! -s "${f}" ]; then
        [ -n "$success" ] && echo "$success"
        return 0
    fi
    local n
    n="$(grep -c . "$f")"
    echo "NROS_LANE_SKIP: ${lane}: ${n} step(s) skipped — $(paste -sd '; ' "$f")"
    echo "lane ${lane} INCOMPLETE — ${n} step(s) skipped, so its fixtures are NOT built:"
    sed 's/^/  - /' "$f"
    exit "${NROS_LANE_SKIP_RC}"
}
