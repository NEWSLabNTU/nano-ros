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

HEADER="packages/core/nros-platform-cffi/include/nros/platform.h"
RUST="packages/core/nros-platform-cffi/src/lib.rs"

if [[ ! -f "$HEADER" ]]; then
    echo "error: header not found: $HEADER" >&2
    exit 2
fi
if [[ ! -f "$RUST" ]]; then
    echo "error: rust mirror not found: $RUST" >&2
    exit 2
fi

# Extract function names declared in the header. Match any line that
# contains `nros_platform_<ident>(` followed eventually by `;`.
# Strips the call-site form returned later by the `as` cast in tests.
mapfile -t SYMBOLS < <(
    grep -oE 'nros_platform_[a-zA-Z0-9_]+[[:space:]]*\(' "$HEADER" \
        | sed -E 's/[[:space:]]*\($//' \
        | sort -u
)

if (( ${#SYMBOLS[@]} == 0 )); then
    echo "error: no nros_platform_* symbols found in $HEADER" >&2
    exit 2
fi

fail=0
missing_extern=()
missing_macro=()

for sym in "${SYMBOLS[@]}"; do
    if ! grep -qE "pub fn ${sym}\s*\(" "$RUST"; then
        missing_extern+=("$sym")
        fail=1
    fi
    if ! grep -qE "pub extern \"C\" fn ${sym}\s*\(" "$RUST"; then
        missing_macro+=("$sym")
        fail=1
    fi
done

if (( fail )); then
    echo "platform C ABI drift detected between $HEADER and $RUST" >&2
    if (( ${#missing_extern[@]} > 0 )); then
        echo "  missing from unsafe extern \"C\" block:" >&2
        printf '    - %s\n' "${missing_extern[@]}" >&2
    fi
    if (( ${#missing_macro[@]} > 0 )); then
        echo "  missing from nros_platform_export_*! macro emission:" >&2
        printf '    - %s\n' "${missing_macro[@]}" >&2
    fi
    exit 1
fi

echo "platform C ABI mirror clean: ${#SYMBOLS[@]} symbols match in $RUST"
