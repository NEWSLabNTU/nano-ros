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
#                            "esp32c3,esp32s3,esp32"). Each chip maps
#                            to an Arduino-IDE arch subdir:
#                              esp32c3 → arduino/nros/src/esp32c3/
#                              esp32s3 → arduino/nros/src/esp32s3/
#                              esp32   → arduino/nros/src/esp32/
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
TARGETS="${ARDUINO_LIB_TARGETS:-esp32c3,esp32s3,esp32}"
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
        idf.py -B "$build_dir" set-target "$chip"
        idf.py -B "$build_dir" build
    )

    echo "==> [$chip] bundling per-component archives into libnanoros.a"
    bundle="$out_dir/libnanoros.a"
    rm -f "$bundle"

    component_archives=()
    for required in nros_c-static nros_rmw_zenoh-static nros_platform_esp_idf \
                    nros_rmw_cffi-static nros_platform_cffi-static \
                    zpico-sys-static; do
        match="$(find "$build_dir" -name "lib${required}.a" -print -quit || true)"
        if [[ -n "$match" ]]; then
            component_archives+=("$match")
        fi
    done

    if [[ ${#component_archives[@]} -eq 0 ]]; then
        echo "  no component archives located under $build_dir" >&2
        exit 3
    fi

    # `ar crsT` produces a thin archive — keeps each component's .o
    # references rather than copying them, which the Arduino IDE's
    # `precompiled=true` link step handles fine.
    ar crsT "$bundle" "${component_archives[@]}"
    strip --strip-debug "$bundle" 2>/dev/null || true

    sz=$(du -h "$bundle" | cut -f1)
    echo "  wrote $bundle ($sz, $(echo "${component_archives[@]}" | wc -w) components)"
done

echo
echo "Done. To package the Arduino library zip, run:"
echo "  scripts/arduino/package-arduino-lib.sh"
