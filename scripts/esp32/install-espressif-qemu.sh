#!/usr/bin/env bash
set -euo pipefail

# Install Espressif QEMU to ~/.local/
#
# - `qemu-system-riscv32` covers the ESP32-C3 machine (default)
# - `qemu-system-xtensa` covers the ESP32 / ESP32-S2 / ESP32-S3 machines
#   (Phase 117.0 — opt in via NROS_ESP32_QEMU_TARGETS=riscv32,xtensa)
#
# All Espressif boards (Phase 89.4 OpenETH ESP32-C3, Phase 117 ESP32-S3
# PSRAM DDS bring-up) consume from the same install prefix.

PREFIX="${HOME}/.local"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
SRCDIR="${REPO_ROOT}/third-party/esp32/qemu"
JOBS="$(nproc)"

echo "=== Espressif QEMU installer ==="
echo "  prefix:  ${PREFIX}"
echo "  source:  ${SRCDIR}"
echo "  jobs:    ${JOBS}"
echo

# Check build dependencies
MISSING=()
for cmd in git ninja python3 pkg-config; do
    if ! command -v "$cmd" &>/dev/null; then
        MISSING+=("$cmd")
    fi
done

for lib in glib-2.0 pixman-1 libgcrypt slirp; do
    if ! pkg-config --exists "$lib" 2>/dev/null; then
        MISSING+=("$lib")
    fi
done

if [ ${#MISSING[@]} -gt 0 ]; then
    echo "ERROR: Missing dependencies: ${MISSING[*]}"
    echo
    echo "On Ubuntu/Debian, install with:"
    echo "  sudo apt-get install -y git ninja-build python3 pkg-config \\"
    echo "    libglib2.0-dev libpixman-1-dev libgcrypt20-dev libslirp-dev"
    exit 1
fi

# Verify submodule is initialized
if [ ! -f "${SRCDIR}/configure" ]; then
    echo "ERROR: Espressif QEMU submodule not initialized at ${SRCDIR}"
    echo "Run: git submodule update --init third-party/esp32/qemu"
    exit 1
fi
cd "${SRCDIR}"

# Resolve the per-arch target list. Defaults to RISC-V 32-bit only
# (back-compat with the original ESP32-C3 use case). Set
# `NROS_ESP32_QEMU_TARGETS=riscv32,xtensa` (or just `xtensa`) to
# extend; comma-separated values map to QEMU's `<arch>-softmmu`
# target names.
TARGETS_RAW="${NROS_ESP32_QEMU_TARGETS:-riscv32}"
IFS=',' read -ra TARGET_ARCHES <<<"${TARGETS_RAW}"
TARGET_LIST=""
for arch in "${TARGET_ARCHES[@]}"; do
    arch_trimmed="$(echo "$arch" | tr -d '[:space:]')"
    case "${arch_trimmed}" in
        riscv32) TARGET_LIST+="riscv32-softmmu," ;;
        xtensa)  TARGET_LIST+="xtensa-softmmu," ;;
        *)       echo "ERROR: unsupported arch in NROS_ESP32_QEMU_TARGETS: ${arch_trimmed}"; exit 1 ;;
    esac
done
TARGET_LIST="${TARGET_LIST%,}"
echo ">>> Target list: ${TARGET_LIST}"

# Configure
echo ">>> Configuring ..."
./configure \
    --target-list="${TARGET_LIST}" \
    --prefix="${PREFIX}" \
    --enable-gcrypt \
    --enable-slirp \
    --disable-strip \
    --disable-user \
    --disable-capstone \
    --disable-vnc \
    --disable-gtk \
    --disable-sdl \
    --disable-docs

# Build
echo ">>> Building with ${JOBS} jobs ..."
ninja -C build -j "${JOBS}"

# Install
echo ">>> Installing to ${PREFIX} ..."
ninja -C build install

# Verify every requested arch landed
echo
echo "=== Installed successfully ==="
ANY_MISSING=0
for arch in "${TARGET_ARCHES[@]}"; do
    arch_trimmed="$(echo "$arch" | tr -d '[:space:]')"
    case "${arch_trimmed}" in
        riscv32) bin="${PREFIX}/bin/qemu-system-riscv32"; machine="esp32c3" ;;
        xtensa)  bin="${PREFIX}/bin/qemu-system-xtensa";  machine="esp32s3" ;;
        *)       continue ;;
    esac
    if [ -x "${bin}" ]; then
        echo "  [OK] ${bin}"
        if "${bin}" -machine help 2>/dev/null | grep -q "\b${machine}\b"; then
            echo "       supports ${machine} machine"
        else
            echo "       WARNING: ${machine} machine not listed by -machine help"
            ANY_MISSING=1
        fi
    else
        echo "  [MISSING] ${bin}"
        ANY_MISSING=1
    fi
done
if [ "${ANY_MISSING}" -ne 0 ]; then
    exit 1
fi
if [[ ":${PATH}:" != *":${PREFIX}/bin:"* ]]; then
    echo
    echo "NOTE: Add ~/.local/bin to your PATH if not already:"
    echo "  export PATH=\"\${HOME}/.local/bin:\${PATH}\""
fi
