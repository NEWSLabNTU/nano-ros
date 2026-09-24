#!/usr/bin/env bash
#
# RFC-0054 (phase-299) — regenerate the COMMITTED Rust bindings for the
# C-header ABI SSoT packages. The C headers are the single source of
# truth; the Rust side ships this generated file, so embedded/cross
# builds never need libclang. Run after ANY edit to
# `packages/core/nros-rmw-abi/include/nros/*.h`, commit the result.
# The `check-abi-bindings` gate re-runs this and fails on a diff.
#
# --no-layout-tests: bindgen's generated size/offset asserts bake the
# HOST's (64-bit) layout literals, which are WRONG on 32-bit embedded
# targets (vtable 144 vs 288 bytes) — the same file compiles for every
# target. Layout parity needs no assert: both sides are repr(C) over
# identical field types, laid out by each target's own ABI rules.
#
# bindgen-cli is PINNED — output differs across versions, which would
# make the diff gate flap. Install with:
#   cargo install bindgen-cli --locked --version "$BINDGEN_PIN"

set -euo pipefail

# shellcheck source=scripts/lib/grep-q.sh
source "$(dirname "${BASH_SOURCE[0]}")/lib/grep-q.sh"

# A FAILED bindgen must not overwrite the committed bindings. The `{ … } > tmp`
# block writes its header lines regardless of whether bindgen ran, so a clang
# error (a missing include path, a header that moved) produced a valid-looking
# stub — which then compiled to "no such symbol" errors that point at the
# CALLERS rather than at the generator. Phase 376 W3.a hit exactly that.
# A binding file with no `pub` item in it is not a binding file.
refuse_stub() {
    local f="$1"
    # issue 0726 — `nros_grep_q`, so a grep that fails to START cannot be read
    # as "no items" and delete a good regeneration.
    if ! nros_grep_q '^[[:space:]]*pub ' "$f"; then
        echo "ERROR: bindgen produced no items for $f — the generator failed;" >&2
        echo "       refusing to overwrite the committed bindings with a stub." >&2
        rm -f "$f"
        exit 1
    fi
}

cd "$(dirname "$0")/.."

BINDGEN_PIN="0.72.1"

if ! command -v bindgen >/dev/null 2>&1; then
    echo "error: bindgen-cli not installed (cargo install bindgen-cli --locked --version $BINDGEN_PIN)" >&2
    exit 2
fi
have="$(bindgen --version | awk '{print $2}')"
if [ "$have" != "$BINDGEN_PIN" ]; then
    echo "error: bindgen $have != pinned $BINDGEN_PIN (output would drift; reinstall the pin)" >&2
    exit 2
fi

# The PINNED nightly, not a bare `+nightly` — issue 1464.
#
# `rustfmt +nightly` asks rustup for a toolchain named exactly `nightly`. A
# developer box has one, which is why the bare spelling worked everywhere it was
# ever tried. `ci/docker/ci-base/Dockerfile` installs `nightly-2026-04-11` and NO
# `nightly` alias — MEASURED in that image: `rustup toolchain list` returns
# `stable-x86_64-unknown-linux-gnu` and `nightly-2026-04-11-x86_64-unknown-linux-gnu`,
# nothing else. There rustup does not fail; it goes to the network and installs
# whatever nightly is current ("info: syncing channel updates for ..."), so the
# formatter that decides the committed bytes would be an unpinned moving target
# and this gate would flap for a reason that has nothing to do with the headers.
# `scripts/api_parity/extract_rust.py` already records this exact hazard for
# rustdoc; this file was the last bare `+nightly` in the tree.
#
# Read from `tools/rust-toolchain.toml`, where the pin lives, so a bump still
# moves one file — the same awk every `just` module uses.
NIGHTLY="$(awk '/^channel/ {gsub(/"/, "", $3); print $3; exit}' tools/rust-toolchain.toml)"
[ -n "$NIGHTLY" ] || {
    echo "error: no \`channel = \"...\"\` line in tools/rust-toolchain.toml" >&2
    exit 2
}

# ...and it FAILS rather than being skipped. The old form was
# `rustfmt +nightly "$f" 2>/dev/null || true`: a step that cannot fail is not a
# step, and an unformatted regeneration does not announce itself — it reports as
# "the committed ABI bindings are stale", which is a wrong diagnosis pointing at
# the headers. (Today the pass is a NO-OP, measured: bindgen-cli formats its own
# output, and skipping rustfmt entirely leaves all three files byte-identical. So
# this is latent, not live — which is exactly how long it would have stayed
# invisible once the gate started running in the container.)
nros_rustfmt_pinned() {
    local f="$1"
    if ! rustfmt "+$NIGHTLY" "$f"; then
        echo "error: rustfmt +$NIGHTLY failed on $f — the pinned nightly decides the" >&2
        echo "       committed bytes, so an unformatted regeneration must not be written." >&2
        echo "       Install it: rustup toolchain install $NIGHTLY -c rustfmt" >&2
        exit 2
    fi
}

# ---- RMW surface: nros-rmw-abi headers -> nros-rmw-cffi/src/generated.rs
RMW_ABI="packages/core/nros-rmw-abi"
RMW_OUT="packages/rmw/cffi/src/generated.rs"

wrapper="$(mktemp --suffix=.h)"
trap 'rm -f "$wrapper"' EXIT
cat > "$wrapper" << 'EOF'
#include <nros/rmw_ret.h>
#include <nros/rmw_entity.h>
#include <nros/rmw_event.h>
#include <nros/rmw_vtable.h>
#include <nros/rmw_transport.h>
EOF

{
    echo "//! AUTO-GENERATED by scripts/gen-abi-bindings.sh (bindgen $BINDGEN_PIN) — DO NOT EDIT."
    echo "//!"
    echo "//! Source of truth: \`packages/core/nros-rmw-abi/include/nros/*.h\` (RFC-0054)."
    echo "//! Edit the headers, rerun the script, commit both."
    echo "#![allow(non_camel_case_types, non_snake_case, non_upper_case_globals)]"
    echo "#![allow(unsafe_op_in_unsafe_fn, clippy::missing_safety_doc)]"
    # Phase 376 W3.a — the ABI's types are vendor-free now (`rmw_publisher_t`,
    # `rmw_ret_t`, …), so an allowlist keyed only on `nros_rmw_*` silently DROPS
    # them. The nano-ros-prefixed patterns stay for what is still ours: the
    # vtable, the descriptor, the registration entry points.
    #
    # NOTE the comment lives HERE and not among the flags: a `#` line between
    # backslash-continued arguments ENDS the command, so the `-I` at the bottom
    # never reached clang and bindgen failed with "'nros/rmw_ret.h' file not
    # found" — while the surrounding `{ … } > file` still truncated the
    # committed bindings to a stub.
    bindgen "$wrapper" \
        --use-core \
        --ctypes-prefix core::ffi \
        --default-enum-style moduleconsts \
        --default-macro-constant-type signed \
        --allowlist-item 'rmw_.*|RMW_.*|nros_rmw_.*|NROS_RMW_.*|nros_transport_.*|NROS_TRANSPORT_.*' \
        --no-layout-tests \
        --sort-semantically \
        -- -I"$RMW_ABI/include"
} > "$RMW_OUT.tmp"
refuse_stub "$RMW_OUT.tmp"

nros_rustfmt_pinned "$RMW_OUT.tmp"
# write-if-changed: an identical rewrite still bumps mtime, which re-stales
# every fixture whose dep graph contains this file (the check lane runs this
# script on EVERY `just check` via check-abi-bindings).
if ! cmp -s "$RMW_OUT.tmp" "$RMW_OUT"; then
    mv "$RMW_OUT.tmp" "$RMW_OUT"
    echo "regenerated $RMW_OUT ($(wc -l < "$RMW_OUT") lines, bindgen $BINDGEN_PIN)"
else
    rm -f "$RMW_OUT.tmp"
    echo "unchanged $RMW_OUT"
fi

# ---- Platform surface: nros-platform-api headers -> nros-platform-cffi/src/generated.rs
# Function DECLARATIONS only (the port side defines them via the
# nros_platform_export_*! macros, which stay hand-written — they emit
# definitions, not declarations). platform_zephyr.h is excluded: it is a
# Zephyr-conditional surface consumed C-side only.
PLAT_API="packages/platform/nros-platform-api"
PLAT_OUT="packages/platform/nros-platform-cffi/src/generated.rs"

plat_wrapper="$(mktemp --suffix=.h)"
trap 'rm -f "$wrapper" "$plat_wrapper"' EXIT
cat > "$plat_wrapper" << 'EOF2'
#include <nros/platform.h>
#include <nros/platform_net.h>
#include <nros/platform_timer.h>
EOF2

{
    echo "//! AUTO-GENERATED by scripts/gen-abi-bindings.sh (bindgen $BINDGEN_PIN) — DO NOT EDIT."
    echo "//!"
    echo "//! Source of truth: \`packages/platform/nros-platform-api/include/nros/platform*.h\` (RFC-0054)."
    echo "//! Edit the headers, rerun the script, commit both."
    echo "#![allow(non_camel_case_types, non_snake_case, non_upper_case_globals)]"
    echo "#![allow(unsafe_op_in_unsafe_fn, clippy::missing_safety_doc)]"
    bindgen "$plat_wrapper" \
        --use-core \
        --ctypes-prefix core::ffi \
        --default-enum-style moduleconsts \
        --default-macro-constant-type signed \
        --allowlist-item 'nros_platform_.*|NROS_PLATFORM_.*' \
        --no-layout-tests \
        --sort-semantically \
        -- -I"$PLAT_API/include"
} > "$PLAT_OUT.tmp"
refuse_stub "$PLAT_OUT.tmp"

nros_rustfmt_pinned "$PLAT_OUT.tmp"
if ! cmp -s "$PLAT_OUT.tmp" "$PLAT_OUT"; then
    mv "$PLAT_OUT.tmp" "$PLAT_OUT"
    echo "regenerated $PLAT_OUT ($(wc -l < "$PLAT_OUT") lines, bindgen $BINDGEN_PIN)"
else
    rm -f "$PLAT_OUT.tmp"
    echo "unchanged $PLAT_OUT"
fi

# ---- Board surface: nros-board-cffi header -> nros-board-cffi/src/generated.rs
# Declarations only; the nros_board_export! macro (definitions, port side)
# stays hand-written — same split as the platform surface.
BOARD_API="packages/boards/nros-board-cffi"
BOARD_OUT="packages/boards/nros-board-cffi/src/generated.rs"

{
    echo "//! AUTO-GENERATED by scripts/gen-abi-bindings.sh (bindgen $BINDGEN_PIN) — DO NOT EDIT."
    echo "//!"
    echo "//! Source of truth: \`packages/boards/nros-board-cffi/include/nros/board.h\` (RFC-0054)."
    echo "//! Edit the header, rerun the script, commit both."
    echo "#![allow(non_camel_case_types, non_snake_case, non_upper_case_globals)]"
    echo "#![allow(unsafe_op_in_unsafe_fn, clippy::missing_safety_doc)]"
    bindgen "$BOARD_API/include/nros/board.h" \
        --use-core \
        --ctypes-prefix core::ffi \
        --default-enum-style moduleconsts \
        --default-macro-constant-type signed \
        --enable-function-attribute-detection \
        --allowlist-item 'nros_board_.*|NROS_BOARD_.*' \
        --no-layout-tests \
        --sort-semantically \
        -- -I"$BOARD_API/include"
} > "$BOARD_OUT.tmp"
refuse_stub "$BOARD_OUT.tmp"

nros_rustfmt_pinned "$BOARD_OUT.tmp"
if ! cmp -s "$BOARD_OUT.tmp" "$BOARD_OUT"; then
    mv "$BOARD_OUT.tmp" "$BOARD_OUT"
    echo "regenerated $BOARD_OUT ($(wc -l < "$BOARD_OUT") lines, bindgen $BINDGEN_PIN)"
else
    rm -f "$BOARD_OUT.tmp"
    echo "unchanged $BOARD_OUT"
fi
