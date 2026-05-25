set working-directory := '..'

NIGHTLY := `awk '/^channel/ {gsub(/"/, "", $3); print $3; exit}' tools/rust-toolchain.toml`

# Pinned ninja version. Ubuntu's apt `ninja-build` lags (1.10); we need
# ≥1.13 for the GNU-jobserver client (Phase 176). Installed as a static
# binary under `third-party/ninja/`; `.envrc` prepends it to PATH.
NINJA_VERSION := "1.13.2"

# Pinned GNU make version. Ubuntu ships 4.3, whose jobserver is
# pipe-fd-based; ninja 1.13's jobserver client speaks ONLY the
# fifo protocol (make ≥4.4, `--jobserver-style=fifo`). Phase 176's
# unified jobserver needs 4.4. Built from source to third-party/make/;
# `.envrc` prepends it to PATH.
MAKE_VERSION := "4.4.1"

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
#
# Rust toolchain channels and components are driven by the
# `rust-toolchain.toml` files checked into the repo (root, tools/,
# examples/**/, packages/verification/nros-verification/). No bare
# `stable`/`nightly` channels are installed here — every version is
# pinned in a TOML file and installed by `rust-pinned-toolchains`.
[group("setup")]
setup:
    @just workspace apt-packages
    @just workspace install-ninja
    @just workspace install-make
    @just workspace rust-pinned-toolchains
    @just workspace rust-targets
    @just workspace cargo-tools

# Build the pinned GNU make (>=4.4) from source under third-party/make/.
# Ubuntu's make 4.3 jobserver is pipe-fd-based; ninja 1.13's jobserver
# client only speaks fifo (make 4.4 `--jobserver-style=fifo`). Needed
# for the Phase 176 unified jobserver. `.envrc` puts it first on PATH.
# Lives under third-party/ (survives `just clean`). Idempotent.
[group("setup")]
install-make:
    #!/usr/bin/env bash
    set -euo pipefail
    want="{{MAKE_VERSION}}"
    dest="third-party/make"
    bin="$dest/make"
    if [ -x "$bin" ] && "$bin" --version 2>/dev/null | head -1 | grep -q "$want"; then
        echo "[make] $want already installed ($bin)"
        exit 0
    fi
    mkdir -p "$dest"
    tmp="$(mktemp -d)"
    trap 'rm -rf "$tmp"' EXIT
    echo "[make] downloading $want..."
    curl -fSL "https://ftp.gnu.org/gnu/make/make-$want.tar.gz" -o "$tmp/make.tar.gz"
    tar xzf "$tmp/make.tar.gz" -C "$tmp"
    echo "[make] building (./configure && ./build.sh)..."
    ( cd "$tmp/make-$want" && ./configure >/dev/null 2>&1 && ./build.sh >/dev/null 2>&1 )
    cp "$tmp/make-$want/make" "$bin"
    chmod +x "$bin"
    # Some sub-builds (cmake generators, RTOS kernel makefiles) invoke
    # `gmake`, not `make`. Provide a gmake -> make 4.4 alias so they too
    # are fifo-jobserver-capable; otherwise a stray make 4.3 chokes on
    # the inherited `--jobserver-auth=fifo:` in MAKEFLAGS.
    ln -sf make "$dest/gmake"
    echo "[make] installed: $("$bin" --version | head -1) ($bin) (+ gmake alias)"
    echo "[make] ensure third-party/make is on PATH (direnv .envrc does this)."

# Install the pinned ninja static binary under third-party/ninja/.
# Ubuntu's apt ninja is too old (1.10) for the GNU-jobserver client;
# `.envrc` puts third-party/ninja first on PATH so this one wins. Lives
# under third-party/ (not build/) so `just clean` doesn't wipe it.
# Idempotent: skips when the pinned version is already present.
[group("setup")]
install-ninja:
    #!/usr/bin/env bash
    set -euo pipefail
    want="{{NINJA_VERSION}}"
    dest="third-party/ninja"
    bin="$dest/ninja"
    if [ -x "$bin" ] && [ "$("$bin" --version 2>/dev/null)" = "$want" ]; then
        echo "[ninja] $want already installed ($bin)"
        exit 0
    fi
    case "$(uname -m)" in
        x86_64)  asset="ninja-linux.zip" ;;
        aarch64) asset="ninja-linux-aarch64.zip" ;;
        *) echo "[ninja] unsupported arch $(uname -m); install ninja >=1.13 manually" >&2; exit 1 ;;
    esac
    url="https://github.com/ninja-build/ninja/releases/download/v$want/$asset"
    echo "[ninja] downloading $want ($asset)..."
    mkdir -p "$dest"
    tmp="$(mktemp -d)"
    trap 'rm -rf "$tmp"' EXIT
    curl -fSL "$url" -o "$tmp/ninja.zip"
    unzip -o "$tmp/ninja.zip" -d "$dest" >/dev/null
    chmod +x "$bin"
    echo "[ninja] installed: $("$bin" --version) ($bin)"
    echo "[ninja] ensure third-party/ninja is on PATH (direnv .envrc does this)."

# Diagnose workspace install status (read-only).
# Returns exit 1 if anything is missing.
[group("setup")]
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

    # --- pinned ninja (>=1.13 for the GNU-jobserver client, Phase 176) ---
    ninja_bin="third-party/ninja/ninja"
    if [ -x "$ninja_bin" ] && [ "$("$ninja_bin" --version 2>/dev/null)" = "{{NINJA_VERSION}}" ]; then
        echo "  [OK] ninja {{NINJA_VERSION}} (third-party/ninja; jobserver-capable)"
    else
        echo "  [MISSING] ninja {{NINJA_VERSION}} (run: just workspace install-ninja)"
        fail=1
    fi

    # --- pinned make (>=4.4 for the fifo jobserver, Phase 176) ---
    make_bin="third-party/make/make"
    if [ -x "$make_bin" ] && "$make_bin" --version 2>/dev/null | head -1 | grep -q "{{MAKE_VERSION}}"; then
        echo "  [OK] make {{MAKE_VERSION}} (third-party/make; fifo jobserver)"
    else
        echo "  [MISSING] make {{MAKE_VERSION}} (run: just workspace install-make)"
        fail=1
    fi

    # --- rust-pinned-toolchains ---
    # Every `rust-toolchain.toml` checked into the repo must have its
    # channel installed, together with the components it declares.
    # This is the single source of truth for both versions and components
    # — no separate `rustup toolchain install` / `rustup component add`
    # steps.
    shopt -s globstar nullglob 2>/dev/null
    pinned_missing=()
    comp_missing=()
    has_component() {
        rustup component list --installed --toolchain "$2" 2>/dev/null \
            | grep -qE "^$1(-[^[:space:]]+)?$"
    }
    for f in $(just workspace _pinned-toolchain-files); do
        channel=$(awk '/^channel *=/ { gsub(/["]/, "", $3); print $3; exit }' "$f")
        [ -z "$channel" ] && continue
        if ! rustup toolchain list 2>/dev/null | grep -q "^$channel"; then
            pinned_missing+=("$channel ($f)")
            continue
        fi
        components=$(awk '/^components *=/ { sub(/^[^=]*= */, ""); gsub(/[][",]/, " "); print; exit }' "$f")
        for c in $components; do
            [ -z "$c" ] && continue
            has_component "$c" "$channel" || comp_missing+=("$channel/$c")
        done
    done
    if [ ${#pinned_missing[@]} -eq 0 ] && [ ${#comp_missing[@]} -eq 0 ]; then
        echo "  [OK] rust-pinned-toolchains"
    else
        if [ ${#pinned_missing[@]} -gt 0 ]; then
            echo "  [MISSING] rust-pinned-toolchains channels: ${pinned_missing[*]}"
        fi
        if [ ${#comp_missing[@]} -gt 0 ]; then
            echo "  [MISSING] rust-pinned-toolchains components: ${comp_missing[*]}"
        fi
        echo "            run: just workspace rust-pinned-toolchains"
        fail=1
    fi

    # --- rust-targets ---
    tgt_missing=()
    for t in thumbv7em-none-eabihf thumbv7m-none-eabi armv7r-none-eabihf riscv32imc-unknown-none-elf riscv64gc-unknown-none-elf; do
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
    for t in cargo-nextest cargo-llvm-cov espflash nros; do
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
[group("setup")]
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

# Print the list of project-owned `rust-toolchain.toml` files that
# `rust-pinned-toolchains` and `doctor` walk. Third-party vendored
# files (third-party/, scripts/zenohd/) are excluded. Private helper —
# kept as a recipe so the list has a single definition.
[private]
_pinned-toolchain-files:
    #!/usr/bin/env bash
    set -e
    shopt -s globstar nullglob
    emit() { [ -f "$1" ] && echo "$1"; }
    emit rust-toolchain.toml
    emit tools/rust-toolchain.toml
    emit packages/verification/nros-verification/rust-toolchain.toml
    for f in examples/**/rust-toolchain.toml; do
        case "$f" in */target/*) continue ;; esac
        echo "$f"
    done

# Install every pinned toolchain referenced by a checked-in
# `rust-toolchain.toml`, together with the components each file declares.
#
# This is the single source of truth for Rust toolchain versions in the
# repo:
#   - rust-toolchain.toml                                   stable + workspace components
#   - tools/rust-toolchain.toml                             pinned nightly for workspace tooling
#   - examples/qemu-arm-nuttx/rust-toolchain.toml           nightly for -Z build-std + patched libc
#   - examples/zephyr/rust-toolchain.toml                   nightly for Corrosion + Zephyr Rust
#   - packages/verification/nros-verification/…             Verus-required channel
#
# Channels and components are never duplicated into shell scripts; the
# TOML files are authoritative. Running cargo in a directory triggers
# rustup's auto-download of the channel, but *not* of declared
# components — so this recipe installs both explicitly.
[group("setup")]
rust-pinned-toolchains:
    #!/usr/bin/env bash
    set -e
    declare -A seen
    while read -r f; do
        [ -z "$f" ] && continue
        channel=$(awk '/^channel *=/ { gsub(/["]/, "", $3); print $3; exit }' "$f")
        components=$(awk '/^components *=/ { sub(/^[^=]*= */, ""); gsub(/[][",]/, " "); print; exit }' "$f")
        [ -z "$channel" ] && continue
        key="$channel | $components"
        if [ -n "${seen[$key]}" ]; then continue; fi
        seen[$key]=1
        echo "=== $channel ($f; components: ${components:-<none>}) ==="
        comp_args=""
        for c in $components; do
            [ -n "$c" ] && comp_args="$comp_args -c $c"
        done
        # shellcheck disable=SC2086
        rustup toolchain install --profile minimal "$channel" $comp_args
    done < <(just workspace _pinned-toolchain-files)

# Install cross-compilation targets.
[group("setup")]
rust-targets:
    #!/usr/bin/env bash
    set -e
    rustup target add thumbv7em-none-eabihf
    rustup target add thumbv7m-none-eabi
    # Phase 100 — AGX Orin SPE (Cortex-R5F). Tier 3, but rustup ships a
    # prebuilt rust-std for both stable and nightly so `cargo check`
    # works without `-Zbuild-std`. The SPE board crate (100.6) may
    # still opt into `-Zbuild-std` for `panic_abort` / build-std-features
    # tuning; that lives in the board crate's .cargo/config.toml.
    rustup target add armv7r-none-eabihf
    rustup target add riscv32imc-unknown-none-elf
    rustup target add riscv64gc-unknown-none-elf
    rustup +{{NIGHTLY}} target add thumbv7m-none-eabi
    if rustc +{{NIGHTLY}} --print target-list 2>/dev/null | grep -q armv7a-nuttx-eabi; then
        echo "  armv7a-nuttx-eabi (NuttX Tier 3): supported via nightly + build-std"
    else
        echo "  WARNING: armv7a-nuttx-eabi not in nightly target list — NuttX builds may fail"
    fi

# Install cargo tools (nextest, llvm-cov, espflash, nano-ros CLI).
[group("setup")]
cargo-tools:
    #!/usr/bin/env bash
    set -e
    cargo install cargo-nextest --locked
    cargo install cargo-llvm-cov --locked
    cargo install espflash --locked      || echo "WARNING: espflash install failed (non-fatal)"
    cargo install rustfilt --locked      || echo "WARNING: rustfilt install failed (non-fatal)"
    cargo install cargo-show-asm --locked || echo "WARNING: cargo-show-asm install failed (non-fatal)"
    cargo install --path packages/codegen/packages/nros-cli --locked
