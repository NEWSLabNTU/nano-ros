#!/usr/bin/env bash
#
# Phase 23.2 — Cross-compile every nano-ros static archive that the
# Arduino library needs, for one or more ESP32 chip variants, then
# bundle them into a single `arduino/nros/src/<arch>/libnanoros.a`.
#
# Usage:
#   scripts/arduino/build-libnanoros.sh [--targets "esp32c3,esp32s3"]
#                                       [--clean]
#
# Env overrides:
#   NROS_ESP_IDF_WORKSPACE   Path to the ESP-IDF checkout (default:
#                            `$repo/esp-idf-workspace/esp-idf`). Must be
#                            populated first via `just esp_idf setup`.
#   ARDUINO_LIB_TARGETS      Comma-separated chip list (default:
#                            "esp32c3"). Each chip maps to an
#                            Arduino-IDE arch subdir:
#                              esp32c3 → arduino/nros/src/esp32c3/
#                              esp32s3 → arduino/nros/src/esp32s3/
#                              esp32   → arduino/nros/src/esp32/
#
#                            ESP32-C3 (RISC-V) builds out-of-the-box
#                            with the stock `rustup` `riscv32imc-
#                            unknown-none-elf` target. ESP32 / ESP32-S3
#                            (Xtensa) need esp-rs's custom rustc fork
#                            (`espup install`) — track as
#                            Phase 23.2.x follow-up.
#
# Output layout:
#   build/arduino/<chip>/   — IDF build dir (cached between runs).
#   arduino/nros/src/<arch>/libnanoros.a — bundled archive.
#
# How the bundle is produced:
#   1. `idf.py set-target <chip>` + `idf.py build` against the IDF
#      project at `scripts/arduino/idf-builder/`. That project pulls
#      `integrations/esp-idf` as a component, which transitively
#      builds `nros_c-static`, `libzpico.a`, and
#      `nros_platform_esp_idf` as IDF component archives.
#   2. Walk the IDF build dir for each component's `.a` and merge
#      them with `ar crsT` (thin archive — keeps original .o paths).
#   3. Run `strip --strip-debug` on the result to keep the Arduino
#      Library Manager zip small.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
NANO_ROS_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
BUILDER_DIR="$SCRIPT_DIR/idf-builder"
ARDUINO_LIB_ROOT="$NANO_ROS_ROOT/arduino/nros"

WORKSPACE_DIR="${NROS_ESP_IDF_WORKSPACE:-$NANO_ROS_ROOT/esp-idf-workspace/esp-idf}"
TARGETS="${ARDUINO_LIB_TARGETS:-esp32c3}"
CLEAN=0

while [[ $# -gt 0 ]]; do
    case "$1" in
        --targets) TARGETS="$2"; shift 2 ;;
        --clean)   CLEAN=1; shift ;;
        -h|--help)
            sed -n '1,38p' "${BASH_SOURCE[0]}"
            exit 0
            ;;
        *) echo "unknown flag: $1" >&2; exit 1 ;;
    esac
done

if [[ ! -f "$NANO_ROS_ROOT/esp-idf-workspace/env.sh" ]]; then
    echo "esp-idf-workspace/env.sh missing — run \`just esp_idf setup\` first" >&2
    exit 2
fi

# shellcheck disable=SC1091
source "$NANO_ROS_ROOT/esp-idf-workspace/env.sh"

if ! command -v idf.py >/dev/null 2>&1; then
    echo "idf.py not on PATH after sourcing env.sh — re-run \`just esp_idf setup\`" >&2
    exit 2
fi

# Resolve chip → Arduino arch subdir. arduino-esp32 names match the
# IDF chip names except for the base ESP32 (Xtensa LX6), which the
# Arduino core simply calls "esp32".
arduino_arch_for() {
    case "$1" in
        esp32)   echo "esp32" ;;
        esp32c3) echo "esp32c3" ;;
        esp32s3) echo "esp32s3" ;;
        *)       echo "unsupported chip: $1" >&2; exit 1 ;;
    esac
}

for chip in $(echo "$TARGETS" | tr ',' ' '); do
    arch="$(arduino_arch_for "$chip")"
    build_dir="$NANO_ROS_ROOT/build/arduino/$chip"
    out_dir="$ARDUINO_LIB_ROOT/src/$arch"
    mkdir -p "$build_dir" "$out_dir"

    if [[ "$CLEAN" -eq 1 ]]; then
        rm -rf "$build_dir"
        mkdir -p "$build_dir"
    fi

    echo "==> [$chip] cross-compiling via idf.py build (out: $build_dir)"
    (
        cd "$BUILDER_DIR"
        # Phase 21.6 — scrub vanilla-FreeRTOS env that direnv / .env
        # injects for the mps2-an385 build. zpico-sys's build.rs
        # treats any non-empty `FREERTOS_DIR` / `LWIP_DIR` as a
        # request to inject the ARM Cortex-M3 FreeRTOS port headers,
        # which clashes with ESP-IDF's RISC-V / Xtensa compile
        # flags. ESP-IDF supplies kernel + lwIP includes through its
        # own component manager — no `FREERTOS_*` env required.
        unset FREERTOS_DIR FREERTOS_PORT FREERTOS_CONFIG_DIR LWIP_DIR

        # Phase 21.10.A — two-pass build. The first `idf.py
        # reconfigure` lets the integration shell walk every
        # `__idf_<comp>` target's INTERFACE_INCLUDE_DIRECTORIES,
        # collect the FreeRTOS / lwIP / esp_* include dirs, and
        # write them to `nros_esp_idf_rust_cflags.env`. We then
        # source that file so `CFLAGS_<rust-target>` is in the
        # shell env before `idf.py build` re-launches ninja —
        # cmake's `set(ENV{...})` at configure time does NOT
        # propagate to Corrosion's `cmake -E env` invocations
        # (those inherit ninja's launch env). Corrosion 0.5
        # `corrosion_set_env_vars` cannot help here either: the
        # genex it relies on reads a property off a bare-name
        # target Corrosion never creates.
        idf.py -B "$build_dir" set-target "$chip"
        idf.py -B "$build_dir" reconfigure
        if [[ -f "$build_dir/nros_esp_idf_rust_cflags.env" ]]; then
            echo "==> [$chip] sourcing Rust CFLAGS"
            set -a
            # shellcheck disable=SC1091
            source "$build_dir/nros_esp_idf_rust_cflags.env"
            set +a
        else
            echo "  WARN: $build_dir/nros_esp_idf_rust_cflags.env not found" >&2
        fi
        idf.py -B "$build_dir" build
    )

    echo "==> [$chip] bundling per-component archives into libnanoros.a"
    bundle="$out_dir/libnanoros.a"
    rm -f "$bundle"

    # IDF emits per-component static archives under
    # `<build>/esp-idf/<component>/lib<name>.a`. The nano-ros
    # umbrella component (`integrations/esp-idf` staged under
    # `components/nano-ros`) adds `nros-platform-esp-idf` and the
    # Corrosion-built `libnros_c.a` / `libnros_cpp.a` /
    # `libnros_rmw_zenoh_staticlib.a` to its own build tree under
    # `esp-idf/nano-ros/nano_ros_root/...`. zenoh-pico's C archive
    # (`libzenohpico.a`) and the canonical platform-aliases TU
    # (`libzpico_platform_aliases.a`) live under
    # `<cargo>/.../build/zpico-sys-*/out/`.
    component_archives=()
    while IFS= read -r match; do
        component_archives+=("$match")
    done < <(
        # Prefer the merged `esp-idf/<comp>/lib<comp>.a` static archives
        # that IDF produces — these contain just the .o files for the
        # component, no test/main/etc. duplicates.
        find "$build_dir/esp-idf" -maxdepth 4 \
            \( -name "libnros_c.a" -o -name "libnros_cpp.a" \
               -o -name "libnros_rmw_zenoh_staticlib.a" \
               -o -name "libnros-platform-esp-idf.a" \) \
            -print 2>/dev/null
        # The zenoh-pico vendor lib + alias TU live in zpico-sys's
        # cargo build out dir; pick the freshest copy.
        find "$build_dir/cargo" -path "*/build/zpico-sys-*/out/libzenohpico.a" \
            -print 2>/dev/null
        find "$build_dir/cargo" -path "*/build/zpico-sys-*/out/libzpico_platform_aliases.a" \
            -print 2>/dev/null
        # cc-rs weak-stubs that nros-c / nros-cpp's build.rs emits.
        find "$build_dir/cargo" -path "*/build/nros-c-*/out/libnros_c_weak_stubs.a" \
            -print 2>/dev/null
        find "$build_dir/cargo" -path "*/build/nros-cpp-*/out/libnros_cpp_weak_stubs.a" \
            -print 2>/dev/null
    )

    if [[ ${#component_archives[@]} -eq 0 ]]; then
        echo "  no component archives located under $build_dir" >&2
        exit 3
    fi

    # `ar crsT` produces a thin archive — keeps each component's .o
    # references rather than copying them. Arduino IDE's
    # `precompiled=true` link step de-references this at sketch
    # link time so the per-component .a files must remain reachable
    # at the recorded paths. For a fully self-contained zip we'd
    # want `ar -M` with `addlib` / `save` to copy objects — track
    # as 23.2.x follow-up.
    ar crsT "$bundle" "${component_archives[@]}"
    strip --strip-debug "$bundle" 2>/dev/null || true

    sz=$(du -h "$bundle" | cut -f1)
    echo "  wrote $bundle ($sz, ${#component_archives[@]} components)"

    # Phase 23.5b — copy the per-build `nros_config_generated.h`
    # next to the public nros headers so Arduino sketches (and the
    # QEMU smoke test under `tests/arduino/`) can `#include
    # <nros/init.h>` without depending on CORROSION_BUILD_DIR. The
    # header is written by nros-c's build.rs at cross-compile time
    # and contains the SIZE / OPAQUE_U64S macros that the bundled
    # public headers reference.
    cfg_header="$(find "$build_dir" -path "*/nros-c/include/nros/nros_config_generated.h" -print -quit || true)"
    if [[ -z "$cfg_header" ]]; then
        cfg_header="$(find "$build_dir" -name "nros_config_generated.h" -print -quit || true)"
    fi
    if [[ -n "$cfg_header" ]]; then
        nros_inc_dir="$ARDUINO_LIB_ROOT/src/nros"
        mkdir -p "$nros_inc_dir"
        cp "$cfg_header" "$nros_inc_dir/nros_config_generated.h"
        # Mirror the public hand-written headers next to it so the
        # sketch's `#include <nros/init.h>` etc. all resolve from
        # the library's own src/nros/ dir.
        for h in init.h node.h publisher.h subscription.h client.h \
                 service.h action.h executor.h clock.h types.h \
                 visibility.h platform.h check.h cdr.h app_main.h \
                 component.h bridge.h guard_condition.h lifecycle.h \
                 parameter.h nros.h nros_generated.h; do
            src="$NANO_ROS_ROOT/packages/core/nros-c/include/nros/$h"
            [[ -f "$src" ]] && cp "$src" "$nros_inc_dir/$h"
        done
        echo "  staged $(ls "$nros_inc_dir"/*.h 2>/dev/null | wc -l) public headers + nros_config_generated.h under $nros_inc_dir"
    else
        echo "  WARN: nros_config_generated.h not located under $build_dir" >&2
    fi
done

# Phase 23.1.4 — bundle the common ROS 2 message-type C headers
# under `arduino/nros/src/<pkg>/`. One run covers every chip; the
# generated layout is target-agnostic. `cargo nano-ros generate-c`
# (and the standalone `nano-ros` binary it wraps) reads the deps
# from `scripts/arduino/msg-bundle/package.xml` and emits C
# headers for every transitively-reachable interface package.
echo
echo "==> bundling ROS 2 interface headers"
gen_root="$NANO_ROS_ROOT/build/arduino/msg-gen"
mkdir -p "$gen_root"
nano_ros_bin="$NANO_ROS_ROOT/packages/codegen/packages/target/debug/nano-ros"
if [[ ! -x "$nano_ros_bin" ]]; then
    (cd "$NANO_ROS_ROOT/packages/codegen/packages" && cargo build -p cargo-nano-ros --bin nano-ros >/dev/null 2>&1)
fi
"$nano_ros_bin" generate-c \
    --manifest-path "$SCRIPT_DIR/msg-bundle/package.xml" \
    --output "$gen_root" \
    --force | tail -10 || true

for pkg_dir in "$gen_root"/*/; do
    [[ -d "$pkg_dir" ]] || continue
    pkg=$(basename "$pkg_dir")
    dest="$ARDUINO_LIB_ROOT/src/$pkg"
    rm -rf "$dest"
    mkdir -p "$dest"
    cp -r "$pkg_dir"* "$dest/"
done
n_headers=$(find "$ARDUINO_LIB_ROOT/src" -mindepth 2 -name '*.h' | wc -l)
echo "  bundled $n_headers ROS interface headers under arduino/nros/src/"

echo
echo "Done. To package the Arduino library zip, run:"
echo "  scripts/arduino/package-arduino-lib.sh"
