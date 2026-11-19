#!/usr/bin/env bash
# Provision a gate lane's claims: ONE implementation per claim (phase-565 W2,
# RFC-0131 D3), run by `just setup-worktree` for the `dev` lane and by CI's
# scripts/ci/provision-gate-lane.sh for a CI lane.
#
# What a lane claims, and how each claim is provided, is `.config/gate-lanes.toml`
# (`[lane.<name>] claims`, `[provision.claim]`, `[provision] order`), read through
# scripts/lib/lane_provision.py. A claim with a `provisioner` runs the
# `claim_<provisioner>` function below; a HOST claim (cmake, cc, ros2, colcon, ...)
# is named with its remedy and never installed (no sudo). The lane preflight
# decides afterwards whether everything is there.
#
# Usage: scripts/lib/claim-provision.sh <lane> [--ci]     (from the checkout root)
#   --ci   a CI job: submodules at depth 1, and a `ci = false` provisioner is
#          left to the job's own steps.
#
# Gate: `lane-claims-provisioned` (every capability has a provisioner here or a
# host remedy; every provisioner named in the table is a function here).
set -euo pipefail
cd "$(dirname "$0")/../.."

# --- submodules ---------------------------------------------------------------
# The `[[provision.submodule]]` rows whose tiers meet the lane's (issue 1513's
# one list). A STORE-FIRST source goes into the store once a FRESH CLI can do it
# (phase-484 W3c, RFC-0103 D5); otherwise the submodule is checked out. Only an
# UNINITIALISED submodule is touched: one checked out off its pin is reported,
# never updated (AGENTS.md: the update discards work in it), and an interrupted
# checkout is reported with its remedy (issue 2306).
claim_submodules() {
    local rows p row state broken=0 drifted=0 _cli=""
    local -a paths=() want=() depth=()
    rows="$(python3 scripts/lib/lane_provision.py submodules --tiers "${LANE_TIERS[@]}")"
    [ -z "$rows" ] || mapfile -t paths <<<"$rows"
    [ "$CI_MODE" = 1 ] && depth=(--depth 1)
    # issue 1850 — strictly THIS checkout's CLI (`NROS_CLI=`): it provisions
    # this checkout's store rows. Empty on a fresh clone: "no CLI yet".
    # shellcheck source=scripts/build/cargo.sh
    source scripts/build/cargo.sh
    _cli="$(NROS_CLI= nros_cli_bin --checkout-only)" || _cli=""
    # shellcheck source=scripts/lib/submodule-provision.sh
    . scripts/lib/submodule-provision.sh
    for p in "${paths[@]}"; do
        row="$(python3 scripts/lib/source_locate.py --store-row-for "$p" 2>/dev/null || true)"
        # A STALE CLI is "no CLI yet" here, not a CLI to call: it refuses every
        # verb, and checking out `play_launch` (a CLI build input, issue 0561)
        # stales it by itself. `source-stamp` is `check cli-fresh`'s question.
        if [ -n "$row" ] && [ -n "$_cli" ] && "$_cli" source-stamp >/dev/null 2>&1; then
            echo "  [STORE]   $p -> nros setup --source $row"
            "$_cli" setup --source "$row"
            continue
        fi
        state="$(nros_submodule_provision_state "$p")"
        case "$state" in
            missing) want+=("$p") ;;
            deregistered)
                # Issue 2505 -- re-register only; the checkout is someone's
                # and stays as it is. Said loudly: the cause is another
                # session, which this worktree's own history cannot show.
                echo "  [RE-REGISTERED] $p was checked out here but no longer registered:"
                echo "            another linked worktree ran \`git submodule deinit\`, which removes the"
                echo "            registration every worktree shares. Remove a finished worktree with"
                echo "            \`just worktree-prune\` or \`git worktree remove --force\`, never deinit."
                git submodule init -- "$p" >/dev/null || broken=$((broken + 1))
                ;;
            incomplete:* | diverged)
                broken=$((broken + 1))
                echo "  [BROKEN]  $p is ${state%%:*} ($state): an interrupted or redirected checkout."
                echo "            NOT repaired - that would discard what it holds. When it holds nothing:"
                echo "                $(nros_submodule_state_remedy "$state" "$p")"
                ;;
            behind:*)
                # issue 2701: KEPT, and a failure. A rebase across a pin move
                # leaves the checkout behind; every gate then reads the old tree.
                drifted=$((drifted + 1))
                echo "  [BEHIND]  $p is checked out BEHIND its pin ($state)."
                echo "            NOT updated - that would discard work in it (AGENTS.md)."
                echo "            When you are sure it holds nothing you need:"
                echo "                git submodule update $p"
                ;;
            ahead:* | unknown-pin)
                # AHEAD is a vendored-fork fix mid-flight (push the fork, then
                # bump the pin), so it is reported and passes.
                echo "  [KEPT]    $p is checked out at a commit other than its pin ($state)."
                echo "            NOT updated - that would discard work in it (AGENTS.md)."
                echo "            When you are sure it holds nothing you need:"
                echo "                git submodule update $p"
                ;;
            not-submodule) echo "  [UNKNOWN] $p is not a submodule of this checkout." ;;
            *) echo "  [OK]      $p already checked out." ;;
        esac
    done
    if [ "${#want[@]}" -gt 0 ]; then
        echo "  checking out ${#want[@]} submodule(s)."
        git submodule update --init "${depth[@]}" "${want[@]}"
    fi
    if [ "$broken" -gt 0 ]; then
        echo "claim-provision: $broken submodule(s) are BROKEN (listed above); repair them, then re-run." >&2
        return 1
    fi
    if [ "$drifted" -gt 0 ]; then
        echo "claim-provision: $drifted submodule(s) are BEHIND their pin (listed above, issue 2701);" >&2
        echo "  update each with the command shown once it holds nothing you need, then re-run." >&2
        return 1
    fi
}

# --- git-history --------------------------------------------------------------
# For the gates that ask about the COMMIT GRAPH (`doc-commit-citations`,
# `roadmap-commit-refs`): on a shallow clone a dead hash and an unfetched one
# look the same. The BASE branch's commits, treeless (`--filter=tree:0`):
# measured 1.8 s and +13 MiB from a depth-1 clone, against 49 s and ~1 GiB for a
# full unshallow (issue 1476). The base branch only, never a bare `origin`: with
# checkout's `+refs/heads/*` refspec that pulled every branch and ran 85
# minutes, twice. A worktree of a full clone is not shallow: nothing to do.
claim_git_history() {
    local base="${GITHUB_BASE_REF:-main}"
    if [ "$(git rev-parse --is-shallow-repository)" = true ]; then
        # The depth-1 base fetch FIRST is load-bearing: without the base tip
        # present, git 2.34's treeless unshallow fetches trees one at a time
        # and stalls (measured: past a 240 s cap; with it, 5 s).
        git fetch --no-tags --quiet --depth=1 origin \
            "+refs/heads/$base:refs/remotes/origin/$base"
        # shellcheck disable=SC1091
        . scripts/lib/deadline.sh
        "${NROS_DEADLINE[@]}" 300 git fetch --no-tags --quiet --filter=tree:0 --unshallow \
            origin "+refs/heads/$base:refs/remotes/origin/$base"
    fi
    test "$(git rev-parse --is-shallow-repository)" = false
    echo "  [OK]      history: $(git rev-list --count HEAD) commits"
}

# --- zephyr-tree --------------------------------------------------------------
# Zephyr's C++ minimal-libc headers, a sparse clone at the pinned west revision
# (issue 2308 keeps it from reading as a west workspace).
claim_zephyr_tree() {
    local rev
    rev="$(awk '/- name: zephyr$/ { z = 1 } z && /revision:/ { print $2; exit }' west.yml)"
    [ -n "$rev" ] || { echo "claim-provision: no zephyr revision in west.yml" >&2; return 1; }
    if [ ! -d zephyr-workspace/zephyr/lib/cpp/minimal/include ]; then
        git clone --quiet --depth 1 --filter=blob:none --sparse --branch "$rev" \
            https://github.com/zephyrproject-rtos/zephyr.git zephyr-workspace/zephyr
        git -C zephyr-workspace/zephyr sparse-checkout set lib/cpp/minimal/include
    fi
    echo "  [OK]      zephyr-tree at $rev"
}

# --- cli ----------------------------------------------------------------------
# This checkout's CLI and the launch resolver. Last among what the others feed:
# `submodules` checks out `play_launch`, whose pin is a CLI build input (issue
# 0561), so it runs first ([provision] order). `setup-cli` asks the CLI's own
# freshness question (`source-stamp`), so a fresh CLI is not rebuilt.
claim_cli() {
    just setup-cli
    just setup-launch-resolve
}

# --- sources ------------------------------------------------------------------
# The pinned rosidl source clone (`python-entry-provenance`). Needs a CLI, so it
# runs after `cli` ([provision] order).
claim_sources() {
    # shellcheck disable=SC1091
    source ./activate.sh
    nros setup --source rosidl
}

# --- arm-gcc ------------------------------------------------------------------
# Issue 2413 — the FreeRTOS C++ copy-out units build with the PINNED cross
# compiler; the image's distro arm-none-eabi-gcc 10.3 is below issue 1936's C++
# floor of GCC 11, so without this they fail at configure.
claim_arm_gcc() {
    # shellcheck disable=SC1091
    source ./activate.sh
    nros setup --tool arm-none-eabi-gcc
}

# --- the lane -----------------------------------------------------------------
lane="${1:?usage: claim-provision.sh <lane> [--ci]}"
CI_MODE=0
[ "${2:-}" = "--ci" ] && CI_MODE=1
decl="$(python3 scripts/lib/lane_provision.py lane "$lane")"
read -r -a LANE_TIERS <<<"$(sed -n 's/^tiers: //p' <<<"$decl")"
ci_flag=()
[ "$CI_MODE" = 1 ] && ci_flag=(--ci)
steps="$(python3 scripts/lib/lane_provision.py plan "$lane" "${ci_flag[@]}")"
echo "claim-provision: lane \`$lane\` ($(sed -n 's/^claims: //p' <<<"$decl"))"
rc=0
while IFS=$'\t' read -r kind what remedy; do
    case "$kind" in
        run)
            echo "== $what"
            "claim_${what//-/_}" || rc=1
            ;;
        job) echo "== $what: provided by this job's own steps" ;;
        host) echo "== $what: a HOST tool, not installed here -> $remedy" ;;
    esac
done <<<"$steps"
exit "$rc"
