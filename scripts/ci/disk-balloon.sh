#!/usr/bin/env bash
#
# disk-balloon.sh — run a command so that running OUT of disk is a reported,
# deliberate failure instead of a dead runner with no logs.
#
#   scripts/ci/disk-balloon.sh run [--balloon-gb N] [--min-free-gb M] -- <cmd> [args…]
#
# merge-runtime run 43534848324: the zephyr tier-1 shard filled the hosted
# runner's disk, the runner's own diagnostic write failed with `No space left
# on device`, and the job ended with every log gone (`BlobNotFound`) and no
# artifact. Nothing said where the disk went, which is the one question the
# ruling on which shards fit a hosted runner needs answered (phase-559 W5b).
#
# A watcher writing to the same disk dies with it, so the watcher holds a
# reserve. At start it `fallocate`s a balloon (default 3 GB) beside the build.
# Every 60 s it appends `df` to the watch log. When free space drops below the
# floor (default 2 GB) it:
#   1. deletes the balloon, giving the runner room to upload and report;
#   2. writes the per-directory `du` of the build's usual suspects, and runs
#      `disk-report.sh`;
#   3. kills the command's process group, so the step FAILS on purpose and
#      says why.
#
# Files (in $NROS_DISK_BALLOON_DIR, else $RUNNER_TEMP, else $TMPDIR):
#   nros-disk-watch.log      timestamped `df` samples
#   nros-disk-exhausted.txt  the du-per-dir summary, written only on exhaustion
#
# A workflow uploads both with `if: always()`. Outside the exhaustion case the
# wrapper's exit status is the command's.
set -uo pipefail

usage() {
    echo "usage: disk-balloon.sh run [--balloon-gb N] [--min-free-gb M] -- <cmd> [args...]" >&2
    exit 2
}

[ "${1:-}" = run ] || usage
shift
balloon_gb=3
min_free_gb=2
interval="${NROS_DISK_BALLOON_INTERVAL:-60}"
while [ $# -gt 0 ]; do
    case "$1" in
        --balloon-gb) balloon_gb="${2:?}"; shift 2 ;;
        --min-free-gb) min_free_gb="${2:?}"; shift 2 ;;
        --) shift; break ;;
        *) usage ;;
    esac
done
[ $# -gt 0 ] || usage

out_dir="${NROS_DISK_BALLOON_DIR:-${RUNNER_TEMP:-${TMPDIR:-/tmp}}}"
mkdir -p "$out_dir"
watch_log="$out_dir/nros-disk-watch.log"
summary="$out_dir/nros-disk-exhausted.txt"
# The balloon lives on the BUILD's filesystem (the cwd), which is the one that
# fills; a balloon on another mount would reserve nothing.
balloon="$(pwd)/.nros-disk-balloon"
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=scripts/lib/deadline.sh
. "$here/../lib/deadline.sh"

free_kb() { df -Pk . | awk 'NR==2 {print $4}'; }

rm -f "$summary"
if fallocate -l "${balloon_gb}G" "$balloon" 2>/dev/null \
    || dd if=/dev/zero of="$balloon" bs=1M count=$((balloon_gb * 1024)) status=none 2>/dev/null; then
    echo "disk-balloon: reserved ${balloon_gb} GB at $balloon; floor ${min_free_gb} GB free"
else
    rm -f "$balloon"
    echo "disk-balloon: could not reserve ${balloon_gb} GB; watching without a reserve" >&2
fi

summarise() {
    {
        echo "== disk exhausted: $(date -u +%Y-%m-%dT%H:%M:%SZ), $(free_kb) KB free after releasing the balloon"
        df -h . / 2>/dev/null
        echo "--- du -sh, biggest first (build dirs, target dirs, caches, stores) ---"
        # shellcheck disable=SC2035
        # Bounded: a summary that hangs costs the reserve it exists to use.
        "${NROS_DEADLINE[@]}" 300 du -sh build/* target* */target* examples/*/*/build examples/*/*/*/build \
            examples/*/*/target* packages/*/*/target* \
            "$HOME/.cache" "$HOME/.cache"/* "$HOME/.nros" "$HOME/.nros"/* \
            "$HOME/.cargo" /tmp 2>/dev/null | sort -rh | head -60
    } > "$summary" 2>&1
    NROS_DISK_TRANSCRIPT="${NROS_DISK_TRANSCRIPT:-}" "${NROS_DEADLINE[@]}" 300 bash "$here/disk-report.sh" "disk exhausted" || true
}

# The command in its own session, so one signal reaches every descendant
# (cargo, make, ninja, rustc), not just the shell that started them.
setsid "$@" &
cmd_pid=$!
# A cancelled or killed wrapper takes the command group down with it, rather
# than leaving a build filling the disk with nothing watching.
trap 'kill -KILL -- "-$cmd_pid" 2>/dev/null; rm -f "$balloon"' EXIT
trap 'exit 143' TERM INT

exhausted=0
floor_kb=$((min_free_gb * 1024 * 1024))
while kill -0 "$cmd_pid" 2>/dev/null; do
    kb="$(free_kb)"
    printf '%s free_kb=%s\n' "$(date -u +%H:%M:%S)" "$kb" >> "$watch_log"
    if [ -n "$kb" ] && [ "$kb" -lt "$floor_kb" ]; then
        rm -f "$balloon"
        exhausted=1
        echo "::error::disk-balloon: free space fell below ${min_free_gb} GB ($kb KB); stopping the build to keep room for its logs. Per-directory usage: $summary" >&2
        # Stop the build FIRST, so it cannot spend the released reserve while
        # the summary runs; what it wrote stays on disk to be measured.
        kill -TERM -- "-$cmd_pid" 2>/dev/null
        sleep 20
        kill -KILL -- "-$cmd_pid" 2>/dev/null
        summarise
        cat "$summary" >&2
        break
    fi
    # Wake early when the command ends instead of sleeping out the interval.
    for _ in $(seq 1 "$interval"); do
        kill -0 "$cmd_pid" 2>/dev/null || break
        sleep 1
    done
done

rc=0
wait "$cmd_pid" || rc=$?
rm -f "$balloon"
printf '%s end free_kb=%s rc=%s exhausted=%s\n' "$(date -u +%H:%M:%S)" "$(free_kb)" "$rc" "$exhausted" >> "$watch_log"
if [ "$exhausted" = 1 ]; then
    exit 1
fi
exit "$rc"
