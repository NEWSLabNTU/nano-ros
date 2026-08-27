#!/usr/bin/env bash
# Build workspace fixtures from examples/fixtures.toml.
#
# Workspace fixtures differ from single-node fixtures: each row is a complete
# workspace with Node packages, a Bringup package, and an Entry package. The
# build must follow the documented user workflow:
#
#   nros sync
#   nros codegen-system --bringup <bringup> --out <codegen_out>
#   cargo build -p <entry> ... OR cmake --build ... --target <entry>
#
# Usage, from anywhere in the repo checkout:
#   scripts/build/workspace-fixtures-build.sh <platform> [lang] [--id <id>]
#
# Two ways to narrow to ONE manifest row, and they do NOT mean the same thing
# (issue 0406):
#
#   NROS_FIXTURE_ID=<id>   a sweep-wide narrowing. It crosses builders — the
#                          compile-check lane reads it too, and a platform
#                          recipe runs several stages — so a stage that matches
#                          nothing says so and passes.
#   --id <id>              this invocation targets THIS builder. Nothing else
#                          will run, so matching nothing is an error.
#
# Either way, an id that exists in no table at all is fatal: no stage in any
# sweep could ever build it.
set -euo pipefail

platform="${1:?usage: workspace-fixtures-build.sh <platform> [lang] [--id <id>]}"
shift
lang_filter=""
if [ $# -gt 0 ] && [[ "$1" != --* ]]; then
    lang_filter="$1"
    shift
fi

id_filter="${NROS_FIXTURE_ID:-}"
id_filter_source="env"
while [ $# -gt 0 ]; do
    case "$1" in
        --id)
            [ $# -ge 2 ] || {
                echo "workspace-fixtures-build.sh: --id needs a value" >&2
                exit 2
            }
            id_filter="$2"
            id_filter_source="flag"
            shift 2
            ;;
        --id=*)
            id_filter="${1#--id=}"
            id_filter_source="flag"
            shift
            ;;
        *)
            echo "workspace-fixtures-build.sh: unknown option: $1" >&2
            echo "usage: workspace-fixtures-build.sh <platform> [lang] [--id <id>]" >&2
            exit 2
            ;;
    esac
done

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$script_dir/../.." && pwd)"

# shellcheck source=scripts/build/cargo.sh
# shellcheck source=scripts/build/build-root.sh
source "$repo_root/scripts/build/build-root.sh"
source "$repo_root/scripts/build/cargo.sh"
# shellcheck source=scripts/build/cmake-incremental.sh
source "$repo_root/scripts/build/cmake-incremental.sh"

cd "$repo_root"

# Issue 0406 — reject a typo'd platform before the lane guards below let it
# through to a zero-row "success".
# shellcheck source=scripts/build/fixture-id-guard.sh
source "$repo_root/scripts/build/fixture-id-guard.sh"
nros_fixture_require_known_platform "$platform"

# Lane-env guards (the 2026-07-23 "broken rust lanes" were missing-env
# invocations, not repo breakage): the platform just recipes wrap this
# script with required env — direct calls must fail LOUD, not deep-panic.
case "$platform" in
    freertos|freertos-posix)
        if [ -z "${NROS_PLATFORM_FREERTOS_SRC:-}" ] || [ -z "${NROS_PLATFORM_CFFI_INCLUDE:-}" ]; then
            echo "[ERROR] the freertos lane needs the just/sdk-env.just exports" >&2
            echo "        (NROS_PLATFORM_FREERTOS_SRC, NROS_PLATFORM_CFFI_INCLUDE, …)." >&2
            echo "        Run via the just recipe (e.g. \`just freertos build-fixtures\`)" >&2
            echo "        instead of invoking this script directly." >&2
            exit 2
        fi
        ;;
    nuttx|nuttx-riscv)
        if [ "${lang_filter:-}" = "rust" ] && [ -z "${NUTTX_DIR:-}" ]; then
            echo "[ERROR] NUTTX_DIR not set — the NuttX rust workspace entry links" >&2
            echo "        the kernel export libs (-lboard/-lopenamp). Run via" >&2
            echo "        \`just nuttx build-fixtures\` (exports NUTTX_DIR +" >&2
            echo "        NUTTX_APPS_DIR) instead of invoking this script directly." >&2
            exit 2
        fi
        ;;
    zephyr)
        if [ "${lang_filter:-}" = "rust" ]; then
            echo "[ERROR] the Zephyr rust workspace entries are west-built staticlibs" >&2
            echo "        (workspace-EXCLUDED from cargo; see examples/workspaces/rust/" >&2
            echo "        Cargo.toml). Build them via \`just zephyr build-fixtures\`" >&2
            echo "        (scripts/build/zephyr-fixture-leaves.sh), not this script." >&2
            exit 2
        fi
        ;;
esac

nros_cli="$(nros_cli_bin)"

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

nros_require_sync "$nros_cli"

export NROS_REPO_DIR="${NROS_REPO_DIR:-$repo_root}"
export NROS_REPO_ROOT="${NROS_REPO_ROOT:-$repo_root}"
export NROS_CLI="$nros_cli"
export NROS_CLI_BIN="$nros_cli"

if [ "${NROS_JOBSERVER:-}" = "1" ] || [ -n "${NROS_WS_RECORDS_FILE:-}" ]; then
    # Jobserver-fed paths (the phase-176 outer pool, or a group worker under
    # this script's own fifo fan-out below): leave CMAKE_BUILD_PARALLEL_LEVEL
    # unset so ninja runs bare and JOINS the token pool — an explicit -j would
    # opt it out and multiply the load by the group count.
    unset CMAKE_BUILD_PARALLEL_LEVEL
else
    export CMAKE_BUILD_PARALLEL_LEVEL="${CMAKE_BUILD_PARALLEL_LEVEL:-${NROS_BUILD_JOBS:-8}}"
fi

# Issue 0393 — same lane narrowing as scripts/build/fixtures-build.sh. Workspace
# fixtures are 79 of the manifest's 337 rows, so leaving them unfiltered would
# have made a "lane" build most of the expensive half anyway. `list-workspaces`
# already accepts `--coords-from` (check-fixtures-stale.sh:140 gates on it).
coords_args=()
if [ -n "${NROS_FIXTURE_COORDS:-}" ]; then
    if [ ! -s "${NROS_FIXTURE_COORDS}" ]; then
        echo "workspace-fixtures-build.sh: NROS_FIXTURE_COORDS=${NROS_FIXTURE_COORDS} is empty or absent" >&2
        exit 2
    fi
    coords_args=(--coords-from "$NROS_FIXTURE_COORDS")
fi

manifest() {
    python3 "$repo_root/scripts/build/fixtures-manifest.py" list-workspaces \
        --platform "$platform" ${lang_filter:+--lang "$lang_filter"} \
        ${id_filter:+--id "$id_filter"} "${coords_args[@]}"
}

# issue 0439 — the SAME query with the lane filter removed, used only to tell
# "this id does not exist" from "this id is not in this lane". Sibling of
# `manifest()` rather than a parameter on it: they differ in exactly one
# argument and a reader should see that at a glance.
manifest_unnarrowed() {
    python3 "$repo_root/scripts/build/fixtures-manifest.py" list-workspaces \
        --platform "$platform" ${lang_filter:+--lang "$lang_filter"} \
        ${id_filter:+--id "$id_filter"}
}

profile_dir="$(nros_cargo_target_profile_dir)"
mapfile -t cargo_profile_args < <(nros_cargo_profile_args)

build_workspace() {
    local record="$1"
    local id lang dir bringup entry build_subdir target_dir codegen_out defs envstr cargo_extra board conf_files image
    IFS=$'\x1f' read -r id lang dir bringup entry build_subdir target_dir codegen_out defs envstr cargo_extra board conf_files image <<< "$record"

    [ -n "$id" ] || return 0
    [ -n "$dir" ] && [ -n "$bringup" ] && [ -n "$entry" ] || {
        echo "workspace fixture '$id' is missing dir/bringup/entry" >&2
        return 2
    }
    [ -d "$repo_root/$dir" ] || {
        echo "workspace fixture '$id' dir does not exist: $dir" >&2
        return 2
    }

    # Dependency gate (issue 0120): a cyclonedds workspace fixture vendors C++
    # CycloneDDS from `third-party/dds/cyclonedds`. When that submodule is absent the
    # build otherwise fails DEEP and cryptically — e.g. the bridge's
    # `nros::main!(launch=...)` finds no `nros sync`-generated `nros-bridge.toml`,
    # falls back to a normal-launch entry, and errors `E0433: cannot find
    # nros_board_linux`. Fail LOUD + actionable here instead. Scoped to the host:
    # the embedded cyclonedds lanes (freertos/threadx/zephyr) have their own
    # graceful idlc/submodule skips and must not be turned into hard failures.
    case "$defs" in
        *NROS_RMW=cyclonedds*)
            if [ "$platform" = "linux" ] && \
               [ ! -e "$repo_root/third-party/dds/cyclonedds/CMakeLists.txt" ]; then
                echo "ERROR: workspace fixture '$id' requires the cyclonedds submodule," >&2
                echo "       which is not checked out (third-party/dds/cyclonedds is empty)." >&2
                echo "       This fixture vendors C++ CycloneDDS by design and cannot build" >&2
                echo "       without it. Run:" >&2
                echo "         nros setup --source cyclonedds-src" >&2
                echo "       (or: git submodule update --init --recursive third-party/dds/cyclonedds)" >&2
                return 2
            fi
            ;;
    esac

    # `codegen_out` is required for the BAKE path (`nros codegen-system`). A
    # pure-cargo `nros::main!` entry bakes the system at proc-macro expansion
    # time, so it has no codegen_out — skip the codegen-system step for those.

    echo "  -> $id ($lang) $dir"
    (
        cd "$repo_root/$dir"
        if [ "$lang" != "rust" ] && [ -n "$build_subdir" ] && \
           [ -f "$build_subdir/CMakeCache.txt" ] && \
           [ ! -f "$build_subdir/build.ninja" ] && \
           [ ! -f "$build_subdir/Makefile" ]; then
            case "$build_subdir" in
                ""|"."|"/")
                    echo "refusing to clean unsafe CMake build dir: '$build_subdir'" >&2
                    return 2
                    ;;
            esac
            echo "     removing incomplete CMake build dir: $build_subdir"
            rm -rf "$build_subdir"
        fi
        [ -n "$codegen_out" ] && mkdir -p "$(dirname "$codegen_out")"

        # issue 0649 — the `nros sync` that used to be HERE is a pre-pass over
        # `group_dirs` now. It ran per ROW while sync is per-WORKSPACE, and the
        # large workspaces carry many rows: measured over one `lane=native`
        # build, `examples/workspaces/features` was synced 22 times for its 24
        # manifest rows.
        #
        # Safe to hoist because the row's own `env` cannot reach it: `export
        # $envstr` is BELOW, deliberately (the comment there records that env
        # reaching `codegen-system` was the fix for issue 0257 — sync was never
        # in that scope). Concurrent same-dir syncs were measured safe too, so
        # this removes waste rather than a race.

        # phase-351 W3 — the Phase 225.O re-append of the board's
        # `[patch.crates-io] libc` is GONE: sync no longer strips that row, it
        # DELIVERS it (`# nros-managed`, leaf-relative). Two lanes carried the
        # same workaround; both are retired together.

        # The manifest's `env = { … }` applies to the WHOLE row: codegen AND the
        # compile, in every language. It used to be exported only inside the
        # `lang = rust` branch, and only AFTER `codegen-system` — so a C/C++
        # row's `env` was silently ignored, and even a rust row's could not
        # reach codegen. `NROS_EXECUTOR_MAX_CBS` is read by codegen-system
        # (issue 0257), so the ignored env surfaced as a hard "table holds 4"
        # failure on a row that declares 8.
        if [ -n "$envstr" ]; then
            export $envstr
        fi

        # BAKE path only: a pure-cargo `nros::main!` entry (no codegen_out)
        # bakes the system in the proc-macro, so it skips codegen-system.
        if [ -n "$codegen_out" ]; then
            echo "     nros codegen-system --bringup $bringup --out $codegen_out"
            "$nros_cli" codegen-system --bringup "$bringup" --out "$codegen_out" >/dev/null
        fi

        if [ "$lang" = "rust" ]; then
            local profile_args=("${cargo_profile_args[@]}")
            local row_profile_dir="$profile_dir"
            # The NuttX standalone flat image miscompiles at any `lto = "off"`
            # profile (it boots to `main` but the runtime never functions — no
            # transport, zero output). Build the NuttX workspace Entry at the
            # carve-out profile until that is root-caused. phase-285 W5 —
            # nuttx-riscv rides the same dodge. The profile NAME comes from the
            # table (`nros_cargo_profile::NUTTX_RUST_PROFILE`) because the
            # test-side resolvers read the same constant; when the two were
            # separate literals they drifted (#156).
            if [ "$platform" = "nuttx" ] || [ "$platform" = "nuttx-riscv" ]; then
                local nuttx_profile
                nuttx_profile="$(nros_cargo_nuttx_profile)"
                # `nros_cargo_profile_args_for`, NOT the raw `_nros_profile_query`:
                # the table stores the flags as one string, so mapfiling the raw
                # query yields a single argv element `--profile nros-minsizerel`
                # and cargo rejects it. Every other path here goes through an
                # accessor that splits.
                mapfile -t profile_args < <(nros_cargo_profile_args_for "$nuttx_profile")
                row_profile_dir="$(_nros_profile_query dir "$nuttx_profile")"
            fi
            local extra_args=()
            if [ -n "$cargo_extra" ]; then
                read -r -a extra_args <<< "$cargo_extra"
            fi
            if [ -n "$image" ]; then
                # phase-383 W9.b — a MIGRATED row. The entry package is
                # generated, so the row names the image and `nros build` does
                # discovery, the generated root, the entry and the handoff. The
                # native args after `--` reach cargo verbatim, which is how the
                # fixture keeps its own profile and target-dir: the image
                # deliberately declares no `profile`, so there is exactly one
                # `--profile` on the command line.
                # QUALIFIED — see the note in the cargo branch below: an image
                # id can be declared by more than one bringup in a workspace.
                local qual_image="$(basename "$bringup"):$image"
                local nros_args=(build "$qual_image" --workspace . --offline --)
                nros_args+=("${profile_args[@]}")
                if [ -n "$target_dir" ]; then
                    nros_args+=(--target-dir "$target_dir")
                fi
                nros_args+=("${extra_args[@]}")
                echo "     nros ${nros_args[*]}"
                "$nros_cli" "${nros_args[@]}"
            else
                # A HAND-WRITTEN entry in a MIGRATED workspace still needs the
                # generated root to exist: `cargo build -p <entry>` resolves the
                # package through it, and the root is build output that a fresh
                # clone does not have —
                #
                #   error: package ID specification `esp32_entry` did not match
                #   any packages
                #
                # `--all --dry-run` writes the root and every entry and runs no
                # build tool, which is exactly the missing step. Only when the
                # workspace has no tracked root, so an unmigrated workspace is
                # untouched.
                if [ ! -f Cargo.toml ]; then
                    echo "     nros build --all --dry-run   (generate the root)"
                    "$nros_cli" build --all --dry-run >/dev/null
                fi
                local cargo_args=(build "${profile_args[@]}" -p "$entry")
                if [ -n "$target_dir" ]; then
                    cargo_args+=(--target-dir "$target_dir")
                fi
                cargo_args+=("${extra_args[@]}")
                echo "     cargo ${cargo_args[*]}"
                cargo "${cargo_args[@]}"
            fi

            local out_root="${target_dir:-target}"
            echo "     built: $dir/$out_root/$row_profile_dir/$entry"
        else
            [ -n "$build_subdir" ] || {
                echo "workspace fixture '$id' is missing build_subdir for CMake build" >&2
                return 2
            }
            local cmake_args=()
            if [ -n "$defs" ]; then
                read -r -a cmake_args <<< "$defs"
            fi
            cmake_args+=(
                "-DNROS_CLI_BIN=$nros_cli"
                "-D_NANO_ROS_CODEGEN_TOOL=$nros_cli"
            )

            if [ -n "$image" ]; then
                # phase-383 W10.a — a MIGRATED cmake row. `nros build` writes
                # the root (with the entry EMITTED into it, W4.b), configures
                # and builds. The row's `build_subdir` names where that lands,
                # `build/<coord>/cmake`, so the artifact locator below and the
                # test-side resolver keep reading one manifest fact.
                # The row's `cmake_defs` reach the generated configure as
                # native args. They carry facts no image declares and no board
                # knows — `NROS_ENTRY_LOCATOR = "tcp/10.0.2.2:8330"` is the QEMU
                # host address this fixture's peer listens on, which is a
                # property of the TEST, not of the program.
                # QUALIFIED `<bringup>:<image>`. A workspace may declare the
                # same image id in more than one bringup — `realtime-c` has
                # `demo_bringup:native` and `smp_bringup:native` — and an
                # unqualified name is then ambiguous and refused. The row
                # already names its bringup, so qualifying costs nothing and is
                # unambiguous everywhere, including single-bringup workspaces.
                local qual_image="$(basename "$bringup"):$image"
                local nros_args=(build "$qual_image" --workspace . --offline)
                if [ -n "$defs" ]; then
                    local def_args=()
                    read -r -a def_args <<< "$defs"
                    nros_args+=(-- "${def_args[@]}")
                fi
                echo "     nros ${nros_args[*]}"
                "$nros_cli" "${nros_args[@]}"
                if [ -x "$build_subdir/$entry" ]; then
                    echo "     built: $dir/$build_subdir/$entry"
                else
                    # A generated entry lands at the TOP of the cmake binary
                    # dir, not under `src/<entry>/` — it is emitted by the root
                    # rather than being a subdirectory package. Missing means
                    # the emit did not happen, which is silent otherwise: the
                    # node libraries still build and the lane still goes green
                    # over a fixture with no executable (W4.b was checked off in
                    # exactly that state).
                    echo "  !! $id: nros build produced no '$entry' in $dir/$build_subdir" >&2
                    return 2
                fi
            else

            echo "     cmake -S . -B $build_subdir ${cmake_args[*]}"
            nros_cmake_configure_if_needed . "$build_subdir" "${cmake_args[@]}"

            # phase-263 C2b — on NuttX the entry's real artifact is the NuttX KERNEL ELF
            # produced by the cargo `<entry>_build` custom target (cross arm-none-eabi-gcc link),
            # NOT the cmake `add_executable(<entry>)` — which `nros_board_link_app` marks
            # EXCLUDE_FROM_ALL but whose explicit `--target <entry>` link would still fire on the
            # HOST toolchain and fail (the generated C++ entry TU + component archives reference
            # the cargo-only nros_cpp_* / backend symbols). Target `<entry>_build` so only the
            # cargo path runs (steps that emit + copy the kernel ELF), mirroring the standalone
            # NuttX `all`-build which skips the EXCLUDE_FROM_ALL host exe.
            local build_target="$entry"
            if [ "$platform" = "nuttx" ] || [ "$platform" = "nuttx-riscv" ]; then
                build_target="${entry}_build"
            fi
            echo "     cmake --build $build_subdir --target $build_target"
            cmake --build "$build_subdir" --target "$build_target"
            # The entry executable lands at the canonical
            # `<build>/src/<entry>/<entry>` for every cmake-built workspace, so
            # ask for it directly instead of searching. The old unpruned
            # `find "$build_subdir"` walked the whole build tree once per
            # fixture row — 22.8k of 24.4k files in a cpp workspace are the
            # cargo target dir — and `sort | head -1` picked the
            # lexicographically first match, where `cargo/` sorts BEFORE `src/`:
            # a cargo artifact of the same name would have won over the cmake
            # entry. The fallback (NuttX copies its kernel ELF elsewhere) prunes
            # the three big generated subtrees.
            local built_path
            if [ -x "$build_subdir/src/$entry/$entry" ]; then
                built_path="$build_subdir/src/$entry/$entry"
            else
                built_path="$(find "$build_subdir" \
                    \( -name cargo -o -name _deps -o -name CMakeFiles \) -prune -o \
                    -type f -name "$entry" -perm -111 -print | sort | head -n 1 || true)"
            fi
            if [ -n "$built_path" ]; then
                echo "     built: $dir/$built_path"
            elif [ "$platform" = "nuttx" ] || [ "$platform" = "nuttx-riscv" ]; then
                # "No silent caps": on NuttX the real artifact is the kernel ELF that
                # `${entry}_build`'s cargo cross-link emits and its POST_BUILD step copies
                # to a `$entry`-named executable under $build_subdir (nros-nuttx.cmake).
                # That link can fail WITHOUT `cmake --build` returning non-zero (the failure
                # is not always propagated out of the custom-command/kernel-make layer), so an
                # empty find here means the link silently failed — fail loudly instead of
                # writing an inputsig stamp for a fixture with no bootable image.
                echo "  !! $id: NuttX build produced no '$entry' kernel ELF under $dir/$build_subdir" >&2
                echo "     — the cross-link failed silently (cmake --build returned 0 with no artifact)." >&2
                return 2
            else
                echo "     built target: $entry under $dir/$build_subdir"
            fi
            fi
        fi

        local stamp_dir
        if [ "$lang" = "rust" ]; then
            stamp_dir="${target_dir:-target}"
        else
            stamp_dir="$build_subdir"
        fi
        mkdir -p "$stamp_dir"
        bash "$repo_root/scripts/build/workspace-fixture-signature.sh" "$record" \
            > "$stamp_dir/.nros-workspace-fixture.$id.inputsig"
    )
}

# Group-worker re-entry (parallel fan-out below): when NROS_WS_RECORDS_FILE is
# set, this invocation IS one group worker — build the file's records serially
# (they share a workspace dir: one `nros sync`, one .cargo/config.toml, one
# cmake build tree — intra-group parallelism would race) and exit.
if [ -n "${NROS_WS_RECORDS_FILE:-}" ]; then
    while IFS= read -r record; do
        [ -n "$record" ] || continue
        build_workspace "$record"
    done < "$NROS_WS_RECORDS_FILE"
    exit 0
fi

mapfile -t all_records < <(manifest)
live_records=()
for record in "${all_records[@]}"; do
    [ -n "$record" ] && live_records+=("$record")
done

if [ "${#live_records[@]}" -eq 0 ]; then
    # Issue 0406 — this used to print one line and exit 0 for every reason at
    # once, including a typo'd id that nothing could ever build. The guard
    # keeps the benign sweep miss quiet-ish and fails the invocation errors.
    if [ -n "$id_filter" ]; then
        # shellcheck source=scripts/build/fixture-id-guard.sh
        source "$repo_root/scripts/build/fixture-id-guard.sh"
        # Issue 0439 — same reconciliation as `fixtures-build.sh`, via the same
        # helper. This builder has the identical shape (`--id` can set
        # `id_filter_source=flag`, and it honours `--coords-from`), so it carried
        # the identical latent bug; it simply had no lane-narrowed `--id` caller
        # yet. Fixed with the reported site rather than after it reappears here.
        if nros_fixture_id_out_of_lane "$id_filter" \
            "$([ "${#coords_args[@]}" -gt 0 ] && echo 1 || echo 0)" \
            "$(manifest_unnarrowed)" "${platform}${lang_filter:+/$lang_filter}"; then
            exit 0
        fi
        nros_fixture_id_no_match \
            "$id_filter" "${id_filter_source:-env}" workspace_fixture \
            "$platform" "$lang_filter"
        exit 0
    fi
    echo "No workspace fixtures matched platform=$platform${lang_filter:+ lang=$lang_filter}."
    exit 0
fi

# Distinct workspace dirs, order-preserving (field 3 of the \x1f record).
group_dirs=()
for record in "${live_records[@]}"; do
    IFS=$'\x1f' read -r _ _ dir _ <<< "$record"
    seen=0
    for g in "${group_dirs[@]:-}"; do [ "$g" = "$dir" ] && seen=1 && break; done
    [ "$seen" = "1" ] || group_dirs+=("$dir")
done

# issue 0649 — one `nros sync` per workspace DIRECTORY, before the rows fan out.
#
# `group_dirs` already exists for the shared cargo group, and it is exactly the
# right set: sync's outputs (generated msg crates, the patch config, resolved
# models) are per-workspace and do not vary by the row coordinate. Running it
# per row asked the same question up to 22 times for one directory.
#
# Serial and in the parent, mirroring the pre-pass in `fixtures-build.sh`: a
# user runs `nros sync` once and then builds, which is what this lane is
# supposed to be simulating.
for dir in "${group_dirs[@]:-}"; do
    [ -n "$dir" ] || continue
    [ -d "$repo_root/$dir" ] || continue
    echo "  -> nros sync $dir"
# phase-367 W5 — `--no-provider-index`: this driver never READS
# `<ws>/build/nros/providers.json`. cmake keeps its own index at
# `${CMAKE_BINARY_DIR}/nros-providers.json` and reads it THROUGH the CLI, and
# no caller points `nano_ros_load_providers(INDEX …)` at the sync-written one.
# Writing it costs the underlay scan — 28 %% of a warm sync after W1/W2
# (0.095 s -> 0.068 s), across ~101 syncs a build.
    ( cd "$repo_root/$dir" && "$nros_cli" sync --no-provider-index >/dev/null )
done

pinned_make="$repo_root/third-party/make/make"
use_pool=0
if [ "${NROS_JOBSERVER:-}" != "1" ] && [ "${#group_dirs[@]}" -gt 1 ] && \
   [ -x "$pinned_make" ] && "$pinned_make" --version | head -1 | grep -q "4.4"; then
    use_pool=1
fi

if [ "$use_pool" != "1" ]; then
    # Serial walk: under an inherited jobserver (NROS_JOBSERVER=1) the child
    # tools already share the outer token pool; without pinned make 4.4 the
    # exported CMAKE_BUILD_PARALLEL_LEVEL gives each build its width.
    for record in "${live_records[@]}"; do
        build_workspace "$record"
    done
    exit 0
fi

# Parallel fan-out — one make target per WORKSPACE DIR (rows within a dir stay
# serial inside their group worker), scheduled by pinned make 4.4's fifo
# jobserver with the full NROS_BUILD_JOBS budget as the token pool. The
# recipes run bare `cmake --build`/`cargo build` (CMAKE_BUILD_PARALLEL_LEVEL
# is unset for the pool path), so every ninja/cargo joins the pool and total
# concurrency stays at the budget no matter how many groups run at once.
pool_jobs="${NROS_BUILD_JOBS:-$(nproc 2>/dev/null || echo 8)}"
# phase-334 W2.b step 2 — the make-scratch root comes from the ONE derivation
# (RFC-0070 R3). NROS_BUILD_LOG_DIR still wins when set, because a caller that
# redirected its logs meant the scratch to follow them; what changes is that the
# FALLBACK is derived rather than a second spelling of "$repo_root/build".
work_root="${NROS_BUILD_LOG_DIR:-$(nros_build_root)}/workspace-fixtures-make"
mkdir -p "$work_root"
stamp="$(date +%Y%m%d-%H%M%S)-$$"
makefile="$work_root/ws-$platform-$stamp.mk"
group_targets=()
gi=0
for dir in "${group_dirs[@]}"; do
    group_file="$work_root/ws-$platform-$stamp-group-$gi.records"
    : > "$group_file"
    for record in "${live_records[@]}"; do
        IFS=$'\x1f' read -r _ _ rdir _ <<< "$record"
        [ "$rdir" = "$dir" ] && printf '%s\n' "$record" >> "$group_file"
    done
    group_targets+=("ws-group-$gi|$group_file|$dir")
    gi=$((gi + 1))
done

{
    printf '# Generated by workspace-fixtures-build.sh (%s groups)\n' "${#group_dirs[@]}"
    printf 'SHELL := /bin/bash\n'
    printf '.SHELLFLAGS := -eu -o pipefail -c\n'
    printf '.DELETE_ON_ERROR:\n'
    printf '.PHONY: all'
    for spec in "${group_targets[@]}"; do
        printf ' %s' "${spec%%|*}"
    done
    printf '\n\nall:'
    for spec in "${group_targets[@]}"; do
        printf ' %s' "${spec%%|*}"
    done
    printf '\n\n'
    for spec in "${group_targets[@]}"; do
        tgt="${spec%%|*}"
        rest="${spec#*|}"
        gfile="${rest%%|*}"
        gdir="${rest#*|}"
        printf '%s:\n' "$tgt"
        # Only pass filters that are actually SET. This used to interpolate
        # `%q %q` unconditionally, sending two EMPTY positional arguments on the
        # common path (no lang, no id). Harmless while the callee read
        # `lang_filter="${2:-}"`; fatal the moment the parser got strict
        # (cf3a362d6 / issue 0406): the empty string is not `--*`, so it was
        # consumed as the lang filter, and the second empty string fell through
        # to `*)` — "unknown option: " with nothing after the colon.
        # `just native build-workspace-fixtures` failed outright on main.
        #
        # A stricter parser meeting a caller that always passed placeholders is
        # the entire bug, and the fix belongs on the caller: it should never
        # have been sending arguments it did not have.
        printf '\t+@echo "== workspace group: %s =="; env -u CMAKE_BUILD_PARALLEL_LEVEL NROS_WS_RECORDS_FILE=%q bash %q %q' \
            "$gdir" "$gfile" "$0" "$platform"
        # Explicit `if`, not `[ … ] && printf`: under `set -e` a failing test
        # in an AND-list is a non-zero status that aborts the script, and the
        # common path is exactly the one where both tests fail.
        if [ -n "${lang_filter:-}" ]; then printf ' %q' "$lang_filter"; fi
        if [ -n "${id_filter:-}" ]; then printf ' --id %q' "$id_filter"; fi
        printf '\n\n'
    done
} > "$makefile"

echo "workspace-fixtures-build: ${#live_records[@]} row(s) in ${#group_dirs[@]} workspace group(s), pool=$pool_jobs (fifo)"
# issue 0762 — THIS is the make that outlived its launcher on 2026-08-23,
# still building for ten minutes after the top-level `just` was killed. Under
# the guard it shares the outermost launcher's process group (passthrough), and
# when invoked directly it becomes that group itself.
source "$repo_root/scripts/build/subtree-guard.sh"
nros_guard_exec workspace-fixtures \
    env -u MAKEFLAGS -u CARGO_MAKEFLAGS "$pinned_make" -j"$pool_jobs" --jobserver-style=fifo -f "$makefile"
rm -f "$makefile" "$work_root/ws-$platform-$stamp"-group-*.records
