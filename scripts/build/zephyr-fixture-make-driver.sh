#!/usr/bin/env bash
# Generate and run a temporary make graph for Zephyr fixture records.
#
# Phase 226 Z3: replace only the scheduling layer. Zephyr fixture records still
# come from scripts/build/zephyr-fixture-leaves.sh, and each leaf is executed by
# scripts/build/zephyr-fixture-run-one.sh.
set -euo pipefail

usage() {
    cat >&2 <<'EOF'
usage: scripts/build/zephyr-fixture-make-driver.sh [--dry-run] [--keep] [LEAF OPTIONS...]

Generates Zephyr fixture records, writes a temporary makefile/joblog/status/log
layer under build/zephyr-fixture-make-driver/, and schedules one runner process
per record.

Options:
  --dry-run, -n   generate and print the make command without executing it
  --keep          keep generated records and makefile after a successful run
  -h, --help      show this help

All other arguments are passed to scripts/build/zephyr-fixture-leaves.sh after
--emit records, for example:
  scripts/build/zephyr-fixture-make-driver.sh --dry-run --filter build-rs-talker-zenoh
EOF
}

dry_run=0
keep=0
leaf_args=()
while [ "$#" -gt 0 ]; do
    case "$1" in
        --dry-run|-n)
            dry_run=1
            ;;
        --keep)
            keep=1
            ;;
        --help|-h)
            usage
            exit 0
            ;;
        *)
            leaf_args+=("$1")
            ;;
    esac
    shift
done

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$script_dir/../.." && pwd)"
leaves_script="$repo_root/scripts/build/zephyr-fixture-leaves.sh"
runner_script="$repo_root/scripts/build/zephyr-fixture-run-one.sh"

if [ ! -x "$leaves_script" ]; then
    echo "zephyr-fixture-make-driver: missing executable $leaves_script" >&2
    exit 2
fi
if [ ! -f "$runner_script" ]; then
    echo "zephyr-fixture-make-driver: missing $runner_script" >&2
    echo "zephyr-fixture-make-driver: Z2 one-leaf runner must land before non-dry-run execution" >&2
    if [ "$dry_run" != "1" ]; then
        exit 2
    fi
fi

stamp="$(date +%Y%m%d-%H%M%S)-$$-$RANDOM"
# RFC-0070 R1/R3 — cache paths come from the ONE derivation, so
# `NROS_BUILD_ROOT` moves this writer with every other. Default is
# `<repo>/build`, so the emitted path is unchanged.
# shellcheck source=scripts/build/build-root.sh
. "$(dirname "${BASH_SOURCE[0]}")/build-root.sh"
work_root="$(nros_build_dir "$NROS_KIND_ZEPHYR_FIXTURE_MAKE_DRIVER")"
record_dir="$work_root/records/$stamp"
# issue 0446 — the runner takes this path as its ARGUMENT (`record_path="${1:-}"`
# in zephyr-fixture-run-one.sh). It was ALSO exported as
# `NROS_ZEPHYR_RUNNER_RECORD`, which nothing ever read — zero references in the
# tree outside the two lines that set it.
#
# A dead export is not free here. `nros-sizes-build::knob_identity()` sweeps
# EVERY `NROS_*` in the environment into the sizes-probe key, deliberately, so
# an unknown knob costs a directory rather than risking issue 0528's
# order-dependent corruption. `$stamp` is a timestamp+pid, so this one differed
# on every run and every Zephyr fixture build minted probe keys that could never
# be reused — the mechanism phase-353 W4 removed for three other names, arriving
# by a fourth. Deleting the export is the fix; excluding it would have been
# bookkeeping for a variable with no consumer.
log_dir="$work_root/logs/$stamp"
status_dir="$work_root/status/$stamp"
joblog="$work_root/joblog-$stamp.tsv"
makefile="$work_root/zephyr-fixtures-$stamp.mk"
records_file="$work_root/records-$stamp.tsv"
mkdir -p "$record_dir" "$log_dir" "$status_dir"
printf 'target\tid\tstatus\tstart_epoch\tend_epoch\tduration_s\tscheduler_log\tzephyr_log\trecord\n' >"$joblog"

"$leaves_script" --emit records "${leaf_args[@]}" >"$records_file"
if [ ! -s "$records_file" ]; then
    echo "zephyr-fixture-make-driver: no Zephyr fixture records emitted" >&2
    exit 1
fi

shell_quote() {
    printf '%q' "$1"
}

target_name_for() {
    local value="$1"
    value="${value//\//-}"
    value="${value//:/-}"
    value="${value// /-}"
    value="${value//[^A-Za-z0-9_.-]/-}"
    printf '%s' "$value"
}

make_bin="make"
make_args=()
fallback_ninja_jobs="${NROS_ZEPHYR_NINJA_JOBS:-${NROS_BUILD_JOBS:-$(nproc 2>/dev/null || echo 8)}}"
jobserver_mode=0
if [ -x "$(nros sdk-path make)/bin/make" ] && \
   "$(nros sdk-path make)/bin/make" --version | head -1 | grep -q "4.4"; then
    make_bin="$(nros sdk-path make)/bin/make"
    # In fifo-jobserver mode the -j value IS the shared token pool: every
    # leaf's ninja joins it, so it bounds TOTAL concurrency across the
    # family, and leaves build into disjoint build-<name> dirs (parallel-safe
    # by design — see the issue #19 note below; only cross-INVOCATION overlap
    # needs the lock). It therefore defaults to nproc, not 1: the old
    # `:-1` default handed the whole family a single token — one compiler
    # process at a time across ~56 west builds on any host that didn't set
    # NROS_BUILD_JOBS, which is why the standalone zephyr lane crawled while
    # the no-jobserver fallback (each ninja -j nproc) ran circles around it.
    # NROS_ZEPHYR_JOBSERVER_TOKENS is the pool size the CALLER intends
    # (zephyr-ci.just passes the full family budget, or the user's explicit
    # NROS_ZEPHYR_BUILD_JOBS when they pinned concurrency — the serial
    # escape hatch). Falling back to NROS_ZEPHYR_BUILD_JOBS here kept the
    # old behavior for direct driver invocations.
    outer_jobs="${NROS_ZEPHYR_JOBSERVER_TOKENS:-${NROS_ZEPHYR_BUILD_JOBS:-${NROS_BUILD_JOBS:-$(nproc 2>/dev/null || echo 8)}}}"
    make_args=(-j"$outer_jobs" --jobserver-style=fifo)
    jobserver_mode=1
    export PATH="$(nros sdk-path make)/bin:$(nros sdk-path ninja)/bin:$PATH"
    echo "zephyr-fixture-make-driver: using pinned fifo make"
else
    # No shared token pool here: each leaf's ninja runs -j$fallback_ninja_jobs
    # on its own, so a serial outer walk is what keeps total load ≈ one
    # machine-width (outer N would oversubscribe to N × ninja_jobs).
    outer_jobs="${NROS_ZEPHYR_BUILD_JOBS:-${NROS_BUILD_JOBS:-1}}"
    make_args=(-j"$outer_jobs")
    echo "zephyr-fixture-make-driver: pinned fifo make not found; using ordinary make fallback"
fi

targets=()
leaf_details=()
index=0
while IFS= read -r record; do
    kind="$(printf '%s\n' "$record" | awk -F '\t' '{print $1}')"
    [ -n "${kind:-}" ] || continue
    id="$(printf '%s\n' "$record" | awk -F '\t' '{print $2}')"
    build_name="$(printf '%s\n' "$record" | awk -F '\t' '{print $11}')"
    zephyr_log="$(printf '%s\n' "$record" | awk -F '\t' '{print $13}')"
    index=$((index + 1))
    name="$(target_name_for "${build_name:-$id}")"
    target="zephyr-fixture-$index-$name"
    record_file="$record_dir/$name-$index.tsv"
    scheduler_log="$log_dir/$name.log"
    status_file="$status_dir/$name.status"
    printf '%s\n' "$record" >"$record_file"
    targets+=("$target")
    leaf_details+=("  target=$target id=$id scheduler_log=$scheduler_log zephyr_log=$zephyr_log record=$record_file")
done <"$records_file"

if [ "${#targets[@]}" -eq 0 ]; then
    echo "zephyr-fixture-make-driver: no schedulable Zephyr fixture records found" >&2
    exit 1
fi

{
    printf '# Generated by scripts/build/zephyr-fixture-make-driver.sh at %s\n' "$stamp"
    printf 'SHELL := /bin/bash\n'
    printf '.SHELLFLAGS := -eu -o pipefail -c\n'
    printf '.DELETE_ON_ERROR:\n'
    printf '.PHONY: all'
    for target in "${targets[@]}"; do
        printf ' %s' "$target"
    done
    printf '\n\n'
    printf 'all:'
    for target in "${targets[@]}"; do
        printf ' %s' "$target"
    done
    printf '\n\n'

    index=0
    while IFS= read -r record; do
        kind="$(printf '%s\n' "$record" | awk -F '\t' '{print $1}')"
        [ -n "${kind:-}" ] || continue
        id="$(printf '%s\n' "$record" | awk -F '\t' '{print $2}')"
        build_name="$(printf '%s\n' "$record" | awk -F '\t' '{print $11}')"
        zephyr_log="$(printf '%s\n' "$record" | awk -F '\t' '{print $13}')"
        index=$((index + 1))
        name="$(target_name_for "${build_name:-$id}")"
        target="zephyr-fixture-$index-$name"
        record_file="$record_dir/$name-$index.tsv"
        scheduler_log="$log_dir/$name.log"
        status_file="$status_dir/$name.status"

        printf '%s:\n' "$target"
        printf '\t+@echo "zephyr fixture: %s"\n' "$id"
        printf '\t+@start=$$(date +%%s); status=0; echo "running" >%s; ' "$(shell_quote "$status_file")"
        if [ "$jobserver_mode" = "1" ]; then
            printf '( env NROS_JOBSERVER=1 NROS_ZEPHYR_JOBSERVER=1 %s %s ) >%s 2>&1 || status=$$?; ' \
                "$(shell_quote "$runner_script")" "$(shell_quote "$record_file")" "$(shell_quote "$scheduler_log")"
        else
            printf '( env NROS_ZEPHYR_NINJA_JOBS=%s %s %s ) >%s 2>&1 || status=$$?; ' \
                "$(shell_quote "$fallback_ninja_jobs")" "$(shell_quote "$runner_script")" "$(shell_quote "$record_file")" "$(shell_quote "$scheduler_log")"
        fi
        printf 'end=$$(date +%%s); duration=$$((end - start)); if [ "$$status" -eq 0 ]; then state=ok; else state=fail; fi; '
        printf 'printf "target=%%s\\nid=%%s\\nstatus=%%s\\nstart_epoch=%%s\\nend_epoch=%%s\\nduration_s=%%s\\nscheduler_log=%%s\\nzephyr_log=%%s\\nrecord=%%s\\n" %s %s "$$state" "$$start" "$$end" "$$duration" %s %s %s >%s; ' \
            "$(shell_quote "$target")" "$(shell_quote "$id")" "$(shell_quote "$scheduler_log")" "$(shell_quote "$zephyr_log")" "$(shell_quote "$record_file")" "$(shell_quote "$status_file")"
        printf 'printf "%%s\\t%%s\\t%%s\\t%%s\\t%%s\\t%%s\\t%%s\\t%%s\\t%%s\\n" %s %s "$$state" "$$start" "$$end" "$$duration" %s %s %s >>%s; ' \
            "$(shell_quote "$target")" "$(shell_quote "$id")" "$(shell_quote "$scheduler_log")" "$(shell_quote "$zephyr_log")" "$(shell_quote "$record_file")" "$(shell_quote "$joblog")"
        printf 'if [ "$$status" -ne 0 ]; then echo "zephyr-fixture-make-driver: %s failed; scheduler log tail:" >&2; tail -n "$${NROS_FIXTURE_FAIL_TAIL:-80}" %s >&2 || true; if [ -f %s ]; then echo "zephyr-fixture-make-driver: Zephyr log tail:" >&2; tail -n "$${NROS_FIXTURE_FAIL_TAIL:-80}" %s >&2 || true; fi; exit "$$status"; fi\n' \
            "$target" "$(shell_quote "$scheduler_log")" "$(shell_quote "$zephyr_log")" "$(shell_quote "$zephyr_log")"
    done <"$records_file"
} >"$makefile"

echo "zephyr-fixture-make-driver: makefile=$makefile"
echo "zephyr-fixture-make-driver: records=$records_file"
echo "zephyr-fixture-make-driver: record-dir=$record_dir"
echo "zephyr-fixture-make-driver: scheduler-log-dir=$log_dir"
echo "zephyr-fixture-make-driver: status-dir=$status_dir"
echo "zephyr-fixture-make-driver: joblog=$joblog"
echo "zephyr-fixture-make-driver: outer-jobs=$outer_jobs"
if [ "$jobserver_mode" = "1" ]; then
    echo "zephyr-fixture-make-driver: leaf-mode=fifo-jobserver"
else
    echo "zephyr-fixture-make-driver: leaf-mode=fallback fallback-ninja-jobs=$fallback_ninja_jobs"
fi
echo "zephyr-fixture-make-driver: targets=${targets[*]}"
echo "zephyr-fixture-make-driver: command=$make_bin ${make_args[*]} -f $makefile"

if [ "$dry_run" = "1" ]; then
    echo "zephyr-fixture-make-driver: leaves:"
    for detail in "${leaf_details[@]}"; do
        echo "$detail"
    done
    echo "zephyr-fixture-make-driver: dry run; not executing make"
    exit 0
fi

# issue #19 — serialize concurrent invocations on the shared Zephyr workspace.
# Within ONE invocation, leaves use disjoint `build-<name>` dirs (safe under
# `-j`), but two OVERLAPPING `just zephyr build-fixtures` runs (e.g. a manual
# build alongside a CI/agent build) write the same `zephyr-workspace/build-*`
# trees and race — a torn-down build dir surfaces as garbled cmake/ninja errors
# and `nros-c` size-probe `.fingerprint` write failures. A repo-level advisory
# lock makes a second invocation queue instead of clobbering the first. The
# lock is held only for the build phase and auto-releases when fd 9 closes on
# exit. flock-absent hosts skip it (best-effort).
lockfile="$(nros_build_dir "$NROS_KIND_ZEPHYR_FIXTURE_BUILD").lock"
mkdir -p "$(dirname "$lockfile")"
exec 9>"$lockfile"
if command -v flock >/dev/null 2>&1; then
    if ! flock -n 9; then
        echo "zephyr-fixture-make-driver: another zephyr fixture build holds $lockfile; waiting…"
        flock 9
    fi
    echo "zephyr-fixture-make-driver: acquired build lock $lockfile"
fi

env -u MAKEFLAGS -u CARGO_MAKEFLAGS "$make_bin" "${make_args[@]}" -f "$makefile"

echo "zephyr-fixture-make-driver: joblog=$joblog"

if [ "$keep" != "1" ]; then
    rm -f "$makefile" "$records_file"
    rm -rf "$record_dir"
else
    echo "zephyr-fixture-make-driver: kept makefile=$makefile"
    echo "zephyr-fixture-make-driver: kept records=$records_file"
    echo "zephyr-fixture-make-driver: kept record-dir=$record_dir"
fi
