#!/usr/bin/env bash
#
# Issue 1353, step 3 — reclaim the disk a `ci-base` lane has already spent on
# things nothing downstream reads. Phase-466 W4.
#
# WHY THIS EXISTS AT ALL, since `rm -rf` is an antipattern here. CLAUDE.md's
# rule is about build OUTPUT: wiping a build dir to make an incremental build
# behave destroys the reproduction of a missing dependency edge, and the edge is
# the bug. Nothing below is build output of anything this job will read again:
# one is a bind mount the runner brings and this container never opens, the
# other is a cargo scratch dir whose PRODUCT is a JSON sidecar written
# elsewhere. Removing them cannot change an artifact, only how much room the
# next step has.
#
# WHAT THE NUMBERS SAY (issue 1353, measured on host-tests run 35813854577):
# the job reaches `just ci tier1` with 22 G free and the tier wants more than
# that — `packages/cli/target` 404 M -> 13 G, a fresh `target/` at 4.1 G,
# `build/` +3 G — so it dies on a write. Raising the runner is not available
# here (both lanes are `container:` jobs, and a container cannot delete the
# host's preinstalled tooling), and pruning what the tier BUILDS is a question
# about fixture coverage rather than about disk. This reclaims the two things
# that are neither.
#
# NOT A REMEDY FOR A JOB THAT GENUINELY NEEDS MORE THAN THE DISK HOLDS. It buys
# headroom and REPORTS what it bought, so the next failure is priced against a
# number rather than re-argued. It never fails a job: a reclaim that can break
# the lane it is helping is worse than no reclaim, which is the same rule
# `disk-report.sh` states for measurement.
#
# Opt out with NROS_CI_NO_RECLAIM=1 (then the report still prints the sizes, so
# a run with it set still says what was on the table).
#
# Usage:  scripts/ci/reclaim-disk.sh "<label>"
set -uo pipefail

label="${1:-reclaim}"
# `GITHUB_WORKSPACE` then `$PWD`, and deliberately NOT `NROS_REPO_DIR`: issue
# 1280 — that variable resolves ENV-FIRST and an agent worktree INHERITS it
# pointing at the outer checkout, so reading it here would aim an `rm -rf` at a
# tree this run is not building. The checkout a job is running in is the only
# tree whose scratch this script may touch, and `$PWD` is that tree.
ws="${GITHUB_WORKSPACE:-$PWD}"

echo "::group::disk reclaim — ${label}"

_free_kb() { df -Pk "$ws" 2>/dev/null | awk 'NR==2 {print $4}'; }

before_kb="$(_free_kb)"
[ -n "${before_kb:-}" ] || before_kb=0

# The candidates, each with the reason it is safe to remove HERE. A path is
# only ever a candidate when the reason holds for both lanes this runs on
# (`gate`'s `check` job and `host-tests`' integration job).
#
# 1. The hosted tool cache. GitHub mounts the runner's `/opt/hostedtoolcache`
#    into a `container:` job as `$RUNNER_TOOL_CACHE` (`/__t`), on the same
#    filesystem as the workspace — so its ~9 G counts against us, and deleting
#    it frees real space rather than writing overlay whiteouts the way deleting
#    anything from the image itself would. These lanes never read it: their
#    Python, Node and toolchains come from the `nano-ros-ci` image, and the
#    JavaScript actions they do use (`checkout`, `cache`, `upload-artifact`)
#    run on the runner's own bundled node under `/__e`, not on a tool-cache
#    version. Nothing in either workflow is a `setup-*` action.
#
# 2. `<repo>/build/metadata-probe` — the SHARED cargo target dir the sizing
#    metadata probes compile into (issue 0522, which created it precisely
#    because the per-component version measured 108 dirs / 82.4 GiB). It is
#    scratch: a probe's product is `<component>/metadata/<name>.json` beside
#    the component, written by `nros sync` and already on disk by the time any
#    lane reaches the expensive step. Deleting it costs a re-probe if something
#    later syncs again; it cannot make a wrong artifact, because there is no
#    artifact here to get wrong.
_candidates=()
if [ -n "${RUNNER_TOOL_CACHE:-}" ] && [ -d "${RUNNER_TOOL_CACHE}" ]; then
    case "${RUNNER_TOOL_CACHE}" in
        # Never touch something inside the checkout, whatever the runner says
        # it is — that WOULD be build output.
        "$ws"|"$ws"/*) ;;
        *) _candidates+=("${RUNNER_TOOL_CACHE}") ;;
    esac
fi
[ -d "$ws/build/metadata-probe" ] && _candidates+=("$ws/build/metadata-probe")

if [ "${#_candidates[@]}" -eq 0 ]; then
    echo "nothing to reclaim here (no tool cache mount, no probe scratch)"
else
    for p in "${_candidates[@]}"; do
        rc=0
        sz="$(du -sh "$p" 2>/dev/null | awk '{print $1}')" || rc=$?
        [ "$rc" -eq 0 ] && [ -n "${sz:-}" ] || sz="?"
        if [ -n "${NROS_CI_NO_RECLAIM:-}" ]; then
            printf 'would reclaim %s\t%s (NROS_CI_NO_RECLAIM is set)\n' "$sz" "$p"
            continue
        fi
        printf 'reclaiming    %s\t%s\n' "$sz" "$p"
        # Its contents, not the directory: `$RUNNER_TOOL_CACHE` is a mount
        # point, and removing a mount point is a different and failing
        # operation from emptying one.
        rm -rf -- "${p:?}"/* "${p:?}"/.[!.]* 2>/dev/null || true
    done
fi

# 3. Package-manager and pip caches. These are the removals `live-peer.yml`
#    carried INLINE before this script existed, kept here so the repo has ONE
#    reclaim rather than a second spelling of it (CLAUDE.md: add a shared
#    helper, never a second idiom).
#
#    Be honest about what they are worth, because the two halves differ. A file
#    the JOB downloaded — an `apt-get update` list, a pip wheel cache — lives in
#    the container's upper layer and deleting it frees real space. A file BAKED
#    INTO THE IMAGE (`/usr/share/doc`, `/usr/share/man`, `/usr/share/locale`)
#    lives in a lower layer, so deleting it writes a whiteout and frees nothing;
#    it is kept only because dropping it would be an unmeasured behaviour change
#    to a lane this phase does not own, and it costs a second. If someone
#    measures the `df` delta with and without them, the ones that buy nothing
#    should go.
#
#    Guarded on `$GITHUB_ACTIONS` rather than `$CI`: these paths are OUTSIDE
#    the checkout, so on a developer machine they are the developer's system.
#    `CI` is a variable anyone can have set; `GITHUB_ACTIONS` is set by the
#    runner and by nothing else.
if [ -z "${NROS_CI_NO_RECLAIM:-}" ] && [ -n "${GITHUB_ACTIONS:-}" ]; then
    apt-get clean 2>/dev/null || true
    rm -rf /var/lib/apt/lists/* /usr/share/doc /usr/share/man /usr/share/locale 2>/dev/null || true
    rm -rf "${HOME:-/root}/.cache/pip" 2>/dev/null || true
fi

after_kb="$(_free_kb)"
[ -n "${after_kb:-}" ] || after_kb="$before_kb"
freed_mb=$(( (after_kb - before_kb) / 1024 ))
summary="$(printf 'freed %s MB; %s KB free' "$freed_mb" "$after_kb")"
echo "$summary"
echo "::endgroup::"

# Same two channels as `disk-report.sh`, and for the same reason: on the
# scheduled `gate` lane the job log is what the failure destroys, so a number
# that only reaches the log is a number nobody reads.
printf '::notice title=disk reclaim %s::%s\n' "$label" "$summary"
if [ -n "${GITHUB_STEP_SUMMARY:-}" ]; then
    printf -- '- **disk reclaim %s** — %s\n' "$label" "$summary" \
        >>"$GITHUB_STEP_SUMMARY" 2>/dev/null || true
fi

exit 0
