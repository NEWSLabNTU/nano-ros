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

# ---------------------------------------------------------------------------
# Named vs included — phase-407 W2
#
# Everything above is intact and stays: SKIPPED is still a third verdict, and
# `nros_lane_skip` still exits 78 for a lane whose prerequisite is absent. What
# 0599/0650 could not express is WHO ASKED.
#
#   just build-test-fixtures lane=all      zephyr was INCLUDED by a broad lane.
#                                          The operator did not claim to have
#                                          provisioned it. A skip is correct and
#                                          must be reported.
#
#   just zephyr build-fixtures             zephyr was NAMED. There is no lane to
#                                          disambiguate and no second reading of
#                                          the command line. "ZEPHYR_WORKSPACE
#                                          not set up" is then not a skip, it is
#                                          the answer to what was asked, and
#                                          exiting 78 makes the run green.
#
# The rule (phase-407): **named must work; unnamed may skip, and is reported.**
# Naming IS the specification, so no second declaration is kept in step with it.
#
# THE SIGNAL, AND WHY ITS DEFAULT POINTS THIS WAY
#
# `NROS_LANE_INCLUDED` is set by the fixture fan-out in `justfile`'s
# `build-test-fixtures-leaves` — the ONE place that reaches a platform lane
# without the operator having typed its name. Unset means NAMED.
#
# That polarity is deliberate and is the whole safety argument. A site or a
# driver this change fails to reach keeps `NROS_LANE_INCLUDED` unset, so it
# reads as NAMED and FAILS LOUDLY. The opposite default ("included unless
# proven named") would leave every missed site quietly behaving as it does
# today — indistinguishable from a correct one, which is the exact failure
# shape this work exists to remove.
#
# The value is the platform token, for the log; only EMPTINESS is tested.
# Matching the value against the lane name would be a second SSoT and would
# break immediately: the fan-out's tokens are `threadx_linux`/`threadx_riscv64`
# while the lanes' own skip ledgers are named `threadx-linux`/`threadx-riscv64`.
#
# WHAT IS AND IS NOT SUBJECT TO THE RULE
#
# Two things use `nros_lane_skip` and only one of them is a platform lane:
#
#  * A PLATFORM LANE declares itself with `nros_lane_platform <lane>`. It is the
#    thing a user names, so the rule applies. (`nros_lane_skip_note` needs no
#    declaration — it already takes the lane as its first argument, which is the
#    same fact; that is why all ~14 step sites are covered without touching one
#    of them.)
#  * A CHECK GATE or a license-gated optional recipe (`check-submodule-pins`
#    with no network, the ARM FVP recipes, the root-only package probe in
#    `just/ci.just`) declares no platform and keeps 0599's behaviour exactly.
#    Those skips are not about a platform anyone named.
#
# AND A THIRD KIND OF SKIP, WHICH IS NOT A PREREQUISITE AT ALL
#
# `nros_lane_out_of_scope_note` exists because two nuttx sites were skipping for a
# reason that has nothing to do with provisioning: `nros_lane_wants_platform`
# said this run's LANE selected no such coordinate. Naming the platform cannot
# make that step runnable and must not turn it into a failure, so it is a
# separate spelling rather than an exemption string. Both spellings still land
# in the same ledger and are still reported by the flush.

NROS_LANE_NAMED_RC=1

# nros_lane_platform <lane>
#
# "This recipe IS the <lane> platform lane." One line, at the top of a lane that
# uses the whole-recipe `nros_lane_skip`. Gated by `check-named-lane-fails`, so
# a new platform-lane skip site cannot silently forget it.
nros_lane_platform() {
    _NROS_LANE_PLATFORM="${1:?nros_lane_platform: lane}"
}

# nros_lane_named
#
# True when the caller's platform was NAMED rather than included by a fan-out.
nros_lane_named() {
    [ -z "${NROS_LANE_INCLUDED:-}" ]
}

# _nros_lane_named_fail <lane> <reason…>
#
# The verdict a NAMED platform gets instead of SKIPPED. The reason is the site's
# OWN text, unchanged — those messages already name the remedy, and rewording
# them here would put the remedy in two places. What is added is why this is a
# failure, because "run `just zephyr setup`" printed under a red exit is a
# different instruction from the same words printed under a green one.
_nros_lane_named_fail() {
    local lane="${1:?_nros_lane_named_fail: lane}"
    shift
    local reason="$*"
    echo "NROS_LANE_NAMED_FAIL: ${lane}: ${reason}" >&2
    echo "" >&2
    echo "error: you named \`${lane}\`, so this is a FAILURE, not a skip." >&2
    echo "  ${reason}" >&2
    echo "" >&2
    echo "  Naming a platform IS the specification (phase-407). The same platform" >&2
    echo "  reached by a preset or the broad default — \`just build-test-fixtures" >&2
    echo "  lane=<lane>\` — would SKIP here and say so in the summary." >&2
    exit "${NROS_LANE_NAMED_RC}"
}

# nros_lane_skip <reason…>
#
# Exits the calling lane with the SKIPPED verdict. The reason is printed twice
# on purpose: once as prose for whoever reads the lane log directly, once as the
# `NROS_LANE_SKIP:` marker the driver greps out of that log to put in the
# summary. Keep the reason short and name the remedy — it is what the operator
# sees instead of "OK".
#
# phase-407 W2: when the recipe declared `nros_lane_platform` and its platform was
# NAMED, this is a failure instead.
nros_lane_skip() {
    local reason="$*"
    if [ -n "${_NROS_LANE_PLATFORM:-}" ] && nros_lane_named; then
        _nros_lane_named_fail "${_NROS_LANE_PLATFORM}" "${reason}"
    fi
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

# nros_lane_skip_note <lane> <reason…>
#
# A STEP of <lane> cannot run because a PREREQUISITE is missing. Its lane
# argument is already the named/included question's subject, so phase-407 W2
# needed no edit at any of its call sites: when <lane> was NAMED, a missing
# prerequisite is a failure and this does not return.
nros_lane_skip_note() {
    local lane="${1:?nros_lane_skip_note: lane}"
    shift
    local reason="$*"
    if nros_lane_named; then
        _nros_lane_named_fail "$lane" "$reason"
    fi
    local f
    f="$(_nros_lane_skip_file "$lane")"
    mkdir -p "$(dirname "$f")"
    printf '%s\n' "$reason" >> "$f"
    echo "${lane} skip: ${reason}"
}

# nros_lane_out_of_scope_note <lane> <reason…>
#
# A STEP of <lane> is OUT OF THIS RUN'S COORDINATES — `nros_lane_wants_platform`
# said the lane selected no such row (phase-340). Nothing is missing and nothing
# is broken, so naming the platform must NOT turn this into a failure: the user
# named `nuttx`, the lane narrowed away `nuttx-riscv`, and both are true at once.
#
# Recorded and printed exactly like a prerequisite skip — the flush still refuses
# to claim the lane built its fixtures — but never promoted to a failure. It is a
# distinct spelling rather than a reason-string exemption because the difference
# is in the KIND of skip, and a substring match on prose is how the wrong half of
# a class gets fixed.
nros_lane_out_of_scope_note() {
    local lane="${1:?nros_lane_out_of_scope_note: lane}"
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
#
# phase-407 W2 leaves this unchanged, and that is the correct outcome rather than
# an omission: under a NAMED platform a PREREQUISITE note never reaches the
# ledger (it failed at the step), so anything the flush finds here got in through
# `nros_lane_out_of_scope_note` — a lane narrowing, which is reportable and not a
# failure. SKIPPED remains the right verdict for it.
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
