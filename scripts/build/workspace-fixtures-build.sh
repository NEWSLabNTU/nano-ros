#!/usr/bin/env bash
# Build workspace fixtures from examples/fixtures.toml.
#
# Workspace fixtures differ from single-node fixtures: each row is a complete
# workspace with Node packages, a Bringup package, and an Entry package. The
# build must follow the documented user workflow:
#
#   nros ws sync
#   nros codegen-system --bringup <bringup> --out <codegen_out>
#   cargo build -p <entry> ... OR cmake --build ... --target <entry>
#
# Usage, from anywhere in the repo checkout:
#   scripts/build/workspace-fixtures-build.sh <platform> [lang]
set -euo pipefail

platform="${1:?usage: workspace-fixtures-build.sh <platform> [lang]}"
lang_filter="${2:-}"

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$script_dir/../.." && pwd)"

# shellcheck source=scripts/build/cargo.sh
source "$repo_root/scripts/build/cargo.sh"
# shellcheck source=scripts/build/cmake-incremental.sh
source "$repo_root/scripts/build/cmake-incremental.sh"
# shellcheck source=scripts/build/nuttx-libc-patch.sh
source "$repo_root/scripts/build/nuttx-libc-patch.sh"

cd "$repo_root"

nros_cli="$(nros_cli_bin)"
nros_require_ws_sync "$nros_cli"

export NROS_REPO_DIR="${NROS_REPO_DIR:-$repo_root}"
export NROS_REPO_ROOT="${NROS_REPO_ROOT:-$repo_root}"
export NROS_CLI="$nros_cli"
export NROS_CLI_BIN="$nros_cli"

if [ "${NROS_JOBSERVER:-}" = "1" ]; then
    unset CMAKE_BUILD_PARALLEL_LEVEL
else
    export CMAKE_BUILD_PARALLEL_LEVEL="${CMAKE_BUILD_PARALLEL_LEVEL:-${NROS_BUILD_JOBS:-8}}"
fi

manifest() {
    python3 "$repo_root/scripts/build/fixtures-manifest.py" list-workspaces \
        --platform "$platform" ${lang_filter:+--lang "$lang_filter"}
}

profile_dir="$(nros_cargo_target_profile_dir)"
mapfile -t cargo_profile_args < <(nros_cargo_profile_args)

build_workspace() {
    local record="$1"
    local id lang dir bringup entry build_subdir target_dir codegen_out defs envstr cargo_extra board conf_files
    IFS=$'\x1f' read -r id lang dir bringup entry build_subdir target_dir codegen_out defs envstr cargo_extra board conf_files <<< "$record"

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
    # CycloneDDS from `third-party/cyclonedds`. When that submodule is absent the
    # build otherwise fails DEEP and cryptically — e.g. the bridge's
    # `nros::main!(launch=...)` finds no `nros sync`-generated `nros-bridge.toml`,
    # falls back to a normal-launch entry, and errors `E0433: cannot find
    # nros_board_native`. Fail LOUD + actionable here instead. Scoped to native:
    # the embedded cyclonedds lanes (freertos/threadx/zephyr) have their own
    # graceful idlc/submodule skips and must not be turned into hard failures.
    case "$defs" in
        *NROS_RMW=cyclonedds*)
            if [ "$platform" = "native" ] && \
               [ ! -e "$repo_root/third-party/cyclonedds/CMakeLists.txt" ]; then
                echo "ERROR: workspace fixture '$id' requires the cyclonedds submodule," >&2
                echo "       which is not checked out (third-party/cyclonedds is empty)." >&2
                echo "       This fixture vendors C++ CycloneDDS by design and cannot build" >&2
                echo "       without it. Run:" >&2
                echo "         git submodule update --init --recursive third-party/cyclonedds" >&2
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

        echo "     nros ws sync"
        "$nros_cli" sync >/dev/null

        # Phase 225.O — `nros ws sync` strips the board-template
        # `[patch.crates-io] libc` from the rendered `.cargo/config.toml`,
        # so re-append the patched NuttX libc here, mirroring the
        # single-node lane in scripts/build/fixtures-build.sh. Idempotent
        # and a no-op for non-NuttX workspace rows.
        NROS_REPO_DIR="$repo_root" nros_nuttx_libc_patch "$repo_root/$dir"

        # BAKE path only: a pure-cargo `nros::main!` entry (no codegen_out)
        # bakes the system in the proc-macro, so it skips codegen-system.
        if [ -n "$codegen_out" ]; then
            echo "     nros codegen-system --bringup $bringup --out $codegen_out"
            "$nros_cli" codegen-system --bringup "$bringup" --out "$codegen_out" >/dev/null
        fi

        if [ "$lang" = "rust" ]; then
            local profile_args=("${cargo_profile_args[@]}")
            local row_profile_dir="$profile_dir"
            # The NuttX standalone flat image miscompiles at the
            # `nros-fast-release` opt-level (it boots to `main` but the
            # runtime never functions — no transport, zero output;
            # `release` opt-level 3 works). Build the NuttX workspace
            # Entry with `--release` until that profile issue is root-caused.
            if [ "$platform" = "nuttx" ]; then
                profile_args=(--release)
                row_profile_dir="release"
            fi
            local cargo_args=(build "${profile_args[@]}" -p "$entry")
            if [ -n "$target_dir" ]; then
                cargo_args+=(--target-dir "$target_dir")
            fi
            if [ -n "$cargo_extra" ]; then
                local extra_args=()
                read -r -a extra_args <<< "$cargo_extra"
                cargo_args+=("${extra_args[@]}")
            fi
            echo "     cargo ${cargo_args[*]}"
            if [ -n "$envstr" ]; then
                export $envstr
            fi
            cargo "${cargo_args[@]}"

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
            if [ "$platform" = "nuttx" ]; then
                build_target="${entry}_build"
            fi
            echo "     cmake --build $build_subdir --target $build_target"
            cmake --build "$build_subdir" --target "$build_target"
            local built_path
            built_path="$(find "$build_subdir" -type f -name "$entry" -perm -111 | sort | head -n 1 || true)"
            if [ -n "$built_path" ]; then
                echo "     built: $dir/$built_path"
            else
                echo "     built target: $entry under $dir/$build_subdir"
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

found=0
while IFS= read -r record; do
    found=1
    build_workspace "$record"
done < <(manifest)

if [ "$found" = "0" ]; then
    echo "No workspace fixtures matched platform=$platform${lang_filter:+ lang=$lang_filter}."
fi
