#!/usr/bin/env bash
#
# Reap what the last job left behind, and keep the disk from rotting.
# phase-395 W6; design: docs/development/multi-agent-ci-workflow.md
# ("Scripts to own the procedure").
#
# A self-hosted runner is fast because it is PERSISTENT. That is the same
# property that lets it rot, and both halves of the rot look like code failures
# to whoever's PR is running when they finally bite.
#
# HALF ONE — LEAKED PROCESSES.
#
# This session found **71 orphaned `add_two_ints_server` processes**, the oldest
# ten days old, each holding a DDS participant. Issue 0659 recorded 59 of the
# same in August. On a developer's box that is wasted RAM; on a shared runner it
# is worse than that — a leaked peer still discovers, still answers, and still
# publishes, so the NEXT job's test sees a participant it did not start. The
# symptom is a flake in somebody else's change.
#
# Why they leak at all is already written down in scripts/build/subtree-guard.sh
# (issue 0762): killing the top of a build chain kills ONE process; everything
# below is reparented and keeps running, and SIGKILL is not trappable, so no
# amount of trap discipline closes it. The honest fix is a reaper that runs
# between jobs — this one.
#
# What it kills, precisely, and why that rule is safe:
#
#   a process is reaped iff  (its exe or cwd resolves under a sweep root)
#                       AND  (it has been REPARENTED — ppid 1, or a subreaper)
#                       AND  (it is older than --older-than, default 600s)
#
# "Reparented" is the discriminator that makes this safe to run on a busy
# machine. A process belonging to a LIVE job has a live parent chain up to
# `Runner.Worker`; only a process whose parent died gets adopted by init. Age
# alone would not do: a legitimate fixture build runs for forty minutes, and a
# sweep keyed on age would kill it. Matching by process NAME would be worse
# still — `cargo`, `ninja` and `qemu-system-arm` are exactly what a live job is
# running.
#
# Process GROUPS, not processes: a group is inherited, so signalling the group
# reaches the whole subtree in one shot. The same reasoning as subtree-guard's,
# and its `nros_guard_reap` is reused here for the build guards that recorded a
# group id before dying.
#
# HALF TWO — DISK.
#
# "the persistence that makes a self-hosted runner fast is the same property
#  that lets it rot. A 9.2 GB SDK, per-coordinate fixture trees, sccache and
#  `build/` all grow without bound, and a runner that fills its disk fails in
#  ways that look like code failures. Budget per label, evict LRU, and report
#  the high-water mark."
#
# So each capability label owns an area with a byte budget; when an area is over
# budget its entries are evicted oldest-first until it is under. Pinned entries
# are never evicted — the Zephyr SDK version the current workspace's own
# `zephyr/SDK_VERSION` demands is the obvious one: evicting it makes the
# `nros-sdk-zephyr` label instantly false, which is precisely the state
# runner-doctor exists to prevent.
#
# The high-water mark is recorded across runs, because a budget you never
# breach and a budget you breach every time look identical in a single
# snapshot, and only the second one is telling you to buy a disk.
#
# COST NOTE. Measuring an area means `du` over it, and that is a real walk of a
# build tree — seconds to a minute or two, not free. It is a BETWEEN-JOBS
# operation and priced accordingly. It is not a `find` for tracked files (issue
# 0844): there is no index of disk usage to consult, and the roots walked are
# build output and SDK trees that git has never seen.
#
# usage:
#   scripts/ci/runner-sweep.sh [--check] [--processes|--disk]
#                              [--older-than SECONDS] [--all] [--quiet]
#
#   --check          report everything; kill nothing, delete nothing. Exit 0 =
#                    clean, 1 = there is something to sweep.
#   --processes      only half one.  --disk  only half two.
#   --older-than N   reparented processes younger than N seconds are left alone
#                    (default 600).
#   --all            ignore the age floor. Only between jobs.
#   --force          sweep processes even while a Runner.Worker is alive. Only
#                    for a wedged job — see `_nros_sweep_job_running` below.
#
# env:
#   NROS_RUNNER_REPO_ROOT        checkout to sweep (default: this file's own)
#   NROS_SWEEP_WORK              runner work dir (default: $RUNNER_WORK, else
#                                $NROS_RUNNER_DIR/_work, else ~/actions-runner/_work)
#   NROS_SWEEP_STATE_DIR         where the high-water mark is kept
#   NROS_SWEEP_BUDGET_ZEPHYR_GIB    default 14
#   NROS_SWEEP_BUDGET_FIXTURES_GIB  default 60
#   NROS_SWEEP_BUDGET_WORK_GIB      default 40
set -euo pipefail

repo_root="${NROS_RUNNER_REPO_ROOT:-}"
if [ -z "$repo_root" ]; then
    repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
fi
repo_root="${repo_root%/}"

CHECK=0
DO_PROC=1
DO_DISK=1
OLDER_THAN=600
QUIET=0
FORCE=0

while [ "$#" -gt 0 ]; do
    case "$1" in
        --check|--dry-run) CHECK=1 ;;
        --processes)       DO_DISK=0 ;;
        --disk)            DO_PROC=0 ;;
        --all)             OLDER_THAN=0 ;;
        --force)           FORCE=1 ;;
        --quiet)           QUIET=1 ;;
        --older-than)      OLDER_THAN="${2:?--older-than needs seconds}"; shift ;;
        -h|--help)
            sed -n '2,105p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'
            exit 0
            ;;
        *)
            echo "runner-sweep: unknown argument '$1'" >&2
            exit 2
            ;;
    esac
    shift
done

_say()  { [ "$QUIET" -eq 1 ] || printf '%s\n' "$*"; }
_warn() { printf '%s\n' "$*" >&2; }

# `--check` and the real run must differ in exactly one way: whether the action
# runs. Routing every mutation through these two makes that true by
# construction, rather than by two code paths that are supposed to agree.
_would() { _say "  [would] $*"; }
_did()   { _say "  [did]   $*"; }

dirty=0   # 0 = nothing found to sweep; 1 = there was (or would be) work

# --- where the runner's work dir is ------------------------------------------
#
# Never a literal: this script runs on a machine that is not the one it was
# written on, under a user that is not the author, possibly with the runner
# installed somewhere the author never considered.
work_dir="${NROS_SWEEP_WORK:-${RUNNER_WORK:-${NROS_RUNNER_DIR:-$HOME/actions-runner}/_work}}"

# The build cache root, from the repo's ONE derivation (RFC-0070 R1). Sourced
# rather than spelled `$repo/build`, because NROS_BUILD_ROOT exists precisely so
# a runner can put the cache tree on a bigger or faster volume — and a sweep
# that measured the wrong volume would report a healthy budget while the real
# one filled.
build_root="$repo_root/build"
if [ -r "$repo_root/scripts/build/build-root.sh" ]; then
    # shellcheck source=/dev/null
    if . "$repo_root/scripts/build/build-root.sh" 2>/dev/null \
       && command -v nros_build_root >/dev/null 2>&1; then
        build_root="$(NROS_REPO_ROOT="$repo_root" nros_build_root)"
    fi
fi

# =============================================================================
# HALF ONE — processes
# =============================================================================

# Is `$1` a path under something this runner owns? Attribution is by PATH, not
# by name: what makes a process ours is that it was built or launched out of
# this checkout, and that survives renaming, wrappers and per-coordinate binary
# names (`add_two_ints_server` appears under a dozen fixture coordinates).
_under_sweep_root() {
    local p="$1"
    [ -n "$p" ] || return 1
    case "$p" in
        "$repo_root"/*)  return 0 ;;
        "$build_root"/*) return 0 ;;
        "$work_dir"/*)   return 0 ;;
    esac
    return 1
}

# Reparented = orphaned. See the header for why this, and not age or name, is
# the discriminator.
#
# `ppid == 1` is the classic form. A user systemd instance registers itself as a
# child subreaper, so on such a host an orphan is adopted by `systemd --user`
# instead — accepted too, by the parent's comm, because otherwise this whole
# half silently does nothing on a systemd-user machine and reports success.
_is_reparented() {
    local ppid="$1" pcomm
    [ "$ppid" = "1" ] && return 0
    pcomm="$(ps -o comm= -p "$ppid" 2>/dev/null | tr -d ' ')"
    case "$pcomm" in
        systemd|init|systemd-user) return 0 ;;
    esac
    return 1
}

# Is a job running RIGHT NOW?
#
# This matters because the reparented-ness test cannot tell a leaked peer from a
# deliberately daemonized one that a LIVE test is using. `ros2 daemon` is the
# case that proved it: run this script's `--check` on a developer box and it
# correctly finds `ros2cli.daemon.daemonize` processes with ppid 1 and a cwd
# inside the checkout — which is exactly right when the job that started them is
# over, and exactly wrong while a Cyclone test is mid-query against one. (This
# repo already learned that a blanket `ros2 daemon stop` under a parallel suite
# is a cross-test kill, not a reset — issue 0763.)
#
# So: between jobs, sweep. During a job, report and stop. `--force` overrides,
# because a wedged worker is a real reason to sweep anyway.
#
# `awk` rather than `grep -q`, deliberately: `grep -q` cannot tell a NON-MATCH
# (exit 1) from a grep that failed to START (exit >= 2), and this repo has
# already been bitten by exactly that under a 32-way fan-out (issue 0726, gated
# by `check-grep-q-error-conflation`). Here a conflation would mean "no job is
# running" whenever the fork failed — i.e. it would sweep during a live job,
# which is the one thing this function exists to prevent.
_nros_sweep_job_running() {
    local hit
    hit="$(ps -eo comm= 2>/dev/null | awk '$1 == "Runner.Worker" { print "y"; exit }' || true)"
    [ "$hit" = "y" ]
}

# Live PIDs in process group `$1`, one per line — the same idiom as
# subtree-guard's `_nros_guard_group_members`, for the same reason, INCLUDING
# the zombie exclusion: on a runner whose container PID 1 is `tail -f /dev/null`
# nothing ever reaps an orphan, so a dead group stays visible to `ps` at its
# original pgid forever (issue 0853). Counting corpses here would make the sweep
# report debris it cannot clear.
_nros_sweep_group_members() {
    ps -eo pid=,pgid=,stat= 2>/dev/null |
        awk -v g="$1" '$2 == g && $3 !~ /^Z/ { print $1 }' || true
}

_sweep_processes() {
    if ! ps -eo pid=,ppid=,pgid=,etimes=,stat=,comm= >/dev/null 2>&1; then
        _warn "runner-sweep: this \`ps\` has no \`etimes\` column — process sweep SKIPPED."
        _warn "  Needs procps-ng. Nothing was killed; the disk half still ran."
        return 0
    fi

    local own_pgid
    own_pgid="$(ps -o pgid= -p "$$" 2>/dev/null | tr -d ' ' || true)"

    # First, the build guards that recorded a process-group id before their
    # launcher was SIGKILLed. subtree-guard already knows how to distinguish
    # "a build is genuinely running" from "the launcher is gone and this is
    # debris", and that decision is subtle enough that a second implementation
    # here would be a second answer.
    local lock_dir="${NROS_GUARD_LOCK_DIR:-${TMPDIR:-/tmp}/nros-build-guards}"
    if [ -d "$lock_dir" ] && [ -r "$repo_root/scripts/build/subtree-guard.sh" ]; then
        # shellcheck source=/dev/null
        . "$repo_root/scripts/build/subtree-guard.sh" 2>/dev/null || true
        local lock name lock_pgid
        for lock in "$lock_dir"/*.pgid; do
            [ -f "$lock" ] || continue
            name="$(basename "$lock" .pgid)"
            # `<launcher-pid> <payload-pgid>`. A lock whose group has no live
            # members is just a leftover file — reporting it as "work to do"
            # would make every run look dirty and train the operator to ignore
            # the verdict, which is the failure mode a verdict has.
            lock_pgid="$(awk '{print $2}' "$lock" 2>/dev/null || true)"
            case "${lock_pgid:-}" in
                ''|*[!0-9]*) continue ;;
            esac
            [ -n "$(_nros_sweep_group_members "$lock_pgid")" ] || continue
            dirty=1
            if [ "$CHECK" -eq 1 ]; then
                _would "hand '$name' (pgid $lock_pgid) to subtree-guard's reaper ($lock)"
            else
                _did "subtree-guard reap: $name (pgid $lock_pgid)"
                nros_guard_reap "$name" || true
            fi
        done
    fi

    # Then the general case: anything reparented, old enough, and ours.
    local pid ppid pgid age state comm exe cwd
    local -a victims=()
    local reported=0
    while read -r pid ppid pgid age state comm; do
        [ -n "${pid:-}" ] || continue
        [ "$pid" = "$$" ] && continue
        [ "$pid" -le 1 ] 2>/dev/null && continue
        # A zombie is not an orphan to sweep: it holds no CPU, no files and no
        # port, and it cannot be killed — only its parent can collect it, and
        # under a non-reaping container PID 1 nobody ever will. Left in, every
        # one of them would land in the `reported` branch below (a zombie's
        # /proc/<pid>/exe is unreadable) and the sweep would tell the operator
        # to re-run as another user, forever. → issue 0853.
        case "$state" in Z*) continue ;; esac
        [ -n "$own_pgid" ] && [ "$pgid" = "$own_pgid" ] && continue
        [ "$pgid" -le 1 ] 2>/dev/null && continue

        # The runner's own processes are never candidates even if something
        # about them looks orphaned: killing the listener takes the machine off
        # the fleet, and killing a worker fails the job that is running.
        case "$comm" in
            Runner.Listener|Runner.Worker|runsvc.sh|systemd*) continue ;;
        esac

        [ "$OLDER_THAN" -gt 0 ] && [ "$age" -lt "$OLDER_THAN" ] 2>/dev/null && continue
        _is_reparented "$ppid" || continue

        # /proc is readable only for our own processes unless root. An
        # unreadable one is REPORTED rather than silently skipped — "the sweep
        # cannot see half the machine" is something an operator must know, and
        # it is exactly the shape of a gate that quietly checks nothing.
        exe="$(readlink "/proc/$pid/exe" 2>/dev/null || true)"
        cwd="$(readlink "/proc/$pid/cwd" 2>/dev/null || true)"
        if [ -z "$exe" ] && [ -z "$cwd" ]; then
            if [ -d "/proc/$pid" ]; then
                reported=$((reported + 1))
            fi
            continue
        fi
        _under_sweep_root "$exe" || _under_sweep_root "$cwd" || continue

        dirty=1
        _say "  orphan pid=$pid pgid=$pgid age=${age}s comm=$comm"
        _say "         exe=${exe:-?}"
        victims+=("$pgid")
    done < <(ps -eo pid=,ppid=,pgid=,etimes=,stat=,comm= 2>/dev/null)

    if [ "$reported" -gt 0 ]; then
        _warn "runner-sweep: $reported reparented process(es) belong to another user —"
        _warn "  /proc/<pid>/exe is unreadable, so they cannot be attributed and were left."
        _warn "  A runner should own its machine; if these are yours, run the sweep as"
        _warn "  the runner user."
    fi

    if [ "${#victims[@]}" -eq 0 ]; then
        _say "  no orphaned process groups under $repo_root / $build_root / $work_dir"
        return 0
    fi

    # Unique groups. Signalling a group twice is harmless but the report should
    # say how many GROUPS, since that is the unit of the kill.
    local -a groups=()
    local g seen
    for g in "${victims[@]}"; do
        seen=0
        local h
        for h in ${groups[@]+"${groups[@]}"}; do
            if [ "$h" = "$g" ]; then seen=1; fi
        done
        if [ "$seen" -eq 0 ]; then groups+=("$g"); fi
    done

    _say "  ${#groups[@]} orphaned process group(s): ${groups[*]}"
    if [ "$CHECK" -eq 1 ]; then
        _would "kill -TERM -<pgid> then -KILL for: ${groups[*]}"
        return 0
    fi
    if [ "$FORCE" -eq 0 ] && _nros_sweep_job_running; then
        _warn "runner-sweep: a Runner.Worker is ALIVE — not killing anything."
        _warn "  This is a between-jobs operation. A daemonized helper the running job"
        _warn "  depends on (a ros2cli daemon, an XRCE agent) is indistinguishable from"
        _warn "  a leak while that job is still using it."
        _warn "  Re-run after the job, or override with --force."
        return 0
    fi

    for g in "${groups[@]}"; do
        _did "kill -TERM -$g"
        kill -TERM -"$g" 2>/dev/null || true
    done
    # TERM first, then KILL. A DDS participant that is given a chance to leave
    # announces its departure; one that is SIGKILLed leaves its peers to time it
    # out, which is a slower and noisier way to reach the same place.
    local waited=0
    while [ "$waited" -lt 10 ]; do
        local alive=0
        for g in "${groups[@]}"; do
            if [ -n "$(_nros_sweep_group_members "$g")" ]; then alive=1; fi
        done
        if [ "$alive" -eq 0 ]; then break; fi
        sleep 1
        waited=$((waited + 1))
    done
    for g in "${groups[@]}"; do
        if [ -n "$(_nros_sweep_group_members "$g")" ]; then
            _did "pgid $g ignored SIGTERM after ${waited}s — kill -KILL -$g"
            kill -KILL -"$g" 2>/dev/null || true
        fi
    done
}

# =============================================================================
# HALF TWO — disk
# =============================================================================

# Human-readable bytes. Integer GiB alone was the first spelling and it printed
# `0 GiB` for every eviction candidate under a gigabyte — so a report listing
# fifteen deletions summed visibly to less than it freed, which reads as a bug in
# the arithmetic rather than in the formatting.
_human() {
    awk -v b="${1:-0}" 'BEGIN {
        if (b >= 1073741824)   printf "%.1f GiB", b / 1073741824;
        else if (b >= 1048576) printf "%.0f MiB", b / 1048576;
        else if (b >= 1024)    printf "%.0f KiB", b / 1024;
        else                   printf "%d B", b;
    }'
}

# Bytes used by a path, one filesystem only (`-x`): a bind-mounted or
# separately-mounted subtree belongs to whoever owns that mount, not to this
# budget.
_du_bytes() {
    [ -e "$1" ] || { printf '0'; return 0; }
    local n
    # `|| true` on BOTH halves: `du` exits non-zero for a single unreadable
    # subdirectory, and under `pipefail` that would fail an assignment and, with
    # `set -e`, end the sweep — a partial measurement is worth far more here
    # than an abort.
    n="$(du -sx --block-size=1 "$1" 2>/dev/null | awk '{print $1; exit}' || true)"
    case "${n:-}" in
        ''|*[!0-9]*) printf '0' ;;
        *)           printf '%s' "$n" ;;
    esac
    return 0
}

state_dir="${NROS_SWEEP_STATE_DIR:-${XDG_STATE_HOME:-$HOME/.local/state}/nros-runner-sweep}"
hwm_file="$state_dir/high-water.tsv"

# The high-water mark is the number that tells an operator whether a budget is
# comfortable or permanently breached. A single snapshot cannot: an area sitting
# at 90% might be steady or might be the trough between two evictions.
_hwm_get() {
    local n=""
    if [ -f "$hwm_file" ]; then
        n="$(awk -F'\t' -v a="$1" '$1 == a { print $2; exit }' "$hwm_file" 2>/dev/null || true)"
    fi
    case "${n:-}" in
        ''|*[!0-9]*) printf '0' ;;
        *)           printf '%s' "$n" ;;
    esac
    return 0
}
# Best-effort by design. A runner that cannot write its state dir should still
# sweep: losing the high-water history is an inconvenience, refusing to reap
# orphans over it is a failure.
_hwm_put() {
    local area="$1" bytes="$2" tmp
    if [ "$CHECK" -eq 1 ]; then
        return 0
    fi
    mkdir -p "$state_dir" 2>/dev/null || return 0
    tmp="$hwm_file.$$"
    {
        if [ -f "$hwm_file" ]; then
            awk -F'\t' -v a="$area" '$1 != a' "$hwm_file" || true
        fi
        printf '%s\t%s\t%s\n' "$area" "$bytes" "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
    } > "$tmp" 2>/dev/null || true
    if [ -s "$tmp" ]; then
        mv -f "$tmp" "$hwm_file" 2>/dev/null || rm -f "$tmp"
    else
        rm -f "$tmp"
    fi
    return 0
}

# Evict entries from an area, oldest mtime first, until it fits its budget.
#
#   _evict <area> <budget-bytes> <pinned-glob-or-empty> <candidate-dir>...
#
# Candidates are the CHILDREN of each candidate dir — one eviction unit each.
# Never the dir itself: deleting `build/cargo-fixtures` wholesale turns a
# budget overrun into a full rebuild of everything, when dropping the three
# coldest coordinates would have done.
_evict() {
    local area="$1" budget="$2" pinned="$3"
    shift 3
    local dirs=("$@")

    local total=0 d sz
    for d in "${dirs[@]}"; do
        [ -d "$d" ] || continue
        sz="$(_du_bytes "$d")"
        total=$((total + sz))
    done
    if [ "$total" -eq 0 ]; then
        _say "  $area: nothing on disk"
        return 0
    fi

    local hwm
    hwm="$(_hwm_get "$area")"
    if [ "$total" -gt "$hwm" ]; then
        hwm="$total"
    fi
    _hwm_put "$area" "$hwm"

    _say "  $area: $(_human "$total") used / $(_human "$budget") budget (high-water $(_human "$hwm"))"

    if [ "$total" -le "$budget" ]; then
        return 0
    fi
    dirty=1
    _say "  $area: OVER BUDGET by $(_human $((total - budget))) — evicting least-recently-modified entries"

    # `<mtime>\t<path>` for every eviction unit, oldest first. Built with globs
    # rather than `find`: one level, no walk, and nothing here is a file git
    # tracks (issue 0844's rule is about tracked files; these are build output
    # and unpacked SDKs, which the index has never seen).
    local listing="" child
    for d in "${dirs[@]}"; do
        [ -d "$d" ] || continue
        for child in "$d"/*; do
            [ -e "$child" ] || continue
            if [ -n "$pinned" ]; then
                case "$child" in
                    $pinned) _say "      pinned, never evicted: $child"; continue ;;
                esac
            fi
            listing="${listing}$(stat -c '%Y' "$child" 2>/dev/null || echo 0)"$'\t'"$child"$'\n'
        done
    done

    local mtime path freed=0
    while IFS=$'\t' read -r mtime path; do
        [ -n "${path:-}" ] || continue
        [ "$((total - freed))" -le "$budget" ] && break
        sz="$(_du_bytes "$path")"
        if [ "$CHECK" -eq 1 ]; then
            _would "rm -rf $path  ($(_human "$sz"), mtime $(date -d "@$mtime" -u +%Y-%m-%d 2>/dev/null || echo "$mtime"))"
        else
            _did "rm -rf $path  ($(_human "$sz"))"
            rm -rf "$path"
        fi
        freed=$((freed + sz))
    done < <(printf '%s' "$listing" | sort -n)

    if [ "$CHECK" -eq 1 ]; then
        _say "  $area: $(_human "$freed") would be freed"
    else
        _say "  $area: $(_human "$freed") freed"
    fi
    if [ "$((total - freed))" -gt "$budget" ]; then
        _warn "runner-sweep: $area is still over budget after evicting everything evictable."
        _warn "  Either the budget is too small for this runner's labels, or something"
        _warn "  outside the eviction units is holding the space. Raise the budget"
        _warn "  deliberately rather than letting the disk decide."
    fi
}

_sweep_disk() {
    if ! command -v du >/dev/null 2>&1; then
        _warn "runner-sweep: no \`du\` — disk sweep SKIPPED (nothing was measured or deleted)."
        return 0
    fi

    local gib=$((1024 * 1024 * 1024))

    # --- nros-sdk-zephyr ------------------------------------------------------
    #
    # The 9.2 GB measurement in the design doc (sdk/ 7.8 + downloads/ 1.4) is a
    # SINGLE SDK. The store accumulates — issue 0500's lesson one directory over
    # — so a runner that has tracked two Zephyr lines carries two, and the
    # budget is what stops the third from being a surprise.
    #
    # The version this workspace's own zephyr/SDK_VERSION demands is PINNED.
    # Evicting it would make the nros-sdk-zephyr label instantly false, which is
    # the exact state runner-doctor exists to catch — a sweep that creates work
    # for the doctor is a sweep that has misunderstood its job.
    local zsdk="$repo_root/scripts/zephyr/sdk"
    local zdl="$repo_root/scripts/zephyr/downloads"
    if [ -d "$zsdk" ] || [ -d "$zdl" ]; then
        local ws="${NROS_ZEPHYR_WORKSPACE:-$repo_root/zephyr-workspace}"
        local want="" pin=""
        [ -f "$ws/zephyr/SDK_VERSION" ] && want="$(cat "$ws/zephyr/SDK_VERSION" 2>/dev/null)"
        [ -n "$want" ] && pin="$zsdk/zephyr-sdk-$want"
        [ -n "$pin" ] && _say "  zephyr-sdk: pinned by $ws/zephyr/SDK_VERSION -> $pin"
        _evict zephyr-sdk \
            $(( ${NROS_SWEEP_BUDGET_ZEPHYR_GIB:-14} * gib )) \
            "$pin" \
            "$zsdk" "$zdl"
    fi

    # --- fixture / build caches ----------------------------------------------
    #
    # One eviction unit per COORDINATE (RFC-0070 R2 is `<root>/<kind>/<coord>`),
    # so a budget overrun drops the coldest platform×lang×rmw cells and leaves
    # the warm ones. Nothing here is pinned: every one of these is rebuildable
    # by definition, which is what makes them the right thing to evict first.
    #
    # Evicting a fixture makes the next job's staleness probe rebuild it. That
    # is correct and it is also why the budget should be generous: the whole
    # reason for a persistent runner is not paying that cost.
    local -a cache_dirs=()
    local kind
    for kind in cargo-fixtures cmake-fixtures west-fixtures idf-fixtures \
                corrosion-cargo example-build example-lint \
                zephyr-fixture-build compile-check-fixtures; do
        [ -d "$build_root/$kind" ] && cache_dirs+=("$build_root/$kind")
    done
    if [ "${#cache_dirs[@]}" -gt 0 ]; then
        _evict fixtures \
            $(( ${NROS_SWEEP_BUDGET_FIXTURES_GIB:-60} * gib )) \
            "" \
            "${cache_dirs[@]}"
    else
        _say "  fixtures: no build caches under $build_root"
    fi

    # --- the runner's own work dir -------------------------------------------
    #
    # `_actions` (downloaded action code) and `_temp` (per-job scratch) are pure
    # cache and grow forever. The job CHECKOUT is deliberately NOT an eviction
    # unit: on an ephemeral runner it is recreated anyway, and deleting it
    # mid-sweep costs a full clone for no budget gain that dropping `_actions`
    # would not also give.
    local -a work_dirs=()
    [ -d "$work_dir/_actions" ] && work_dirs+=("$work_dir/_actions")
    [ -d "$work_dir/_temp" ]    && work_dirs+=("$work_dir/_temp")
    if [ "${#work_dirs[@]}" -gt 0 ]; then
        _evict runner-work \
            $(( ${NROS_SWEEP_BUDGET_WORK_GIB:-40} * gib )) \
            "" \
            "${work_dirs[@]}"
    else
        _say "  runner-work: no _actions/_temp under $work_dir"
    fi

    # --- sccache: reported, never evicted ------------------------------------
    #
    # sccache enforces its own cap (SCCACHE_CACHE_SIZE) and manages its own LRU.
    # A second evictor over the same directory would fight it and would corrupt
    # the index it keeps. Reported because it is real disk an operator will
    # otherwise fail to account for.
    local sc="${SCCACHE_DIR:-$HOME/.cache/sccache}"
    if [ -d "$sc" ]; then
        local scb
        scb="$(_du_bytes "$sc")"
        _hwm_put sccache "$scb"
        _say "  sccache: $(_human "$scb") at $sc — self-capped (SCCACHE_CACHE_SIZE), not evicted here"
    fi

    # --- the filesystem itself -----------------------------------------------
    #
    # Budgets are per-area; the disk is shared with everything else on the
    # machine, including the unrelated research work these boxes carry. Report
    # the real number last, because it is the one that actually fails builds.
    local avail
    avail="$(df -P -BG "$repo_root" 2>/dev/null | awk 'NR==2 {print $4}' || true)"
    if [ -n "$avail" ]; then
        _say "  filesystem: $avail free on the checkout's volume"
    fi
    return 0
}

# =============================================================================

_say "runner-sweep: checkout   $repo_root"
_say "runner-sweep: build root $build_root"
_say "runner-sweep: work dir   $work_dir"
if [ "$CHECK" -eq 1 ]; then
    _say "runner-sweep: --check — nothing will be killed or deleted."
fi

if [ "$DO_PROC" -eq 1 ]; then
    _say ""
    _say "== processes =="
    _sweep_processes
fi

if [ "$DO_DISK" -eq 1 ]; then
    _say ""
    _say "== disk =="
    _sweep_disk
fi

_say ""
if [ "$CHECK" -eq 1 ] && [ "$dirty" -eq 1 ]; then
    _say "runner-sweep: --check — there IS work to do (see the [would] lines)."
    exit 1
fi
if [ "$dirty" -eq 1 ]; then
    _say "runner-sweep: done — the machine had leftovers and they were cleared."
else
    _say "runner-sweep: done — nothing to sweep."
fi
exit 0
