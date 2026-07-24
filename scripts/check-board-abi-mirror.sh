#!/usr/bin/env bash
#
# Phase 173.4 — drift gate for the canonical board-entry C ABI.
#
# `<nros/board.h>` is the source of truth for the board-cffi symbol
# set. The same names must appear in `nros-board-cffi/src/lib.rs`:
#
#   1. as `pub fn <name>(...)` inside the `unsafe extern "C" { ... }`
#      block (Rust side knows the signature, so a Rust runtime can
#      call a C-supplied board), AND
#   2. as `pub extern "C" fn <name>(...)` inside the
#      `nros_board_export!` macro (so a Rust `Board` impl can supply a
#      definition consumable from C / C++).
#
# Mirrors `scripts/check-platform-abi-mirror.sh`. Hooked from
# `just check` so future header edits land both mirrors.

set -euo pipefail

RUST="packages/boards/nros-board-cffi/src/lib.rs"
# RFC-0054 (phase-299): the extern-"C" declaration half is GENERATED from
# the header (src/generated.rs); the (1) check survives as an
# allowlist-completeness guard against it. The (2) macro-emission half
# checks the hand-written nros_board_export! in lib.rs unchanged.
GENERATED="packages/boards/nros-board-cffi/src/generated.rs"
HEADER="packages/boards/nros-board-cffi/include/nros/board.h"

if [[ ! -f "$RUST" ]]; then
    echo "error: rust mirror not found: $RUST" >&2
    exit 2
fi
if [[ ! -f "$HEADER" ]]; then
    echo "error: header not found: $HEADER" >&2
    exit 2
fi

# Extract every `nros_board_*` function name from the header. Skip
# the `typedef ... (*nros_board_app_fn)` — it's a callback type, not
# a symbol the ABI provider defines.
mapfile -t SYMBOLS < <(
    grep -v -E '^\s*typedef\b' "$HEADER" \
        | grep -oE 'nros_board_[a-zA-Z0-9_]+[[:space:]]*\(' \
        | sed -E 's/[[:space:]]*\($//' \
        | sort -u
)

if (( ${#SYMBOLS[@]} == 0 )); then
    echo "error: no nros_board_* symbols found in $HEADER" >&2
    exit 1
fi

missing_extern=()
missing_macro=()
for sym in "${SYMBOLS[@]}"; do
    if ! grep -qE "pub fn ${sym}\s*\(" "$GENERATED"; then
        missing_extern+=("$sym")
    fi
    if ! grep -qE "pub extern \"C\" fn ${sym}\s*\(" "$RUST"; then
        missing_macro+=("$sym")
    fi
done

if (( ${#missing_extern[@]} > 0 || ${#missing_macro[@]} > 0 )); then
    echo "drift detected between $HEADER and $RUST" >&2
    if (( ${#missing_extern[@]} > 0 )); then
        echo "  missing from unsafe extern \"C\" block:" >&2
        printf '    - %s\n' "${missing_extern[@]}" >&2
    fi
    if (( ${#missing_macro[@]} > 0 )); then
        echo "  missing from nros_board_export! macro emission:" >&2
        printf '    - %s\n' "${missing_macro[@]}" >&2
    fi
    exit 1
fi

echo "board C ABI mirror clean: ${#SYMBOLS[@]} symbols match"
