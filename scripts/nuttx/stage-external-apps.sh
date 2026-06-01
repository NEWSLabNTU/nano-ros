#!/usr/bin/env bash
# Phase 157.C — stage nano-ros integration shell + example external
# apps into a NuttX apps tree so `make` from the configured NuttX
# kernel picks them up via the canonical apps/external/*/Make.defs
# + apps/external/*/Kconfig discovery.
#
# Idempotent. Re-running replaces existing symlinks; never modifies
# files outside $NUTTX_APPS_DIR/external/.
#
# Usage:
#   stage-external-apps.sh <nuttx-apps-dir> [nros-codegen-binary]
#                          [--bringup <bringup-pkg-dir>] [--copy]
#
# Required: <nuttx-apps-dir> must exist and look like a NuttX apps
# tree (has a `Make.defs` at the top level).
#
# Phase 212.H.2 — `--bringup <dir>` additionally stages a Phase 212
# `<system>_bringup` package at `apps/external/<basename(dir)>/` from
# the per-bringup template at `integrations/nuttx/apps-external-template/`.
# The bringup source tree itself is symlinked at
# `apps/external/<bringup>-source` (or copied with `--copy`); the
# template files (Make.defs, Makefile, Kconfig) are written into the
# `apps/external/<bringup>/` shell along with `nros_bringup.mk`
# pinning the workspace + bringup name. Idempotent.

set -euo pipefail

if [ $# -lt 1 ]; then
    echo "usage: $0 <nuttx-apps-dir> [nros-codegen-binary] [--bringup <dir>] [--copy]" >&2
    exit 2
fi

NUTTX_APPS_DIR="$1"; shift
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
INTEGRATION="$ROOT/integrations/nuttx"
TEMPLATE="$INTEGRATION/apps-external-template"
BRINGUP_DIR=""
COPY_MODE="symlink"
CODEGEN_ARG=""

while [ $# -gt 0 ]; do
    case "$1" in
        --bringup)
            BRINGUP_DIR="$2"; shift 2 ;;
        --copy)
            COPY_MODE="copy"; shift ;;
        *)
            if [ -z "$CODEGEN_ARG" ] && [ "${1:0:2}" != "--" ]; then
                CODEGEN_ARG="$1"; shift
            else
                echo "error: unknown arg $1" >&2; exit 2
            fi ;;
    esac
done

CODEGEN="${CODEGEN_ARG:-${NROS_CODEGEN:-${NROS_CLI:-$(command -v nros 2>/dev/null || echo "${NROS_HOME:-$HOME/.nros}/bin/nros")}}}"

if [ ! -f "$NUTTX_APPS_DIR/Make.defs" ]; then
    echo "error: $NUTTX_APPS_DIR doesn't look like a NuttX apps tree (no Make.defs)" >&2
    exit 1
fi

EXT="$NUTTX_APPS_DIR/external"
mkdir -p "$EXT"

# Top-level apps/external/Make.defs — pulls in every sub-app via
# wildcard glob (Make supports it natively).
install -m 0644 "$INTEGRATION/external-Make.defs.in" "$EXT/Make.defs"

# Integration shell.
ln -sfn "$INTEGRATION" "$EXT/nano-ros"

# Each example as a sibling under apps/external/.
# Phase 157.C.9 — each example's `main.{c,cpp}` includes
# `<nros/app_config.h>`. The cmake path generates it via
# `nano_ros_generate_config_header()`; the NuttX make path
# doesn't run cmake, so we generate the header here at staging
# time from the example's config.toml. The make build's CFLAGS
# additions for `<NANO_ROS_ROOT>/<example>/generated/include`
# (added by the per-example Makefile in 157.A) pick it up.
# Phase 157.C.17 — also accumulate cpp FFI staticlib paths into
# a central Make fragment that the integration shell's Make.defs
# `-include`s. Per-example EXTRA_LIBS additions don't propagate
# to the kernel link; only the shell's Make.defs does.
shell_extras="$EXT/nano-ros/extra_libs.mk"
: > "$shell_extras"
staged_dirs=()
for lang in c cpp; do
    for example in talker listener service-server service-client action-server action-client; do
        src="$ROOT/examples/qemu-arm-nuttx/$lang/$example"
        dst="$EXT/nano-ros-$example-$lang"
        if [ -d "$src" ]; then
            ln -sfn "$src" "$dst"
            staged_dirs+=("nano-ros-$example-$lang")
            # Generate per-example app_config.h unconditionally — the cpp
            # examples `#include <nros/app_config.h>` regardless of whether a
            # config.toml exists, and gen-app-config.py emits the cmake-path
            # defaults when config.toml is absent. (Was gated on config.toml,
            # so config-less cpp examples failed: "nros/app_config.h: No such
            # file" under the make/Application.mk path.)
            python3 "$ROOT/scripts/nuttx/gen-app-config.py" \
                "$src/config.toml" \
                "$src/generated/include/nros/app_config.h"
            # Run nros-codegen for message dependencies (parses
            # nros_generate_interfaces() calls from the example's
            # CMakeLists.txt). Skips gracefully if AMENT_PREFIX_PATH
            # is unset / interfaces can't be resolved.
            python3 "$ROOT/scripts/nuttx/gen-interfaces.py" "$src" "$CODEGEN" || true
            # Persist the staticlibs the cmake/Corrosion build produced into
            # generated/ffi/extra_libs.mk so the example's Makefile (which
            # `-include`s this fragment) can append them to EXTRA_LIBS and the
            # make/Application.mk link resolves the nros C/C++ API.
            #
            # Required for BOTH c and cpp: libnros_c.a (the nros C API) plus
            # its auxiliary side-staticlibs (`nros_c_weak_stubs`,
            # `nros_c_log_fmt`) and the per-board `nros_platform_nuttx.a`.
            # Additionally for cpp: libnros_cpp.a (the C++ wrapper) plus the
            # per-package `nano_ros_cpp_ffi_<pkg>` staticlib crates (Phase
            # 157.C.16 — cmake's nros_generate_interfaces equivalent for the
            # cpp ABI bridge).
            extras_mk="$src/generated/ffi/extra_libs.mk"
            mkdir -p "$(dirname "$extras_mk")"
            : > "$extras_mk"
            nros_target_dir="$src/build-zenoh/cargo-target/armv7a-nuttx-eabihf/release"
            if [ -d "$nros_target_dir" ]; then
                # libnros_c.a — the C API.
                if [ -f "$nros_target_dir/deps/libnros_c.a" ]; then
                    printf 'EXTRA_LIBS += %s\n' "$nros_target_dir/deps/libnros_c.a" >> "$extras_mk"
                fi
                # cpp wrapper.
                if [ "$lang" = "cpp" ]; then
                    for libcpp in "$nros_target_dir"/deps/libnros_cpp-*.a; do
                        [ -f "$libcpp" ] && \
                            printf 'EXTRA_LIBS += %s\n' "$libcpp" >> "$extras_mk"
                    done
                fi
                # Auxiliary side-staticlibs (paths carry a build-script hash).
                for pat in "$nros_target_dir"/build/nros-c-*/out/libnros_c_weak_stubs.a \
                           "$nros_target_dir"/build/nros-c-*/out/libnros_c_log_fmt.a \
                           "$nros_target_dir"/build/nros-board-nuttx-qemu-arm-*/out/libnros_platform_nuttx.a; do
                    for resolved in $pat; do
                        [ -f "$resolved" ] && \
                            printf 'EXTRA_LIBS += %s\n' "$resolved" >> "$extras_mk"
                    done
                done
            fi
            # Per-package cpp FFI staticlibs (existing mechanism).
            if [ "$lang" = "cpp" ]; then
                python3 "$ROOT/scripts/nuttx/gen-cpp-ffi-crates.py" "$src" "$CODEGEN" \
                    | while read -r lib_path; do
                        printf 'EXTRA_LIBS += %s\n' "$lib_path" >> "$extras_mk"
                    done
            fi
        else
            echo "  [skip] $src missing"
        fi
    done
done

# Top-level apps/external/Kconfig — enumerate sub-app Kconfigs
# explicitly. NuttX's bundled kconfig-conf doesn't support the
# `osource` glob directive (post-v4.18 kconfig), so generate a
# Kconfig file with explicit source statements for the integration
# shell + every staged example.
{
    echo "# Phase 157.C — apps/external/Kconfig (generated by"
    echo "# scripts/nuttx/stage-external-apps.sh). DO NOT EDIT."
    echo ""
    echo "menu \"External Modules\""
    echo ""
    echo "source \"\$APPSDIR/external/nano-ros/Kconfig\""
    for dir in "${staged_dirs[@]}"; do
        echo "source \"\$APPSDIR/external/$dir/Kconfig\""
    done
    echo ""
    echo "endmenu"
} > "$EXT/Kconfig"

# Phase 157.C.17 — collect cpp FFI staticlib paths across every
# cpp example into the shell-level extras_mk, deduped by lib
# basename. Each cpp example's gen-cpp-ffi-crates.py emits one
# `lib<crate>.a` per resolved package; multiple examples sharing
# package deps (e.g. talker + listener both need std_msgs +
# builtin_interfaces) produce SEPARATE staticlibs that all
# define the same `nros_cpp_*` symbols → "multiple definition"
# at kernel link. Pick one path per unique basename (last writer
# wins — prefer the example with the longest dep chain since its
# staticlibs include!() the most ffi.rs files).
declare -A seen_basename
declare -A path_for_basename
for example_extras in $(ls -t "$ROOT"/examples/qemu-arm-nuttx/cpp/*/generated/ffi/extra_libs.mk 2>/dev/null); do
    while IFS= read -r line; do
        # Line shape: `EXTRA_LIBS += <abs-path>`
        lib_path="${line##*= }"
        base="$(basename "$lib_path")"
        if [ -z "${seen_basename[$base]:-}" ]; then
            seen_basename[$base]=1
            path_for_basename[$base]="$lib_path"
        fi
    done < "$example_extras"
done
for base in "${!path_for_basename[@]}"; do
    printf 'EXTRA_LIBS += %s\n' "${path_for_basename[$base]}" >> "$shell_extras"
done

# Phase 212.H.2 — optional `--bringup <dir>` staging. Symlinks (or
# copies) the bringup workspace next to the apps/external/ tree and
# writes the per-bringup template (Make.defs + Makefile + Kconfig +
# nros_bringup.mk pinning) at apps/external/<bringup>/.
if [ -n "$BRINGUP_DIR" ]; then
    if [ ! -d "$BRINGUP_DIR" ]; then
        echo "error: --bringup target $BRINGUP_DIR not found" >&2
        exit 1
    fi
    if [ ! -d "$TEMPLATE" ]; then
        echo "error: integration template missing: $TEMPLATE" >&2
        exit 1
    fi
    bringup_name="$(basename "$BRINGUP_DIR")"
    bringup_workspace="$(cd "$BRINGUP_DIR/.." && pwd)"
    if [ "$(basename "$bringup_workspace")" = "src" ]; then
        bringup_workspace="$(dirname "$bringup_workspace")"
    fi
    shell_dst="$EXT/$bringup_name"
    mkdir -p "$shell_dst"
    for f in Make.defs Makefile Kconfig; do
        install -m 0644 "$TEMPLATE/$f" "$shell_dst/$f"
    done
    {
        echo "# Generated by stage-external-apps.sh (Phase 212.H.2). DO NOT EDIT."
        printf 'NROS_BRINGUP_NAME      := %s\n' "$bringup_name"
        printf 'NROS_BRINGUP_WORKSPACE := %s\n' "$bringup_workspace"
    } > "$shell_dst/nros_bringup.mk"
    src_link="$EXT/${bringup_name}-source"
    if [ "$COPY_MODE" = "copy" ]; then
        rm -rf "$src_link"
        cp -a "$BRINGUP_DIR" "$src_link"
    else
        ln -sfn "$BRINGUP_DIR" "$src_link"
    fi
    # Re-emit apps/external/Kconfig with the bringup row appended.
    {
        cat "$EXT/Kconfig"
        echo ""
        echo "# Phase 212.H.2 bringup (auto-appended)"
        echo "source \"\$APPSDIR/external/$bringup_name/Kconfig\""
    } > "$EXT/Kconfig.tmp"
    mv "$EXT/Kconfig.tmp" "$EXT/Kconfig"
    echo "Staged Phase 212 bringup pkg: $bringup_name → $shell_dst"
fi

echo "Staged nano-ros external apps under $EXT/"
ls -la "$EXT" | sed 's/^/  /'
