set working-directory := '..'

# Workspace module — host-agnostic dev environment setup.
#
# "workspace" here means the developer's dev workspace (host OS packages,
# Rust toolchains, cargo tools) — NOT the Cargo `[workspace]` term-of-art.
# These recipes configure the host machine to be able to do anything with
# this repo. Platform-specific setup lives in the per-platform modules
# (just/esp32.just, just/nuttx.just, etc.).

default:
    @just --list --justfile {{source_file()}}

# Install all workspace prerequisites (apt + rust + cargo tools).
setup:
    @just workspace apt-packages
    @just workspace rust-toolchains
    @just workspace rust-components
    @just workspace rust-targets
    @just workspace cargo-tools

# Diagnose workspace install status (read-only).
# Returns exit 1 if anything is missing.
doctor:
    #!/usr/bin/env bash
    set +e
    fail=0
    check() {
        if "$@" &>/dev/null; then return 0; else return 1; fi
    }

    # --- apt-packages ---
    apt_missing=()
    check command -v arm-none-eabi-gcc  || apt_missing+=(gcc-arm-none-eabi)
    check command -v qemu-system-arm    || apt_missing+=(qemu-system-arm)
    check command -v qemu-system-riscv64 || apt_missing+=(qemu-system-misc)
    check command -v cmake               || apt_missing+=(cmake)
    check command -v socat               || apt_missing+=(socat)
    check command -v riscv64-unknown-elf-gcc || apt_missing+=(gcc-riscv64-unknown-elf)
    [ -f /usr/include/mbedtls/ssl.h ] || apt_missing+=(libmbedtls-dev)
    if [ ${#apt_missing[@]} -eq 0 ]; then
        echo "  [OK] apt-packages"
    else
        echo "  [MISSING] apt-packages: ${apt_missing[*]} (run: just workspace apt-packages)"
        fail=1
    fi

    # --- rust-toolchains ---
    if rustup toolchain list 2>/dev/null | grep -q '^stable' \
       && rustup toolchain list 2>/dev/null | grep -q '^nightly'; then
        echo "  [OK] rust-toolchains: stable + nightly"
    else
        echo "  [MISSING] rust-toolchains (run: just workspace rust-toolchains)"
        fail=1
    fi

    # --- rust-components ---
    # rust-src is installed as exactly "rust-src" (no host suffix);
    # other components are "<name>-<host>" (e.g. "rustfmt-x86_64-unknown-linux-gnu").
    comp_missing=()
    has_component() {
        rustup component list --installed ${2:+--toolchain "$2"} 2>/dev/null \
            | grep -qE "^$1(-[^[:space:]]+)?$"
    }
    for c in rustfmt clippy rust-src llvm-tools; do
        has_component "$c" || comp_missing+=("$c")
    done
    for c in rustfmt miri rust-src llvm-tools; do
        has_component "$c" nightly || comp_missing+=("nightly/$c")
    done
    if [ ${#comp_missing[@]} -eq 0 ]; then
        echo "  [OK] rust-components"
    else
        echo "  [MISSING] rust-components: ${comp_missing[*]} (run: just workspace rust-components)"
        fail=1
    fi

    # --- rust-targets ---
    tgt_missing=()
    for t in thumbv7em-none-eabihf thumbv7m-none-eabi riscv32imc-unknown-none-elf riscv64gc-unknown-none-elf; do
        rustup target list --installed 2>/dev/null | grep -q "^$t$" || tgt_missing+=("$t")
    done
    if [ ${#tgt_missing[@]} -eq 0 ]; then
        echo "  [OK] rust-targets"
    else
        echo "  [MISSING] rust-targets: ${tgt_missing[*]} (run: just workspace rust-targets)"
        fail=1
    fi

    # --- cargo-tools ---
    tool_missing=()
    for t in cargo-nextest cargo-llvm-cov espflash cargo-nano-ros; do
        check command -v "$t" || tool_missing+=("$t")
    done
    if [ ${#tool_missing[@]} -eq 0 ]; then
        echo "  [OK] cargo-tools"
    else
        echo "  [MISSING] cargo-tools: ${tool_missing[*]} (run: just workspace cargo-tools)"
        fail=1
    fi

    # --- toolchain env vars (exported by .envrc with sensible defaults) ---
    env_missing=()
    for v in NUTTX_DIR THREADX_DIR NETX_DIR FREERTOS_DIR LWIP_DIR; do
        if [ -z "${!v:-}" ]; then
            env_missing+=("$v")
        fi
    done
    if [ ${#env_missing[@]} -eq 0 ]; then
        echo "  [OK] toolchain env vars exported"
    else
        echo "  [INFO] toolchain env vars not in shell: ${env_missing[*]}"
        echo "        Run 'direnv allow' or 'source .envrc' to export defaults."
    fi

    exit $fail

# =============================================================================
# Sub-recipes — each can be run individually
# =============================================================================

# Install required system packages via apt (requires sudo).
apt-packages:
    #!/usr/bin/env bash
    set -e
    pkgs=()
    check_apt() {
        if command -v "$2" &>/dev/null; then
            printf "  %-40s %s\n" "$1" "[already installed]"
        else
            pkgs+=("$1")
        fi
    }
    check_apt gcc-arm-none-eabi              arm-none-eabi-gcc
    check_apt qemu-system-arm                qemu-system-arm
    check_apt qemu-system-misc               qemu-system-riscv64
    check_apt cmake                          cmake
    check_apt socat                          socat
    check_apt gcc-riscv64-unknown-elf        riscv64-unknown-elf-gcc
    if [ -f /usr/include/mbedtls/ssl.h ]; then
        printf "  %-40s %s\n" "libmbedtls-dev" "[already installed]"
    else
        pkgs+=(libmbedtls-dev)
    fi
    # picolibc: check via sysroot
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
        pkgs+=(picolibc-riscv64-unknown-elf)
    fi
    # kconfig-frontends-nox: required by NuttX build
    if command -v kconfig-conf &>/dev/null || command -v olddefconfig &>/dev/null; then
        printf "  %-40s %s\n" "kconfig-frontends-nox" "[already installed]"
    else
        pkgs+=(kconfig-frontends-nox)
    fi
    if [ ${#pkgs[@]} -eq 0 ]; then
        echo "All system packages already installed."
    else
        echo "Installing: ${pkgs[*]}"
        sudo apt-get install -y "${pkgs[@]}"
    fi

# Install Rust stable + nightly toolchains via rustup.
rust-toolchains:
    rustup toolchain install stable
    rustup toolchain install nightly

# Install rustup components (rustfmt, clippy, rust-src, miri, llvm-tools).
rust-components:
    rustup component add rustfmt clippy rust-src llvm-tools
    rustup component add --toolchain nightly rustfmt miri rust-src llvm-tools

# Install cross-compilation targets.
rust-targets:
    #!/usr/bin/env bash
    set -e
    rustup target add thumbv7em-none-eabihf
    rustup target add thumbv7m-none-eabi
    rustup target add riscv32imc-unknown-none-elf
    rustup target add riscv64gc-unknown-none-elf
    rustup +nightly target add thumbv7m-none-eabi
    if rustc +nightly --print target-list 2>/dev/null | grep -q armv7a-nuttx-eabi; then
        echo "  armv7a-nuttx-eabi (NuttX Tier 3): supported via nightly + build-std"
    else
        echo "  WARNING: armv7a-nuttx-eabi not in nightly target list — NuttX builds may fail"
    fi

# Install cargo tools (nextest, llvm-cov, espflash, nano-ros codegen).
cargo-tools:
    #!/usr/bin/env bash
    set -e
    cargo install cargo-nextest --locked
    cargo install cargo-llvm-cov --locked
    cargo install espflash --locked      || echo "WARNING: espflash install failed (non-fatal)"
    cargo install rustfilt --locked      || echo "WARNING: rustfilt install failed (non-fatal)"
    cargo install cargo-show-asm --locked || echo "WARNING: cargo-show-asm install failed (non-fatal)"
    cargo install --path packages/codegen/packages/cargo-nano-ros --locked
