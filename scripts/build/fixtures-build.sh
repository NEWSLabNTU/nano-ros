#!/usr/bin/env bash
# Build all <platform> [<lang>] fixtures from the SSOT manifest
# (examples/fixtures.toml). Phase 181.
#
# Per-fixture options come from the manifest; per-PLATFORM env is the caller's
# responsibility (already exported):
#   rust  — toolchain/+nightly/SDK dirs; codegen (`nros generate-rust`) run by
#           the recipe before this. Manifest record: <dir>\x1f<env>\x1f<cargo-args>.
#   c/cpp — toolchain/SDK cache vars + the codegen tool / idlc paths via
#           $NROS_CMAKE_EXTRA_DEFS (appended to every cmake configure; C/C++
#           message codegen runs inside cmake). Record:
#           <dir>\x1f<build-subdir>\x1f<cmake -D defs>\x1f<target>.
#
# Usage (from repo root):
#   scripts/build/fixtures-build.sh <platform> [<lang>] [<rmw>] [--id <id>]
#     lang default: rust. rmw (optional) restricts to one RMW — recipes use it
#     to gate optional backends (e.g. cyclonedds only when set up). --id
#     restricts to one manifest row with a stable id.
#
# Honors NROS_JOBSERVER=1 (serial; tools inherit fifo tokens) and otherwise
# runs manifest rows through a temporary makefile. No GNU parallel dependency.
#
# Honors NROS_FIXTURE_COORDS (issue 0393): a `lane-coords` file restricting the
# build to one CI lane's `platform,lang,rmw` coordinates. Unset = build every
# matching row.
set -euo pipefail

usage="usage: fixtures-build.sh <platform> [lang] [rmw] [--id <id>] [--core-only]"
if [ "${1:-}" = "-h" ] || [ "${1:-}" = "--help" ]; then
    echo "$usage"
    exit 0
fi
platform="${1:?$usage}"
shift

lang="rust"
rmw=""
fixture_id=""
core_only=""

if [ $# -gt 0 ] && [ "$1" != "--id" ] && [[ "$1" != --id=* ]]; then
    lang="$1"
    shift
fi

while [ $# -gt 0 ]; do
    case "$1" in
        --id)
            [ $# -ge 2 ] || {
                echo "$usage" >&2
                exit 2
            }
            fixture_id="$2"
            shift 2
            ;;
        --id=*)
            fixture_id="${1#--id=}"
            shift
            ;;
        --core-only)
            # Issue #29 — restrict to default-config rows (no isolated
            # target_dir); skips the RMW/feature variant rebuilds that
            # duplicate the dep graph + overrun the host-integration disk.
            core_only="1"
            shift
            ;;
        --*)
            echo "fixtures-build.sh: unknown option: $1" >&2
            echo "$usage" >&2
            exit 2
            ;;
        *)
            if [ -z "$rmw" ]; then
                rmw="$1"
                shift
            else
                echo "fixtures-build.sh: unexpected argument: $1" >&2
                echo "$usage" >&2
                exit 2
            fi
            ;;
    esac
done

# Issue 0406 — reject a typo'd platform before sweeping zero rows successfully.
# shellcheck source=scripts/build/fixture-id-guard.sh
source scripts/build/fixture-id-guard.sh
nros_fixture_require_known_platform "$platform"

# shellcheck source=/dev/null
source scripts/build/cargo.sh

# Issue 0393 — `NROS_FIXTURE_COORDS` narrows the build to one CI lane's
# coordinates, the SAME file `check-fixtures-stale.sh` gates on and
# `fixtures-manifest.py --coords-from` already knew how to filter by. Read from
# the env rather than an argument because every per-platform `build-fixtures`
# recipe calls this script with its own positional args; an env var is one edit
# here instead of ten in just/*.just, and it matches how the gate consumes it.
# Unset (the default) = no filtering, i.e. pre-0393 behavior.
coords_args=()
if [ -n "${NROS_FIXTURE_COORDS:-}" ]; then
    if [ ! -s "${NROS_FIXTURE_COORDS}" ]; then
        echo "fixtures-build.sh: NROS_FIXTURE_COORDS=${NROS_FIXTURE_COORDS} is empty or absent" >&2
        echo "                   (a silent fallthrough would build everything and look like a lane)" >&2
        exit 2
    fi
    coords_args=(--coords-from "$NROS_FIXTURE_COORDS")
fi

manifest() {
    python3 scripts/build/fixtures-manifest.py list \
        --platform "$platform" --lang "$lang" ${rmw:+--rmw "$rmw"} \
        ${fixture_id:+--id "$fixture_id"} ${core_only:+--core-only} \
        "${coords_args[@]}"
}

# issue 0439 — the SAME query with the lane filter removed. Used only to tell
# "this id does not exist" from "this id exists but is not in this lane", which
# the narrowed query cannot distinguish and which #0406's guard would otherwise
# report as the caller's mistake. Deliberately a sibling of `manifest()` rather
# than a parameter on it: the two differ in exactly one argument, and a reader
# comparing them should see that at a glance.
manifest_unnarrowed() {
    python3 scripts/build/fixtures-manifest.py list \
        --platform "$platform" --lang "$lang" ${rmw:+--rmw "$rmw"} \
        ${fixture_id:+--id "$fixture_id"} ${core_only:+--core-only}
}

run_with_make() {
    local fn="$1"
    local work_root makefile target quoted make_quoted line idx jobs make_bin
    work_root="${NROS_BUILD_LOG_DIR:-build}/fixtures-build-make"
    mkdir -p "$work_root"
    makefile="$work_root/${platform}-${lang}${rmw:+-$rmw}-${fixture_id:-all}-$$-$RANDOM.mk"

    {
        printf '# Generated by scripts/build/fixtures-build.sh\n'
        printf 'SHELL := /bin/bash\n'
        printf '.SHELLFLAGS := -eu -o pipefail -c\n'
        printf '.DELETE_ON_ERROR:\n'
        printf '.PHONY: all'
        idx=0
        for line in "${fixture_records[@]}"; do
            printf ' fixture-%04d' "$idx"
            idx=$((idx + 1))
        done
        printf '\n\nall:'
        idx=0
        for line in "${fixture_records[@]}"; do
            printf ' fixture-%04d' "$idx"
            idx=$((idx + 1))
        done
        printf '\n\n'

        idx=0
        for line in "${fixture_records[@]}"; do
            printf -v target 'fixture-%04d' "$idx"
            printf -v quoted '%q' "$line"
            make_quoted="${quoted//\$/\$\$}"
            printf '%s:\n' "$target"
            printf '\t+@%s %s\n\n' "$fn" "$make_quoted"
            idx=$((idx + 1))
        done
    } >"$makefile"

    jobs="$(nros_cargo_frontend_jobs)"
    make_bin="make"
    if [ -x "$PWD/third-party/make/make" ] && \
       "$PWD/third-party/make/make" --version | head -1 | grep -q "4.4"; then
        make_bin="$PWD/third-party/make/make"
    fi

    "$make_bin" -j"$jobs" -f "$makefile"
    rm -f "$makefile"
}

run() {
    local fn="$1"
    local line
    if [ "${NROS_JOBSERVER:-}" = "1" ] || [ "${#fixture_records[@]}" -le 1 ]; then
        for line in "${fixture_records[@]}"; do "$fn" "$line"; done
    else
        run_with_make "$fn"
    fi
}

mapfile -t fixture_records < <(manifest)
if [ "${#fixture_records[@]}" -eq 0 ]; then
    # Issue 0406 — an explicit `--id` that matched nothing is a wrong
    # invocation, not an empty sweep coordinate. Diagnose it instead of
    # exiting 0 with no output.
    if [ -n "$fixture_id" ]; then
        # shellcheck source=scripts/build/fixture-id-guard.sh
        source scripts/build/fixture-id-guard.sh
        # Issue 0439 — FIRST rule out the one way an empty result is not the
        # caller's fault. `--coords-from` (issue 0393) drops rows for a lane
        # reason the recipe has no say in: `just/threadx-riscv64.just` hard-codes
        # `--id threadx-riscv64-logging-smoke` (a `rust` row) and tier 2's
        # coordinate for that platform is `c,cyclonedds`, so the row vanishes and
        # 0406's guard blamed a perfectly correct invocation — with a diagnostic
        # that printed requested and declared coordinates that MATCHED, because
        # the lane appears in neither.
        if nros_fixture_id_out_of_lane "$fixture_id" \
            "$([ "${#coords_args[@]}" -gt 0 ] && echo 1 || echo 0)" \
            "$(manifest_unnarrowed)" "${platform}/${lang}"; then
            exit 0
        fi
        nros_fixture_id_no_match "$fixture_id" flag fixture "$platform" "$lang" "$rmw"
    fi
    # No id filter: an empty (platform, lang, rmw) is routine — the recipes
    # iterate all four languages and not every platform has rows for each.
    exit 0
fi

# This builder narrows with `--id`, NOT with NROS_FIXTURE_ID (which the
# workspace and compile-check builders read). Say so rather than ignoring it
# in silence: a run narrowed by the env var still builds every row here, and
# the surprise otherwise lands as an unexplained rebuild.
if [ -n "${NROS_FIXTURE_ID:-}" ] && [ -z "$fixture_id" ]; then
    echo "fixtures: NROS_FIXTURE_ID='${NROS_FIXTURE_ID}' does not narrow single-node fixtures" \
         "(this stage takes --id); building all ${#fixture_records[@]} ${platform}/${lang} row(s)."
fi

if [ "$lang" = "c" ] || [ "$lang" = "cpp" ]; then
    # cmake cells — configure once (Ninja via the helper) + cmake --build.
    # shellcheck source=/dev/null
    source scripts/build/cmake-incremental.sh
    export NROS_CMAKE_EXTRA_DEFS="${NROS_CMAKE_EXTRA_DEFS:-}"
    nros_fixture_build_cmake() {
        local dir sub defs target
        IFS=$'\x1f' read -r dir sub defs target <<< "$1"
        [ -n "$dir" ] && [ -n "$sub" ] || return 0
        echo "  → $dir/$sub ${defs}${target:+ (target $target)}"
        # shellcheck disable=SC2086
        nros_cmake_configure_if_needed "$dir" "$dir/$sub" $defs $NROS_CMAKE_EXTRA_DEFS
        local ba=(--build "$dir/$sub")
        [ -n "$target" ] && ba+=(--target "$target")
        cmake "${ba[@]}"
    }
    # 0400's cache guard is CALLED BY nros_cmake_configure_if_needed; the
    # exported-function fan-out must carry it (and its helpers) too, or the
    # make workers die "nros_cmake_guard_build_dir: command not found".
    export -f nros_fixture_build_cmake nros_cmake_configure_if_needed \
        nros_cmake_guard_build_dir
    run nros_fixture_build_cmake
else
    # rust cells — cargo build with the manifest's exact features/target-dir/env.
    cargo_profile_args="$(nros_cargo_profile_arg_string)"
    export cargo_profile_args
    # Codegen here (centralized) rather than per-recipe: examples that path-dep
    # generated message crates (std_msgs/builtin_interfaces/example_interfaces via
    # [patch.crates-io] -> generated/ in .cargo/config.toml) need `nros generate-
    # rust` before cargo can resolve the patch path, and the gitignored generated/
    # dirs don't exist on a fresh checkout. Only examples have a package.xml (bins
    # skip). --force is idempotent so recipes that also codegen (freertos) are fine.
    NROS_CLI="$(nros_cli_bin)"; export NROS_CLI
    NROS_REPO_ROOT="${NROS_REPO_ROOT:-$PWD}"; export NROS_REPO_ROOT
    # Phase 226.D — shared fixture-only --target-dir resolver. Eligible
    # default-config rows for a migrated platform (qemu-arm-baremetal,
    # examples) share one `build/fixtures-cargo/<group>` so nano-ros
    # crates compile once for the group, not once per example dir. The
    # stale probe sources the SAME helper (rust-fixture-stale.sh).
    # shellcheck source=scripts/build/fixtures-target-dir.sh
    source scripts/build/fixtures-target-dir.sh
    export platform
    # `nros_build_{root,dir}` come along: the resolver calls them, and a leaf
    # never sources build-root.sh (phase-334 W2.b — the step-1 regression).
    export -f nros_fixture_target_dir_flag nros_fixture_group _nros_fixture_variant_sig \
              nros_build_root nros_build_dir
    # Phase 214.I.2 — fail-loud prereq guard: `nros_fixture_build_one`
    # below invokes `nros sync`, absent from the shipped 0.3.7 release.
    # Probe once here in the parent before make fans out workers; pre-probe
    # avoids N copies of the `[PREREQ]` line.
    nros_require_sync "$NROS_CLI"
    # shellcheck source=scripts/build/codegen-stamp.sh
    source scripts/build/codegen-stamp.sh
    export -f nros_codegen_stamp_compute nros_codegen_stamp_check_or_wipe \
              nros_codegen_stamp_write _codegen_stamp_repo_root _codegen_stamp_sources
    # Phase 214.M.2 — re-append the patched libc to NuttX fixtures'
    # `.cargo/config.toml` after `nros sync` runs. Workaround until
    # the CLI bug that drops `[patch.crates-io]` from the rendered
    # `cargo_config` template is fixed upstream. No-op for non-NuttX
    # fixtures. See `scripts/build/nuttx-libc-patch.sh`.
    # shellcheck source=scripts/build/nuttx-libc-patch.sh
    source scripts/build/nuttx-libc-patch.sh
    export -f nros_nuttx_libc_patch
    nros_fixture_build_one() {
        local dir envstr args
        IFS=$'\x1f' read -r dir envstr args <<< "$1"
        [ -n "$dir" ] || return 0
        echo "  → $dir ${args}"
        # Phase 210.E.3.d.native — pre-cargo `nros sync` writes the
        # auto-managed [patch.crates-io] block + materialises generated
        # msg crates under <dir>/build/. Replaces the legacy
        # `nros generate-rust --force` + per-example .cargo/config.toml
        # [patch.crates-io] chunks. Native rust adopters only — embedded
        # rust fixtures still pre-codegen separately until E.3.d.embedded.
        if [ -f "$dir/package.xml" ]; then
            # Phase 214.J.2 — wipe `<dir>/generated/` if the hash of the
            # trait surface(s) tied to codegen shape (e.g. `RosAction`)
            # has drifted since the previous `nros sync`. Prevents the
            # stale-3-type-action shape that Phase 214.J first surfaced.
            NROS_REPO_DIR="$NROS_REPO_ROOT" nros_codegen_stamp_check_or_wipe "$dir"
            NROS_REPO_DIR="$NROS_REPO_ROOT" "$NROS_CLI" sync "$dir" >/dev/null
            NROS_REPO_DIR="$NROS_REPO_ROOT" nros_codegen_stamp_write "$dir"
            # Phase 214.M.2 — fix up the rendered `.cargo/config.toml`
            # for NuttX fixtures (no-op for other platforms).
            NROS_REPO_DIR="$NROS_REPO_ROOT" nros_nuttx_libc_patch "$dir"
        fi
        # Phase 226.D — append the shared fixture-only --target-dir for
        # eligible rows (no-op for rows that authored their own
        # target_dir or whose platform isn't migrated yet).
        local tdir_flag
        tdir_flag="$(nros_fixture_target_dir_flag "$platform" "$args" "$envstr")"
        # shellcheck disable=SC2086
        ( cd "$dir"; [ -n "$envstr" ] && export $envstr; cargo build $cargo_profile_args $args $tdir_flag --quiet )
    }
    export -f nros_fixture_build_one
    # Phase 244.D1/C5 — Node+Entry split codegen pre-pass. The manifest builds
    # the Entry pkg (`nros::main!()`, NO package.xml), whose [patch.crates-io]
    # resolves the generated msg crates from a sibling Node pkg's gitignored
    # `generated/`. The Node pkg (has package.xml + `[package.metadata.nros.node]`)
    # is NOT a manifest build row, so the per-row codegen in
    # `nros_fixture_build_one` (gated on `$dir/package.xml`) never reaches it —
    # leaving `generated/` absent on a fresh checkout and the Entry build failing
    # to resolve the patch path. Pre-sync every Node pkg in this platform's
    # example tree, once, in the parent before the build fans out. Idempotent;
    # nros sync only materialises crates (no compile), so syncing a Node pkg whose
    # Entry is filtered out (--id / --core-only / rmw) is harmless.
    while IFS= read -r pkgxml; do
        pkgdir="$(dirname "$pkgxml")"
        [ -f "$pkgdir/Cargo.toml" ] || continue
        grep -q '^\[package\.metadata\.nros\.node\]' "$pkgdir/Cargo.toml" || continue
        echo "  → (node-pkg codegen) $pkgdir"
        NROS_REPO_DIR="$NROS_REPO_ROOT" nros_codegen_stamp_check_or_wipe "$pkgdir"
        NROS_REPO_DIR="$NROS_REPO_ROOT" "$NROS_CLI" sync "$pkgdir" >/dev/null
        NROS_REPO_DIR="$NROS_REPO_ROOT" nros_codegen_stamp_write "$pkgdir"
    # phase-300 W1.2 — tracked package.xml only, via the git index (the
    # unpruned find descended every build tree AND could pick up a
    # package.xml staged inside build-*/ -> spurious nros sync).
    done < <(git ls-files "examples/$platform/**/package.xml" "examples/$platform/package.xml")
    run nros_fixture_build_one
fi
