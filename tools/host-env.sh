#!/usr/bin/env bash
# Phase 197.4 — host-env helper for `just <module> setup`.
#
# Provisioning (toolchains/tools + sources from the SDK index) is `nros setup
# <board>`'s job. This helper does only what is OUTSIDE nros scope + host-local:
#   - rustup (install if absent) + the per-platform cross Rust target triple
#   - an apt cross-compiler hint (informational; nros also ships arm-none-eabi-gcc
#     from the index, but a system gcc is a fine alternative — never runs sudo)
#
# Usage: tools/host-env.sh <platform>   (posix|freertos|nuttx|threadx|bare-metal|esp32)
#        [--dry-run]
set -euo pipefail

PLATFORM="${1:-}"
DRY_RUN=0
[ "${2:-}" = "--dry-run" ] && DRY_RUN=1
[ "${1:-}" = "--dry-run" ] && { DRY_RUN=1; PLATFORM="${2:-}"; }

info() { echo "[host-env] $*"; }

if [ -z "$PLATFORM" ]; then
    echo "host-env.sh: missing <platform>" >&2
    exit 2
fi

# Map nano-ros platform -> default cross Rust target triple (rustup-managed).
# Empty = host build or a build-std custom-JSON target (no rustup target needed;
# rust-src comes from the workspace toolchain).
declare -A RUST_TARGET_FOR_PLATFORM=(
    [posix]=""
    [freertos]="thumbv7m-none-eabi"
    [nuttx]=""              # armv7a-nuttx-eabihf custom JSON via build-std
    [threadx]=""            # threadx-linux is host; riscv64 uses build-std
    [zephyr]=""
    [bare-metal]="thumbv7m-none-eabi"
    [esp32]="riscv32imc-unknown-none-elf"
)

# --- rustup (install if absent; add the cross target) ---
if ! command -v rustup >/dev/null 2>&1; then
    if (( DRY_RUN )); then
        info "[dry-run] would install rustup via https://sh.rustup.rs"
    else
        info "installing rustup (no toolchain by default)..."
        curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
            | sh -s -- -y --default-toolchain none --profile minimal
        # shellcheck source=/dev/null
        source "$HOME/.cargo/env"
    fi
fi

TRIPLE="${RUST_TARGET_FOR_PLATFORM[$PLATFORM]:-}"
if [ -n "$TRIPLE" ] && command -v rustup >/dev/null 2>&1; then
    if (( DRY_RUN )); then
        info "[dry-run] rustup target add $TRIPLE"
    else
        info "ensuring rustup target: $TRIPLE"
        rustup target add "$TRIPLE" >/dev/null
    fi
fi

# --- apt cross-compiler hint (informational only — never runs sudo) ---
if [ "${OSTYPE:-}" = linux* ] || command -v dpkg >/dev/null 2>&1; then
    declare -A APT_FOR_PLATFORM=(
        [freertos]="gcc-arm-none-eabi"
        [nuttx]="gcc-arm-none-eabi kconfig-frontends genromfs"
        [threadx]="gcc-arm-none-eabi"
        [bare-metal]="gcc-arm-none-eabi"
    )
    pkgs="${APT_FOR_PLATFORM[$PLATFORM]:-}"
    if [ -n "$pkgs" ] && command -v dpkg >/dev/null 2>&1; then
        missing=()
        for p in $pkgs; do dpkg -s "$p" >/dev/null 2>&1 || missing+=("$p"); done
        if (( ${#missing[@]} > 0 )); then
            info "note: nros provides arm-none-eabi-gcc from the index (on PATH after"
            info "      \`source setup.bash\`). For a system alternative: sudo apt install ${missing[*]}"
        fi
    fi
fi

info "host-env ready (platform=$PLATFORM)"
