#!/usr/bin/env bash
set -euo pipefail

# Install Espressif QEMU (qemu-system-riscv32) to ~/.local/
# Supports ESP32-C3 machine with OpenETH networking

PREFIX="${HOME}/.local"
WORKDIR="${TMPDIR:-/tmp}/espressif-qemu-build"
JOBS="$(nproc)"

echo "=== Espressif QEMU installer ==="
echo "  prefix:  ${PREFIX}"
echo "  workdir: ${WORKDIR}"
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

# Clone
if [ -d "${WORKDIR}" ]; then
    echo ">>> Reusing existing source at ${WORKDIR}"
    cd "${WORKDIR}"
    git fetch --depth=1
else
    echo ">>> Cloning espressif/qemu ..."
    git clone --depth=1 https://github.com/espressif/qemu.git "${WORKDIR}"
    cd "${WORKDIR}"
fi

# Configure (RISC-V 32-bit only, minimal)
echo ">>> Configuring ..."
./configure \
    --target-list=riscv32-softmmu \
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

# Verify
QEMU="${PREFIX}/bin/qemu-system-riscv32"
if [ -x "${QEMU}" ]; then
    echo
    echo "=== Installed successfully ==="
    "${QEMU}" --version
    echo
    echo "Binary: ${QEMU}"
    if [[ ":${PATH}:" != *":${PREFIX}/bin:"* ]]; then
        echo
        echo "NOTE: Add ~/.local/bin to your PATH if not already:"
        echo "  export PATH=\"\${HOME}/.local/bin:\${PATH}\""
    fi
else
    echo "ERROR: ${QEMU} not found after install"
    exit 1
fi
