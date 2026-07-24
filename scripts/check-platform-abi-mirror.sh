#!/usr/bin/env bash
#
# Phase 121.4.b — drift gate for the canonical platform C ABI.
#
# `<nros/platform.h>` is the source of truth for the platform-cffi
# symbol set. The same names must appear:
#
#   1. as `pub fn <name>(...)` inside the `unsafe extern "C" { ... }`
#      block in `nros-platform-cffi/src/lib.rs` (Rust side knows the
#      signature), AND
#   2. as `pub extern "C" fn <name>(...)` in exactly one of the
#      `nros_platform_export_*!` macros (otherwise no platform crate
#      could supply a definition).
#
# This script extracts every `nros_platform_*` declaration from the
# header and fails if either occurrence is missing in the Rust file.
# Hook from `just check` so future header edits land both mirrors.

set -euo pipefail

RUST="packages/core/nros-platform-cffi/src/lib.rs"
GENERATED="packages/core/nros-platform-cffi/src/generated.rs"
INCLUDE_DIR="packages/core/nros-platform-api/include/nros"

# RFC-0054 (phase-299 W2): the extern-"C" DECLARATION half is now
# GENERATED from the headers (src/generated.rs, gen-abi-bindings.sh), so
# per-symbol parity there is by construction. The (a) check survives as an
# ALLOWLIST-COMPLETENESS guard against generated.rs (a header symbol the
# bindgen allowlist misses would silently vanish); the (b) macro-emission
# half is unchanged — the `nros_platform_export_*!` macros are hand-written
# (they EMIT definitions, the port side).
#
# We track per-header expectations via a small table so future ABI surfaces
# (e.g. interrupt / DMA) drop in by adding a row.
HEADERS_REQUIRE_MACRO=(
    "platform.h"
    "platform_net.h"
    "platform_timer.h"
)
HEADERS_EXTERN_ONLY=()

if [[ ! -f "$RUST" ]]; then
    echo "error: rust mirror not found: $RUST" >&2
    exit 2
fi

extract_symbols() {
    local header="$1"
    if [[ ! -f "$header" ]]; then
        echo "error: header not found: $header" >&2
        return 2
    fi
    # Skip `static inline` definitions: those are header-only helpers
    # (e.g. nros_platform_socket_get_fd) — they have no ABI obligation
    # and intentionally aren't declared in the Rust mirror.
    grep -v -E '^\s*static\s+inline\b' "$header" \
        | grep -oE 'nros_platform_[a-zA-Z0-9_]+[[:space:]]*\(' \
        | sed -E 's/[[:space:]]*\($//' \
        | sort -u
}

total=0
fail=0

check_header() {
    local header_name="$1"
    local require_macro="$2"  # "1" or "0"
    local header_path="$INCLUDE_DIR/$header_name"

    mapfile -t SYMBOLS < <(extract_symbols "$header_path")
    if (( ${#SYMBOLS[@]} == 0 )); then
        echo "error: no nros_platform_* symbols found in $header_path" >&2
        fail=1
        return
    fi

    local missing_extern=()
    local missing_macro=()
    for sym in "${SYMBOLS[@]}"; do
        if ! grep -qE "pub fn ${sym}\s*\(" "$GENERATED"; then
            missing_extern+=("$sym")
        fi
        if [[ "$require_macro" == "1" ]]; then
            if ! grep -qE "pub extern \"C\" fn ${sym}\s*\(" "$RUST"; then
                missing_macro+=("$sym")
            fi
        fi
    done

    if (( ${#missing_extern[@]} > 0 || ${#missing_macro[@]} > 0 )); then
        echo "drift detected between $header_path and $RUST" >&2
        if (( ${#missing_extern[@]} > 0 )); then
            echo "  missing from unsafe extern \"C\" block:" >&2
            printf '    - %s\n' "${missing_extern[@]}" >&2
        fi
        if (( ${#missing_macro[@]} > 0 )); then
            echo "  missing from nros_platform_export_*! macro emission:" >&2
            printf '    - %s\n' "${missing_macro[@]}" >&2
        fi
        fail=1
    else
        echo "$header_name clean: ${#SYMBOLS[@]} symbols match"
    fi
    total=$(( total + ${#SYMBOLS[@]} ))
}

for h in "${HEADERS_REQUIRE_MACRO[@]}"; do
    check_header "$h" "1"
done
for h in "${HEADERS_EXTERN_ONLY[@]}"; do
    check_header "$h" "0"
done

if (( fail )); then
    exit 1
fi

echo "platform C ABI mirror clean: $total symbols total across $(( ${#HEADERS_REQUIRE_MACRO[@]} + ${#HEADERS_EXTERN_ONLY[@]} )) headers"

# Phase 121.4.c.remaining — confirm each platform Rust crate invokes the
# core + net macro under `#[cfg(feature = "cffi-export")]`. Without the
# invocation the platform's lib.rs would still compile, but a downstream
# binary that pins `ConcretePlatform = CffiPlatform` would fail to link
# because no provider emits the canonical `nros_platform_*` symbols.
#
# Each entry: <lib.rs path>:<expected macros>.  EXPECTED_MACROS is a
# comma-separated list drawn from {core, net}. Bare-metal `net` is
# emitted by `nros_smoltcp::define_smoltcp_platform!` traits + then
# `nros_platform_export_net!` on the platform ZST itself.

PLATFORM_CRATES=(
    "packages/platforms/nros-platform-mps2-an385/src/lib.rs|core,net"
    "packages/platforms/nros-platform-stm32f4/src/lib.rs|core,net"
    "packages/platforms/nros-platform-esp32-qemu/src/lib.rs|core,net"
    # Phase 121.3.deprecate-rust-remove + 123.A.1.x.2 — the Rust kernel
    # crates (nros-platform-{freertos,nuttx,threadx,zephyr,posix}) were
    # deleted. The directories `packages/core/nros-platform-{rtos,posix}/`
    # now hold the C ports only. No `lib.rs` invocation to check; the C
    # source at `src/platform.c` covers the core surface, `src/net.c`
    # the net surface, `src/timer.c` the timer surface.
    # Phase 121.10 — orin-spe Rust crate deleted; FSP variant of FreeRTOS
    # provides the same kernel surface via `platform-freertos`.
)

invocation_fail=0
for entry in "${PLATFORM_CRATES[@]}"; do
    path="${entry%%|*}"
    expected="${entry##*|}"

    if [[ ! -f "$path" ]]; then
        echo "error: platform crate not found: $path" >&2
        invocation_fail=1
        continue
    fi

    IFS=',' read -ra parts <<< "$expected"
    for part in "${parts[@]}"; do
        case "$part" in
            core) sym="nros_platform_export!" ;;
            net)  sym="nros_platform_export_net!" ;;
            *)    echo "internal: unknown macro tag $part" >&2; exit 2 ;;
        esac
        if ! grep -qF "$sym" "$path"; then
            echo "drift: $path missing invocation of $sym" >&2
            invocation_fail=1
        fi
    done
done

if (( invocation_fail )); then
    exit 1
fi

echo "platform crate macro invocations clean: ${#PLATFORM_CRATES[@]} crates"
