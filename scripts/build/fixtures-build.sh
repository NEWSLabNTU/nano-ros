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

# issue 0522 — a BUILD LANE does not want the metadata probe's cmake cache.
# Measured on `examples/workspaces/c` (sidecars deleted before each run): the
# cache buys ~17 s per re-probe (6.0 s warm vs 23.2 s cold) and costs 4.8 GiB
# per workspace — 50.26 GiB across the 14 of them — and it is only consulted
# when a sidecar is MISSING, which no lane causes. Good deal for a developer
# iterating on a component's metadata, bad one here.
#
# The CLI keeps the tree by default and discards it only on FULL SUCCESS, so a
# failing probe still leaves its evidence behind for whoever reads the log.
export NROS_METADATA_PROBE_CACHE="${NROS_METADATA_PROBE_CACHE:-0}"

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

# phase-344 W2 follow-up — this driver builds the CARGO lanes, so it must select
# by BUILDER, not by `lang`. `--lang rust` was a proxy that held only while every
# rust row built with cargo; the six qemu-riscv64-threadx cyclonedds rows are
# rust leaves driven through cmake (just/threadx-riscv64.just), and routing them
# here made the driver export their cmake-only `build_subdir` as a shell
# variable name: `export: 'build-cyclonedds': not a valid identifier`.
#
# W2 made the MANIFEST predicate builder-aware and this caller kept the proxy —
# the same mistake one level down, which is why it only surfaced in a lane build
# and not in any gate.
_builder_args=()
case "$lang" in
    c | cpp) ;;                       # cmake lanes; leave selection to --lang
    *) _builder_args=(--builder cargo) ;;
esac

manifest() {
    python3 scripts/build/fixtures-manifest.py list \
        --platform "$platform" --lang "$lang" ${rmw:+--rmw "$rmw"} \
        "${_builder_args[@]}" \
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
        "${_builder_args[@]}" \
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
    if [ -x "$(nros sdk-path make)/bin/make" ] && \
       "$(nros sdk-path make)/bin/make" --version | head -1 | grep -q "4.4"; then
        make_bin="$(nros sdk-path make)/bin/make"
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
    # 0706 added two helpers that `nros_cmake_guard_build_dir` calls — the
    # toolchain-RESOLUTION probe and the build dir's recorded compiler — and a
    # make leaf is a fresh bash with only what `export -f` gave it, so they have
    # to ship too. Without them the leaf dies
    # "nros_cmake_toolchain_resolved_cc: command not found", which is where the
    # NuttX C rows failed the whole tier-2 fixture build.
    export -f nros_fixture_build_cmake nros_cmake_configure_if_needed \
        nros_cmake_guard_build_dir \
        nros_cmake_toolchain_resolved_cc nros_cmake_dir_cc
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
    # examples) share one `build/cargo-fixtures/<group>` so nano-ros
    # crates compile once for the group, not once per example dir. The
    # stale probe sources the SAME helper (rust-fixture-stale.sh).
    # shellcheck source=scripts/build/fixtures-target-dir.sh
    source scripts/build/fixtures-target-dir.sh
    export platform
    # `nros_build_{root,dir}` come along: the resolver calls them, and a leaf
    # never sources build-root.sh (phase-334 W2.b — the step-1 regression).
    # `nros_fixture_platform_is_shared` is in the list because
    # `nros_fixture_group` CALLS it (phase-340 B2 split the eligibility test out
    # of it so the batch driver could ask without forking). A leaf is a fresh
    # bash that has only what `export -f` gave it, so an un-exported callee is
    # an unbound command: `nros_fixture_group` would emit nothing, the row would
    # look ineligible, and the build would write the example-local `target/`
    # while the probe and the test resolver looked in the group dir. Caught by
    # `build_root_derivation.sh`'s make-leaf scenario, which reads THIS list.
    export -f nros_fixture_target_dir_flag nros_fixture_group nros_fixture_group_slug \
              nros_fixture_platform_is_shared \
              nros_fixture_strip_authored_target_dir _nros_fixture_variant_sig \
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
    # phase-351 W3 — the NuttX `libc` re-append (Phase 214.M.2,
    # `scripts/build/nuttx-libc-patch.sh`) is GONE. It existed because sync
    # WITHHELD every `${workspace}`-bearing key from the board projection, so
    # the row the board declares never reached a leaf. Sync now delivers it as
    # a `# nros-managed` `[patch.crates-io]` row, leaf-relative — one spelling,
    # in the file the leaf already had.
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
        # issue 0649 — the `nros sync` that used to live HERE is now a pre-pass
        # in the parent (`nros_presync_row_dirs`, below). It ran per ROW, and a
        # workspace has many rows: measured over one `lane=native` build, 185
        # invocations for 69 distinct targets, `examples/workspaces/features`
        # synced 22 times for its 24 manifest rows. Sync is per-WORKSPACE and
        # idempotent — its outputs (generated msg crates, the patch config,
        # resolved models) do not vary by the platform/rmw coordinate that
        # distinguishes one row from another — so the loop asked the same
        # question 22 times.
        # Phase 226.D — append the shared fixture-only --target-dir for
        # eligible rows (no-op for rows whose platform isn't migrated yet).
        #
        # phase-340 W2 — when the group governs, the row's OWN `--target-dir`
        # is stripped first: an authored dir now names a group instead of
        # opting the row out, and passing both would hand cargo two flags.
        # The strip must happen on the same side as the append, or the probe
        # (rust-fixture-stale.sh) and the build would differ by one flag.
        #
        # `if`, not `[ -n … ] && args=…`: the make leaves run under
        # `.SHELLFLAGS := -eu`, where an and-list whose left side is false is
        # itself a failing command and aborts the recipe.
        local tdir_flag
        tdir_flag="$(nros_fixture_target_dir_flag "$platform" "$args" "$envstr")"
        if [ -n "$tdir_flag" ]; then
            args="$(nros_fixture_strip_authored_target_dir "$args")"
        fi
        # phase-351 W5/W6 — deliver the board rung + site config from HERE, the
        # invoker, instead of from a leaf `[env]` row. Same values, one
        # mechanism across every lane; a leaf that resolves no board (a host
        # bin, a fixture with no deploy metadata) simply gets nothing, which is
        # why a failure here is not fatal.
        local facts=""
        if [ -x "$NROS_CLI" ]; then
            facts="$(NROS_REPO_DIR="$NROS_REPO_ROOT" "$NROS_CLI" ws board-facts "$dir" 2>/dev/null || true)"
        fi
        # shellcheck disable=SC2086
        # `if`, not an and-list: the comment above records that a false
        # and-list is itself a failing command under `.SHELLFLAGS := -eu`.
        ( cd "$dir"
          if [ -n "$envstr" ]; then export $envstr; fi
          if [ -n "$facts" ]; then export $facts; fi
          cargo build $cargo_profile_args $args $tdir_flag --quiet )
    }
    export -f nros_fixture_build_one

    # issue 0649 — sync each row DIRECTORY once, in the parent, before the rows
    # fan out.
    #
    # This is the same move as the Node-pkg pre-pass below and for the same
    # reason, applied to the rows themselves. The per-row sync it replaces was
    # not simulating a user procedure: a user runs `nros sync` once and then
    # builds, where the loop ran it once per manifest ROW — 22 times for
    # `examples/workspaces/features`, whose 24 rows share one directory. 185
    # invocations for 69 distinct targets in one `lane=native` build, 63 % of
    # them repeats.
    #
    # Safe to hoist on both axes that could have made it unsafe, and both were
    # checked rather than assumed:
    #
    #   * nothing row-specific reaches sync — the call passed only
    #     `NROS_REPO_DIR`, which is constant for the run, never the row's
    #     `envstr` or `args`;
    #   * concurrent same-directory syncs were already SAFE (8 parallel syncs of
    #     one workspace, warm and cold: 8/8 exit 0, byte-identical output), so
    #     this removes waste rather than a race. Hoisting makes it serial
    #     anyway, which is strictly better than relying on that.
    #
    # Dirs the Node-pkg pre-pass already handled are skipped, so the two passes
    # cannot re-sync the same tree.
    nros_presync_row_dirs() {
        local line dir seen=""
        for line in "${fixture_records[@]}"; do
            IFS=$'\x1f' read -r dir _ _ <<< "$line"
            [ -n "$dir" ] || continue
            [ -f "$dir/package.xml" ] || continue
            case " $seen " in *" $dir "*) continue ;; esac
            seen="$seen $dir"
            echo "  → (row codegen) $dir"
            # Phase 214.J.2 — wipe `<dir>/generated/` if the hash of the trait
            # surface(s) tied to codegen shape (e.g. `RosAction`) has drifted
            # since the previous `nros sync`. Prevents the stale-3-type-action
            # shape that Phase 214.J first surfaced.
            NROS_REPO_DIR="$NROS_REPO_ROOT" nros_codegen_stamp_check_or_wipe "$dir"
# phase-367 W5 — `--no-provider-index`: this driver never READS
# `<ws>/build/nros/providers.json`. cmake keeps its own index at
# `${CMAKE_BINARY_DIR}/nros-providers.json` and reads it THROUGH the CLI, and
# no caller points `nano_ros_load_providers(INDEX …)` at the sync-written one.
# Writing it costs the underlay scan — 28 %% of a warm sync after W1/W2
# (0.095 s -> 0.068 s), across ~101 syncs a build.
            NROS_REPO_DIR="$NROS_REPO_ROOT" "$NROS_CLI" sync --no-provider-index "$dir" >/dev/null
            NROS_REPO_DIR="$NROS_REPO_ROOT" nros_codegen_stamp_write "$dir"
        done
    }

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
    nros_presync_row_dirs
    run nros_fixture_build_one
fi
