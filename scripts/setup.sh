#!/usr/bin/env bash
set -e
echo "=== nros setup ==="
echo ""
echo "This will:"
echo "  1. Install system packages via apt (may prompt for sudo):"
echo "       gcc-arm-none-eabi, qemu-system-arm, qemu-system-misc, cmake, socat,"
echo "       gcc-riscv64-unknown-elf, picolibc-riscv64-unknown-elf"
echo "  2. Install Rust toolchains (stable + nightly)"
echo "  3. Add rustup components: rustfmt, clippy, rust-src, miri"
echo "  4. Add cross-compilation targets:"
echo "       - thumbv7em-none-eabihf       (ARM Cortex-M4F)"
echo "       - thumbv7m-none-eabi          (ARM Cortex-M3)"
echo "       - riscv32imc-unknown-none-elf  (ESP32-C3 RISC-V)"
echo "       - riscv64gc-unknown-none-elf   (QEMU RISC-V 64-bit)"
echo "       - armv7a-nuttx-eabi            (NuttX ARM, Tier 3 via build-std)"
echo "  5. Install cargo tools, pip packages + verification toolchains:"
echo "       - cargo-nextest          (test runner)"
echo "       - espflash               (ESP32 flash tool)"
echo "       - cargo-nano-ros         (message binding generator)"
echo "       - kani-verifier          (bounded model checking)"
echo "       - verus                  (deductive verification)"
echo "       - kconfig-frontends-nox  (NuttX Kconfig tools, apt)"
echo "  6. Build Espressif QEMU from source → ~/.local/bin/qemu-system-riscv32"
echo "     (ESP32-C3 emulator — requires git, ninja, python3, pkg-config,"
echo "      libglib2.0-dev, libpixman-1-dev, libgcrypt20-dev, libslirp-dev)"
echo "  7. Build Micro-XRCE-DDS Agent from source → build/xrce-agent/MicroXRCEAgent"
echo "     (XRCE-DDS integration tests — requires cmake, g++)"
echo "  8. Download FreeRTOS kernel + lwIP → external/freertos-kernel, external/lwip"
echo "  9. Download NuttX RTOS + apps → external/nuttx, external/nuttx-apps"
echo ""
read -r -p "Proceed? [Y/n] " answer
if [[ "$answer" =~ ^[Nn] ]]; then
    echo "Setup cancelled."
    exit 0
fi
echo ""

echo "=== [1/10] System packages (apt) ==="
apt_pkgs=()
check_apt() {
    if command -v "$2" &>/dev/null; then
        printf "  %-40s %s\n" "$1" "[already installed]"
    else
        apt_pkgs+=("$1")
    fi
}
check_apt gcc-arm-none-eabi              arm-none-eabi-gcc
check_apt qemu-system-arm                qemu-system-arm
check_apt qemu-system-misc               qemu-system-riscv64
check_apt cmake                          cmake
check_apt socat                          socat
check_apt gcc-riscv64-unknown-elf        riscv64-unknown-elf-gcc
# mbedTLS: check via header (library package, no binary)
if [ -f /usr/include/mbedtls/ssl.h ]; then
    printf "  %-40s %s\n" "libmbedtls-dev" "[already installed]"
else
    apt_pkgs+=("libmbedtls-dev")
fi
# picolibc: check via sysroot or known path (no binary to test)
picolibc_found=false
if command -v riscv64-unknown-elf-gcc &>/dev/null; then
    sysroot=$(riscv64-unknown-elf-gcc -march=rv32imc -mabi=ilp32 --specs=picolibc.specs -print-sysroot 2>/dev/null || true)
    if [ -n "$sysroot" ] && [ -d "$sysroot/include" ]; then
        picolibc_found=true
    elif [ -d "/usr/lib/picolibc/riscv64-unknown-elf/include" ]; then
        picolibc_found=true
    fi
fi
if $picolibc_found; then
    printf "  %-40s %s\n" "picolibc-riscv64-unknown-elf" "[already installed]"
else
    apt_pkgs+=("picolibc-riscv64-unknown-elf")
fi
if [ ${#apt_pkgs[@]} -gt 0 ]; then
    echo ""
    echo "  Installing: ${apt_pkgs[*]}"
    sudo apt-get install -y "${apt_pkgs[@]}"
else
    echo "  All system packages already installed."
fi
echo ""

echo "=== [2/10] Installing Rust toolchains ==="
rustup toolchain install stable
rustup toolchain install nightly
echo ""

echo "=== [3/10] Adding rustup components ==="
rustup component add rustfmt clippy rust-src
rustup component add llvm-tools
rustup component add --toolchain nightly rustfmt miri rust-src llvm-tools
echo ""

echo "=== [4/10] Adding cross-compilation targets ==="
rustup target add thumbv7em-none-eabihf
rustup target add thumbv7m-none-eabi
rustup target add riscv32imc-unknown-none-elf
rustup target add riscv64gc-unknown-none-elf
rustup +nightly target add thumbv7m-none-eabi
# NuttX: armv7a-nuttx-eabi is Tier 3 — can't install via rustup, uses -Z build-std.
# Verify the nightly compiler knows about it (rust-src installed in step 3).
if rustc +nightly --print target-list 2>/dev/null | grep -q armv7a-nuttx-eabi; then
    echo "  armv7a-nuttx-eabi (NuttX Tier 3): supported via nightly + build-std"
else
    echo "  WARNING: armv7a-nuttx-eabi not in nightly target list — NuttX builds may fail"
fi
echo ""

echo "=== [5/10] Installing cargo tools + verification toolchains ==="
cargo install cargo-nextest --locked
cargo install cargo-llvm-cov --locked
cargo install espflash --locked || echo "WARNING: espflash install failed (non-fatal)"
cargo install rustfilt --locked || echo "WARNING: rustfilt install failed (non-fatal)"
cargo install cargo-show-asm --locked || echo "WARNING: cargo-show-asm install failed (non-fatal)"
if command -v cargo-kani &>/dev/null && [ -d "$HOME/.kani" ]; then
    kani_ver=$(basename "$(ls -d "$HOME"/.kani/kani-* 2>/dev/null | grep -v '\.tar' | head -1)" 2>/dev/null || true)
    echo "kani-verifier already installed ($kani_ver)"
else
    cargo install --locked kani-verifier && cargo kani setup || echo "WARNING: kani install failed (non-fatal)"
fi
just setup-verus || echo "WARNING: Verus setup failed (non-fatal)"
cargo install --path packages/codegen/packages/cargo-nano-ros --locked
# kconfig tools: required by NuttX build (kconfig-conf or olddefconfig)
# Prefer apt (kconfig-frontends-nox) over pip (kconfiglib)
if command -v kconfig-conf &>/dev/null || command -v olddefconfig &>/dev/null; then
    echo "kconfig tools already installed"
elif command -v apt-get &>/dev/null; then
    sudo apt-get install -y kconfig-frontends-nox || echo "WARNING: kconfig-frontends-nox install failed (non-fatal, needed for just nuttx build-kernel)"
else
    pip install kconfiglib || echo "WARNING: kconfiglib install failed (non-fatal, needed for just nuttx build-kernel)"
fi
echo ""

echo "=== [6/10] Building Espressif QEMU (qemu-system-riscv32) ==="
if command -v qemu-system-riscv32 &>/dev/null; then
    echo "Already installed: $(qemu-system-riscv32 --version | head -1)"
    echo "Skipping build. To reinstall, run: ./scripts/esp32/install-espressif-qemu.sh"
else
    ./scripts/esp32/install-espressif-qemu.sh
fi
echo ""

echo "=== [7/10] Building Micro-XRCE-DDS Agent ==="
if [ -f "build/xrce-agent/MicroXRCEAgent" ]; then
    echo "Already built: build/xrce-agent/MicroXRCEAgent"
    echo "To rebuild, run: just build-xrce-agent"
else
    ./scripts/xrce-agent/build.sh || echo "WARNING: XRCE Agent build failed (non-fatal, needed for just test-xrce)"
fi
echo ""

echo "=== [8/10] Downloading FreeRTOS kernel + lwIP ==="
just freertos setup || echo "WARNING: FreeRTOS setup failed (non-fatal, needed for just freertos test)"
echo ""

echo "=== [9/10] Downloading NuttX RTOS + apps ==="
just nuttx setup || echo "WARNING: NuttX setup failed (non-fatal, needed for just nuttx test)"
echo ""

echo "=== [10/10] Downloading ThreadX + NetX Duo ==="
just threadx-linux setup || echo "WARNING: ThreadX setup failed (non-fatal, needed for just threadx-linux test)"
echo ""

echo "Setup complete!"
