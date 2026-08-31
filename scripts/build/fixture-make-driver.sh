#!/usr/bin/env bash
# Generate and run a temporary make graph for manifest-driven fixture leaves.
#
# Phase 226: this emits selected fixture groups and calls the existing grouped
# builder (scripts/build/fixtures-build.sh <platform> <lang> [rmw]) as each
# make leaf.
set -euo pipefail

usage() {
    cat >&2 <<'EOF'
usage: scripts/build/fixture-make-driver.sh [--dry-run] [--keep] <platform|all|linux-cyclonedds-rust|linux-cmake-rmw|linux-cyclonedds-cmake>

Generates a temporary makefile, joblog, and leaf logs under build/fixture-make-driver/.
Current scope:
  linux                   linux (host) manifest-driven fixture groups
  linux-cyclonedds-rust   linux Rust talker/listener Cyclone pure-Cargo leaves
  linux-cmake-rmw         linux C/C++ Zenoh and XRCE CMake fixture groups
  linux-cyclonedds-cmake  linux C/C++ Cyclone CMake fixture groups

Options:
  --dry-run, -n   generate and print the make command without executing it
  --keep         keep the generated makefile after a successful run
EOF
}

dry_run=0
keep=0
scope=""
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
        -*)
            usage
            exit 2
            ;;
        *)
            if [ -n "$scope" ]; then
                usage
                exit 2
            fi
            scope="$1"
            ;;
    esac
    shift
done

if [ -z "$scope" ]; then
    usage
    exit 2
fi

case "$scope" in
    linux|all|linux-cyclonedds-rust|linux-cmake-rmw|linux-cyclonedds-cmake)
        ;;
    *)
        echo "fixture-make-driver: unsupported platform for skeleton: $scope" >&2
        echo "fixture-make-driver: current scope is linux manifest groups and linux CMake/Cyclone Rust leaves only" >&2
        exit 2
        ;;
esac

repo_root="$(pwd)"
if [ ! -f "$repo_root/examples/fixtures.toml" ] || [ ! -x "$repo_root/scripts/build/fixtures-build.sh" ]; then
    echo "fixture-make-driver: run from the nano-ros repository root" >&2
    exit 2
fi

stamp="$(date +%Y%m%d-%H%M%S)-$$-$RANDOM"
# RFC-0070 R1/R3 — cache paths come from the ONE derivation, so
# `NROS_BUILD_ROOT` moves this writer with every other. Default is
# `<repo>/build`, so the emitted path is unchanged.
# shellcheck source=scripts/build/build-root.sh
. "$(dirname "${BASH_SOURCE[0]}")/build-root.sh"
work_root="$(nros_build_dir "$NROS_KIND_FIXTURE_MAKE_DRIVER")"
log_dir="$work_root/logs/$stamp"
status_dir="$work_root/status/$stamp"
joblog="$work_root/joblog-$stamp.tsv"
makefile="$work_root/fixture-$stamp.mk"
mkdir -p "$log_dir" "$status_dir"
printf 'target\tplatform\tlang\trmw\tstatus\tstart_epoch\tend_epoch\tduration_s\tlog\n' >"$joblog"

leaf_file="$work_root/leaves-$stamp.tsv"
mkdir -p "$work_root"
case "$scope" in
    linux|all)
        python3 - "$repo_root/examples/fixtures.toml" >"$leaf_file" <<'PY'
import sys
from pathlib import Path

try:
    import tomllib
except ModuleNotFoundError:
    import tomli as tomllib

manifest = Path(sys.argv[1])
with manifest.open("rb") as f:
    rows = tomllib.load(f).get("fixture", [])

seen = set()
for row in rows:
    if row.get("platform") != "linux":
        continue
    lang = row.get("lang") or "rust"
    rmw = row.get("rmw") or ""
    key = (lang, rmw)
    if key in seen:
        continue
    seen.add(key)
    name = f"linux-{lang}{('-' + rmw) if rmw else ''}".replace("_", "-")
    target = f"fixture-{name}"
    label = f"linux {lang}{(' ' + rmw) if rmw else ''}"
    command = (
        f"NROS_JOBSERVER=1 scripts/build/fixtures-build.sh linux {lang} {rmw}"
        if rmw
        else f"NROS_JOBSERVER=1 scripts/build/fixtures-build.sh linux {lang}"
    )
    rmw_field = rmw if rmw else "-"
    print(f"{target}\tlinux\t{lang}\t{rmw_field}\t{label}\t{name}\t{command}")
PY
        ;;
    linux-cyclonedds-rust)
        # issue 0488 residue 2 — this used to hand-roll
        #   cd examples/native/rust/$role && cargo build … --target-dir target-cyclonedds
        # which is a SECOND spelling of a build the manifest already describes:
        # `examples/fixtures.toml` carries a `linux`/`rust`/`cyclonedds` row for
        # each role, and `linux` is in `NROS_FIXTURE_SHARED_PLATFORMS`, so the
        # fixture lane builds those rows into `build/cargo-fixtures/<slug>`.
        #
        # The two spellings did not merely duplicate bytes, they DISAGREED: the
        # test resolver reads the manifest row (issue 0517), i.e. the group dir,
        # while this wrote a leaf `target-cyclonedds/` nothing read. Recreated on
        # every native sweep — the `native 2 (target-cyclonedds)` line phase-340
        # recorded as surviving wave 2.
        #
        # Routing through `fixtures-build.sh` is what the default leaf set (and
        # the `linux-cmake-rmw` leaf below) already does, so this stops being a
        # special case as well as a duplicate.
        {
            name="linux-rust-cyclonedds"
            printf '%s\tlinux\trust\tcyclonedds\t%s\t%s\t%s\n' \
                "fixture-$name" "linux rust cyclonedds" "$name" \
                "NROS_JOBSERVER=1 scripts/build/fixtures-build.sh linux rust cyclonedds"
        } >"$leaf_file"
        ;;
    linux-cmake-rmw)
        {
            for lang in c cpp; do
                for rmw_name in zenoh xrce; do
                    name="linux-$lang-$rmw_name"
                    target="fixture-$name"
                    label="linux $lang $rmw_name"
                    command="NROS_JOBSERVER=1 scripts/build/fixtures-build.sh linux $lang $rmw_name"
                    printf '%s\tlinux\t%s\t%s\t%s\t%s\t%s\n' "$target" "$lang" "$rmw_name" "$label" "$name" "$command"
                done
            done
        } >"$leaf_file"
        ;;
    linux-cyclonedds-cmake)
        # Issue 0022 — the cyclone deadlock (parallel corrosion→cargo on
        # nros-c/nros) is fixed at the SOURCE: nros-sizes-build now strips the
        # make jobserver from the nested opaque-size probe cargo
        # (packages/tooling/nros-sizes-build/src/lib.rs), so the recursive
        # hold-and-wait that hung the build can no longer form — on ANY platform,
        # while the outer build keeps jobserver coordination. No per-leaf
        # jobserver/CARGO_HOME hacks needed here.
        {
            for lang in c cpp; do
                name="linux-$lang-cyclonedds"
                target="fixture-$name"
                label="linux $lang cyclonedds"
                command="NROS_JOBSERVER=1 scripts/build/fixtures-build.sh linux $lang cyclonedds"
                printf '%s\tlinux\t%s\tcyclonedds\t%s\t%s\t%s\n' "$target" "$lang" "$label" "$name" "$command"
            done
        } >"$leaf_file"
        ;;
esac

if [ ! -s "$leaf_file" ]; then
    echo "fixture-make-driver: no fixture leaves found for scope $scope" >&2
    exit 1
fi

targets=()
leaf_details=()
{
    printf '# Generated by scripts/build/fixture-make-driver.sh at %s\n' "$stamp"
    printf 'SHELL := /bin/bash\n'
    printf '.SHELLFLAGS := -eu -o pipefail -c\n'
    printf '.DELETE_ON_ERROR:\n'
    printf '.PHONY: all'
    while IFS=$'\t' read -r target platform lang rmw label name command; do
        [ -n "$target" ] || continue
        if [ "$rmw" = "-" ]; then
            rmw_value=""
        else
            rmw_value="$rmw"
        fi
        targets+=("$target")
        leaf_details+=("  target=$target platform=$platform lang=$lang rmw=${rmw_value:-<none>} log=$log_dir/$name.log status=$status_dir/$name.status command=$command")
        printf ' %s' "$target"
    done <"$leaf_file"
    printf '\n\n'
    printf 'all:'
    for target in "${targets[@]}"; do
        printf ' %s' "$target"
    done
    printf '\n\n'

    while IFS=$'\t' read -r target platform lang rmw label name command; do
        [ -n "$target" ] || continue
        if [ "$rmw" = "-" ]; then
            rmw_value=""
        else
            rmw_value="$rmw"
        fi
        log="$log_dir/$name.log"
        status_file="$status_dir/$name.status"
        printf '%s:\n' "$target"
        printf '\t+@echo "fixture: %s"\n' "$label"
        printf '\t+@start=$$(date +%%s); status=0; echo "running" >%s; ( %s ) >%s 2>&1 || status=$$?; end=$$(date +%%s); duration=$$((end - start)); if [ "$$status" -eq 0 ]; then state=ok; else state=fail; fi; printf "target=%%s\\nplatform=%%s\\nlang=%%s\\nrmw=%%s\\nstatus=%%s\\nstart_epoch=%%s\\nend_epoch=%%s\\nduration_s=%%s\\nlog=%%s\\n" "%s" "%s" "%s" "%s" "$$state" "$$start" "$$end" "$$duration" "%s" >%s; printf "%%s\\t%%s\\t%%s\\t%%s\\t%%s\\t%%s\\t%%s\\t%%s\\t%%s\\n" "%s" "%s" "%s" "%s" "$$state" "$$start" "$$end" "$$duration" "%s" >>%s; if [ "$$status" -ne 0 ]; then echo "fixture-make-driver: %s failed; tail of %s:" >&2; tail -n "$${NROS_FIXTURE_FAIL_TAIL:-80}" %s >&2 || true; exit "$$status"; fi\n' "$status_file" "$command" "$log" "$target" "$platform" "$lang" "$rmw_value" "$log" "$status_file" "$target" "$platform" "$lang" "$rmw_value" "$log" "$joblog" "$target" "$log" "$log"
    done <"$leaf_file"
} >"$makefile"

rm -f "$leaf_file"

make_bin="make"
make_args=()
jobs="${NROS_BUILD_JOBS:-$(nproc 2>/dev/null || echo 8)}"
if [ -x "$(nros sdk-path make)/bin/make" ] && \
   "$(nros sdk-path make)/bin/make" --version | head -1 | grep -q "4.4" && \
   [ -x "$(nros sdk-path ninja)/bin/ninja" ]; then
    make_bin="$(nros sdk-path make)/bin/make"
    make_args=(-j"$jobs" --jobserver-style=fifo)
    export PATH="$(nros sdk-path make)/bin:$(nros sdk-path ninja)/bin:$PATH"
    echo "fixture-make-driver: using pinned fifo make"
else
    make_args=(-j"$jobs")
    echo "fixture-make-driver: pinned fifo tools not found; using ordinary make fallback"
fi

echo "fixture-make-driver: makefile=$makefile"
echo "fixture-make-driver: log-dir=$log_dir"
echo "fixture-make-driver: status-dir=$status_dir"
echo "fixture-make-driver: joblog=$joblog"
echo "fixture-make-driver: targets=${targets[*]}"
echo "fixture-make-driver: command=$make_bin ${make_args[*]} -f $makefile"

if [ "$dry_run" = "1" ]; then
    echo "fixture-make-driver: leaves:"
    for detail in "${leaf_details[@]}"; do
        echo "$detail"
    done
    echo "fixture-make-driver: dry run; not executing make"
    exit 0
fi

env -u MAKEFLAGS -u CARGO_MAKEFLAGS \
    NROS_BUILD_LOG_DIR="$log_dir" \
    "$make_bin" "${make_args[@]}" -f "$makefile"

echo "fixture-make-driver: joblog=$joblog"

if [ "$keep" != "1" ]; then
    rm -f "$makefile"
else
    echo "fixture-make-driver: kept makefile=$makefile"
fi
