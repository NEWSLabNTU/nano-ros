#!/usr/bin/env bash
# Phase 185.4 — shared cross-build of Cyclone DDS `ddsc` for an embedded RTOS.
#
# Sourced (not executed) by the per-target probes — freertos-cross-probe.sh and
# threadx-cross-probe.sh. Each target sets its own toolchain, RTOS / netstack
# prerequisite checks, `c_flags`, and `cmake_args` (WITH_FREERTOS + WITH_LWIP
# vs WITH_THREADX). This file owns the boilerplate they share: prerequisite
# accounting, `--configure-only` parsing, the stale-LTO-cache self-heal, and the
# configure → build → install sequence for the `ddsc` target.
#
# `nros-rmw-cyclonedds` consumes Cyclone via `find_package(CycloneDDS)` and never
# compiles `ddsc` itself; the host x86 install can't link into an embedded image,
# so each embedded ABI needs its own cross-built `ddsc` (Phase 185 overview).

csb_missing=0

csb_check_file() {
    if [ -f "$1" ]; then
        printf '  [OK]      %s\n' "$2"
    else
        printf '  [MISSING] %s (%s)\n' "$2" "$1"
        csb_missing=1
    fi
}

csb_check_dir() {
    if [ -d "$1" ]; then
        printf '  [OK]      %s\n' "$2"
    else
        printf '  [MISSING] %s (%s)\n' "$2" "$1"
        csb_missing=1
    fi
}

# Verify the cross compiler is on PATH (counts toward csb_missing).
csb_require_compiler() {
    if command -v "$1" >/dev/null 2>&1; then
        printf '  [OK]      %s\n' "$1"
    else
        printf '  [MISSING] %s\n' "$1"
        csb_missing=1
    fi
}

# Parse the probes' single optional arg. Sets `csb_mode` to build|configure.
csb_parse_mode() {
    csb_mode="build"
    if [ "${1:-}" = "--configure-only" ]; then
        csb_mode="configure"
    elif [ "${1:-}" != "" ]; then
        echo "usage: $0 [--configure-only]" >&2
        exit 2
    fi
}

# Exit (nonzero) if any prerequisite check above flagged a miss.
csb_finalize_checks() {
    [ "$csb_missing" -eq 0 ] || exit "$csb_missing"
}

# Phase 179.G — self-heal a stale CMake cache. A build dir configured before LTO
# was disabled keeps `ENABLE_LTO:BOOL=ON`, and an incremental reconfigure leaves
# the GCC slim-LTO objects (GIMPLE bytecode, not machine code) in place, so
# rust-lld cannot resolve any `dds_*` symbol. Wipe the build dir whenever the
# cached LTO setting does not match an LTO-off config. Only meaningful for targets
# that disable LTO (ThreadX); callers that never set ENABLE_LTO skip this.
csb_wipe_stale_lto() {
    local build_dir="$1"
    local cache="$build_dir/CMakeCache.txt"
    if [ -f "$cache" ] && ! grep -q '^ENABLE_LTO:BOOL=OFF' "$cache"; then
        echo "Stale CMake cache (LTO not disabled) — wiping $build_dir for a clean reconfigure"
        rm -rf "$build_dir"
    fi
}

# Configure, then (unless --configure-only) build + install the `ddsc` target.
# Uses caller-set `csb_mode`, the `cmake_args` array, and `build_dir`/`install_dir`.
csb_configure_build_install() {
    echo
    echo "Configuring Cyclone DDS (cross-build)..."
    cmake "${cmake_args[@]}"

    if [ "$csb_mode" = "configure" ]; then
        echo
        echo "Configure-only probe passed."
        exit 0
    fi

    echo
    echo "Building Cyclone DDS ddsc target..."
    cmake --build "$build_dir" --target ddsc --parallel "${NROS_BUILD_JOBS:-4}"
    cmake --install "$build_dir" --prefix "$install_dir"
}
