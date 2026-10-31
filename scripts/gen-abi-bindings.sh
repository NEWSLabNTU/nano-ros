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
#
# TWO MODES (issue 2186):
#
#   gen-abi-bindings.sh           regenerate; write-if-changed into the tree.
#   gen-abi-bindings.sh --check   generate into a TEMP DIR and compare with the
#                                 committed files. Writes NOTHING in the tree.
#
# `check-abi-bindings` used to run the WRITE mode and then `git diff` the
# result, so a gate rewrote tracked files whenever it found them stale — which
# is exactly when it fires. On an Arch host (libclang 23) that happened on
# every `pre-push`: the regenerated files carried doc comments libclang 14 does
# not emit, the worktree went dirty, every fixture keyed on those files
# re-staled, and two OTHER fast gates (`comment-names`,
# `retired-knob-spellings`) went red reading the rewritten text. A gate reads
# the tree; it never writes it (`run-gates-parallel.sh` now refuses a lane that
# changed a tracked file).
#
# THE LIBCLANG IS AN INPUT, measured (issue 2186). libclang 14 (the ROS box and
# `ci-base`) and 12 agree byte for byte; libclang 23 additionally attaches the
# `/** */` comment above a `#define` to the macro, so bindgen emits 40 more
# `#[doc = ...]` lines in the RMW surface and 8 in the platform surface. Only
# doc attributes differ — the items, types and values are identical. So:
#
#   * WRITE mode refuses a libclang whose major is not LIBCLANG_REF_MAJOR: the
#     committed bytes would flap between hosts.
#   * CHECK mode on a different major compares with doc attributes stripped.
#     Code differs -> FAIL (really stale). Only docs differ -> NOT VERIFIED via
#     the `nros_check_skip` ledger, naming expected vs found (issue 1043's third
#     outcome): a header edit that changed only a comment cannot be judged by
#     this libclang, and saying "OK" or "stale" would both be a guess.

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
# The libclang major the COMMITTED bytes were generated with: `ci-base` and the
# ROS box both ship libclang 14 (`libclang1-14`). See TWO MODES above.
LIBCLANG_REF_MAJOR=14

mode=write
case "${1:-}" in
    "") ;;
    --check) mode=check ;;
    *) echo "usage: $0 [--check]" >&2; exit 2 ;;
esac

# In check mode a missing or unrunnable generator is a NOT VERIFIED, not a red:
# the fast line is documented to run on a pristine host.
unverified() {
    # shellcheck source=scripts/build/check-skip.sh
    source scripts/build/check-skip.sh
    nros_check_unverified abi-bindings "$*"
    exit $?
}

if ! command -v bindgen >/dev/null 2>&1; then
    [ "$mode" = check ] && unverified "bindgen-cli not installed (cargo install bindgen-cli --locked --version $BINDGEN_PIN)"
    echo "error: bindgen-cli not installed (cargo install bindgen-cli --locked --version $BINDGEN_PIN)" >&2
    exit 2
fi
# `--verbose` adds the libclang bindgen dlopen'd: "Clang: clang version 23.1.3".
# A bindgen that cannot START (a binary built against a newer glibc, reached
# through a shared $HOME) is reported as such, not as a version mismatch.
if ! bg_version="$(bindgen --version --verbose 2>&1)"; then
    [ "$mode" = check ] && unverified "bindgen on PATH ($(command -v bindgen)) does not run: $(printf '%s' "$bg_version" | head -1)"
    echo "error: bindgen does not run: $bg_version" >&2
    exit 2
fi
have="$(printf '%s\n' "$bg_version" | awk '/^bindgen /{print $2; exit}')"
if [ "$have" != "$BINDGEN_PIN" ]; then
    echo "error: bindgen $have != pinned $BINDGEN_PIN (output would drift; reinstall the pin)" >&2
    exit 2
fi
clang_major="$(printf '%s\n' "$bg_version" | sed -n 's/.*clang version \([0-9][0-9]*\).*/\1/p' | head -1)"
if [ -z "$clang_major" ]; then
    echo "error: could not read the libclang version from \`bindgen --version --verbose\`:" >&2
    printf '%s\n' "$bg_version" >&2
    exit 2
fi
if [ "$mode" = write ] && [ "$clang_major" != "$LIBCLANG_REF_MAJOR" ] \
        && [ "${NROS_ABI_BINDINGS_ANY_LIBCLANG:-}" != 1 ]; then
    echo "error: bindgen is using libclang $clang_major; the committed bindings are libclang" >&2
    echo "       $LIBCLANG_REF_MAJOR bytes, and another major emits different doc comments" >&2
    echo "       (issue 2186), so the files would flap between hosts. Regenerate where" >&2
    echo "       libclang $LIBCLANG_REF_MAJOR is (the ROS box, ci-base), or set" >&2
    echo "       NROS_ABI_BINDINGS_ANY_LIBCLANG=1 for a deliberate experiment." >&2
    exit 2
fi

# Every generated file lands HERE first, never beside its committed copy.
gen_dir="$(mktemp -d)"

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

# finish <committed> <generated> — what each mode does with one surface.
#
# write: write-if-changed. An identical rewrite still bumps mtime, which
# re-stales every fixture whose dep graph contains this file. `cp` onto the
# existing file keeps its mode (mktemp's 0600 would follow an `mv`).
# check: compare only; the verdict is taken once all three are generated.
stale=()
doc_only=()
finish() {
    local out="$1" gen="$2"
    if [ "$mode" = write ]; then
        if ! cmp -s "$gen" "$out"; then
            cp "$gen" "$out"
            echo "regenerated $out ($(wc -l < "$out") lines, bindgen $BINDGEN_PIN)"
        else
            echo "unchanged $out"
        fi
        return 0
    fi
    case "$(classify "$out" "$gen")" in
        same) return 0 ;;
        docs) [ "$clang_major" != "$LIBCLANG_REF_MAJOR" ] && { doc_only+=("$out"); return 0; } ;;
    esac
    {
        stale+=("$out")
        # On a foreign libclang show the CODE diff only: the doc-comment noise
        # is the part this host cannot judge.
        if [ "$clang_major" != "$LIBCLANG_REF_MAJOR" ]; then
            diff -u --label "$out (committed, docs stripped)" --label "$out (generated, docs stripped)" \
                <(strip_docs < "$out") <(strip_docs < "$gen") | head -40 >&2 || true
        else
            diff -u --label "$out (committed)" --label "$out (generated)" "$out" "$gen" \
                | head -40 >&2 || true
        fi
    }
}
# A bindgen doc attribute is one line: `#[doc = "..."]` with `\n` escapes.
strip_docs() { grep -v -E '^[[:space:]]*#\[doc = ".*"\][[:space:]]*$' || true; }
# classify <a> <b> -> same | docs (differ only in doc attributes) | code
classify() {
    if cmp -s "$1" "$2"; then echo same
    elif cmp -s <(strip_docs < "$1") <(strip_docs < "$2"); then echo docs
    else echo code
    fi
}
# The classifier's negative control, on every --check run: a NOT VERIFIED that
# swallowed a code change would be a silent pass on every foreign-libclang host.
selftest() {
    local d="$gen_dir/selftest"
    mkdir -p "$d"
    printf '%s\n' 'pub const A: i32 = 1;' > "$d/base"
    printf '%s\n' '#[doc = " a\n b"]' 'pub const A: i32 = 1;' > "$d/docs"
    printf '%s\n' '#[doc = " a"]' 'pub const A: i32 = 2;' > "$d/code"
    [ "$(classify "$d/base" "$d/base")" = same ] \
        && [ "$(classify "$d/base" "$d/docs")" = docs ] \
        && [ "$(classify "$d/base" "$d/code")" = code ] || {
        echo "gen-abi-bindings: SELFTEST FAIL — the doc-only classifier misjudges a known pair" >&2
        exit 1
    }
}
[ "$mode" = check ] && selftest

# ---- RMW surface: nros-rmw-abi headers -> nros-rmw-cffi/src/generated.rs
RMW_ABI="packages/core/nros-rmw-abi"
RMW_OUT="packages/rmw/cffi/src/generated.rs"

wrapper="$(mktemp --suffix=.h)"
trap 'rm -rf "$wrapper" "$gen_dir"' EXIT
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
} > "$gen_dir/rmw.rs"
refuse_stub "$gen_dir/rmw.rs"

nros_rustfmt_pinned "$gen_dir/rmw.rs"
finish "$RMW_OUT" "$gen_dir/rmw.rs"

# ---- Platform surface: nros-platform-api headers -> nros-platform-cffi/src/generated.rs
# Function DECLARATIONS only (the port side defines them via the
# nros_platform_export_*! macros, which stay hand-written — they emit
# definitions, not declarations). platform_zephyr.h is excluded: it is a
# Zephyr-conditional surface consumed C-side only.
PLAT_API="packages/platform/nros-platform-api"
PLAT_OUT="packages/platform/nros-platform-cffi/src/generated.rs"

plat_wrapper="$(mktemp --suffix=.h)"
trap 'rm -rf "$wrapper" "$plat_wrapper" "$gen_dir"' EXIT
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
} > "$gen_dir/platform.rs"
refuse_stub "$gen_dir/platform.rs"

nros_rustfmt_pinned "$gen_dir/platform.rs"
finish "$PLAT_OUT" "$gen_dir/platform.rs"

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
} > "$gen_dir/board.rs"
refuse_stub "$gen_dir/board.rs"

nros_rustfmt_pinned "$gen_dir/board.rs"
finish "$BOARD_OUT" "$gen_dir/board.rs"

[ "$mode" = check ] || exit 0
if [ "${#stale[@]}" -gt 0 ]; then
    echo "ERROR: committed ABI bindings are stale: ${stale[*]}" >&2
    echo "       Headers changed without rerunning scripts/gen-abi-bindings.sh;" >&2
    echo "       regenerate (libclang $LIBCLANG_REF_MAJOR) and commit the result." >&2
    [ "$clang_major" != "$LIBCLANG_REF_MAJOR" ] && \
        echo "       (libclang $clang_major here; the diff above is in CODE, not only doc comments.)" >&2
    exit 1
fi
if [ "${#doc_only[@]}" -gt 0 ]; then
    unverified "bindgen here uses libclang $clang_major, expected $LIBCLANG_REF_MAJOR: ${doc_only[*]} differ only in doc comments (code matches). Run \`just check abi-bindings\` where libclang $LIBCLANG_REF_MAJOR is (the ROS box, ci-base) to verify the comments too (issue 2186)."
fi
echo "ABI bindings match the C-header SSoT (bindgen $BINDGEN_PIN, libclang $clang_major)."
