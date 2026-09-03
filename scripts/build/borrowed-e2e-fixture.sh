#!/usr/bin/env bash
# Build-stage fixture for the C / C++ borrowed-view runtime E2E (RFC-0033, issue
# 0021 / #0423). Replaces the orphaned+bit-rotted tests/borrowed_{c,cpp}_e2e.sh
# which compiled+linked at TEST time (E1 rule forbids it). Produces the two
# runnable proof binaries; the consuming test (tests/borrowed_e2e.rs) only RUNS
# them:
#   build/borrowed-e2e/borrowed_c_e2e
#   build/borrowed-e2e/borrowed_cpp_e2e
#   build/borrowed-e2e/.compile-ok      (stamp — only after every attempted lang links)
#
# Two rots #0423 documented, fixed here:
#   1. RFC-0042 D1 moved <nros/platform.h> to nros-platform-api — added to -I.
#   2. The `nros_config_variant_sz_<hash>` guard: a standalone `cargo build -p
#      nros-c` can't size the executor (probe → 0), so the archive never defines
#      the anchor the config header imports. But borrowed tests exercise
#      nros_serdes / the CDR readers, NOT the executor's opaque storage, so the
#      guard is a false constraint here. The anchor is emitted WEAK by
#      nros-build-helpers precisely so a consumer may provide its own that merges
#      — so this recipe reads the symbol name out of the freshly-built config
#      header and links a matching `__attribute__((weak)) … = 0;` anchor. Header +
#      archive are one build, so their (stub) sizes agree; the borrowed views read
#      the CDR buffer, unaffected.
#
# A language whose host compiler is absent is skipped (binary not produced; the
# test skips that one). No SDK / cross toolchain needed.
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$script_dir/../.." && pwd)"
# RFC-0070 R1/R3 — cache paths come from the ONE derivation, so
# `NROS_BUILD_ROOT` moves this writer with every other. Default is
# `<repo>/build`, so the emitted path is unchanged.
# shellcheck source=scripts/build/build-root.sh
. "$(dirname "${BASH_SOURCE[0]}")/build-root.sh"
out_dir="$(nros_build_dir "$NROS_KIND_BORROWED_E2E")"

echo "== borrowed-e2e fixture: C / C++ borrowed-view proof binaries =="
rm -rf "$out_dir"
mkdir -p "$out_dir"

# nros-c → libnros_c.a (CDR readers) + the per-build config header, from ONE build
# so the header's variant hash matches the archive's (stub) sizing.
echo "borrowed-e2e: building nros-c (platform-posix)…"
# phase-361 W3 — `std` explicit (host build; nros-c `default = []` now).
#
# `rmw-cffi` is REQUIRED, not decorative — issue 0995. Every `export_size!` in
# `nros::sizes` lives in `mod rmw_sizes`, gated `#[cfg(feature = "rmw-cffi")]`.
# Without it the nested size probe builds an `nros` with NO `__NROS_SIZE_*`
# symbols at all, every size reads 0, and `generate_config` takes its
# "`cargo check --no-default-features` / `cargo doc` — no probe result, skip
# writing" branch. The per-build config header is then never written and this
# fixture fails on the header it is about to link against — which is what the
# build script's own warning means by "do not link the resulting rlib".
( cd "$repo_root" && cargo build -p nros-c --features std,platform-posix,rmw-cffi >/dev/null )
# profile-literal-ok: unprofiled: the line above is a plain `cargo build`, so
# `target/debug/` IS the derived output dir for it.
lib="$repo_root/target/debug/libnros_c.a"
cfg_dir="$repo_root/target/nros-c-generated"
cfg_hdr="$cfg_dir/nros/nros_config_generated.h"
[ -f "$cfg_hdr" ] || { echo "FAIL: nros-c config header missing at $cfg_hdr" >&2; exit 1; }

# The weak variant anchor the config header imports (empty if a probe-less build
# emitted no anchor — then the header carries no extern either, and this is a no-op).
variant_sym="$(grep -oE 'nros_config_variant_sz_[0-9a-fA-F]+' "$cfg_hdr" | head -1 || true)"
anchor_c="$out_dir/nros_variant_anchor.c"
anchor_o="$out_dir/nros_variant_anchor.o"
if [ -n "$variant_sym" ]; then
    printf '/* borrowed-e2e: weak anchor for the config-variant guard (#0423) */\n__attribute__((weak)) const unsigned char %s = 0;\n' \
        "$variant_sym" > "$anchor_c"
    # Compile the anchor as C: in C a file-scope `const` has EXTERNAL linkage, so
    # it can be weak. C++ would give it internal linkage ("weak must be public"),
    # so both links consume the C-compiled object, not the source.
    if command -v gcc >/dev/null 2>&1; then
        gcc -c "$anchor_c" -o "$anchor_o"
    elif command -v cc >/dev/null 2>&1; then
        cc -c "$anchor_c" -o "$anchor_o"
    fi
    echo "borrowed-e2e: variant anchor = $variant_sym"
else
    # Probe-less header carries no extern — compile an empty object so the link
    # lines can reference it unconditionally.
    printf '/* borrowed-e2e: no variant anchor needed */\n' > "$anchor_c"
    { command -v gcc >/dev/null 2>&1 && gcc -c "$anchor_c" -o "$anchor_o"; } \
        || { command -v cc >/dev/null 2>&1 && cc -c "$anchor_c" -o "$anchor_o"; } || true
fi

platform_inc="$repo_root/packages/platform/nros-platform-api/include"

# ---- C ----
if command -v gcc >/dev/null 2>&1; then
    echo "borrowed-e2e: emitting generated C…"
    ( cd "$repo_root/packages/cli" \
        && cargo test -p rosidl-codegen emit_c_borrowed_e2e -- --ignored >/dev/null 2>&1 )
    gen="$repo_root/tmp/borrowed_e2e"
    [ -f "$gen/e2e_msgs_msg_borrowed.h" ] || { echo "FAIL: generated C header missing" >&2; exit 1; }
    driver_c="$repo_root/packages/testing/nros-tests/fixtures/borrowed-c-e2e/driver.c"
    echo "borrowed-e2e: compiling C proof binary…"
    gcc -std=c11 -D_DEFAULT_SOURCE -Wall -DNROS_PLATFORM_POSIX \
        -I "$cfg_dir" -I "$platform_inc" \
        -I "$repo_root/packages/api/nros-c/include" \
        -I "$gen" \
        "$driver_c" "$gen/e2e_msgs_msg_borrowed.c" "$anchor_o" \
        "$lib" -lpthread -ldl -lm \
        -o "$out_dir/borrowed_c_e2e"
    echo "   built $out_dir/borrowed_c_e2e"
else
    echo "borrowed-e2e: gcc absent — skipping C proof binary"
fi

# ---- C++ ----
if command -v g++ >/dev/null 2>&1; then
    echo "borrowed-e2e: emitting generated C++ + FFI glue…"
    ( cd "$repo_root/packages/cli" \
        && cargo test -p rosidl-codegen emit_cpp_borrowed_e2e -- --ignored >/dev/null 2>&1 )
    build="$repo_root/tmp/borrowed_cpp_e2e"
    for f in e2e_msgs_msg_borrowed.hpp e2e_msgs_msg_borrowed_types.rs e2e_msgs_msg_borrowed_exports.rs; do
        [ -f "$build/$f" ] || { echo "FAIL: generated C++ file $f missing" >&2; exit 1; }
    done
    fix="$repo_root/packages/testing/nros-tests/fixtures/borrowed-cpp-e2e"
    cp "$fix/Cargo.toml.in" "$build/Cargo.toml"
    cp "$fix/ffi_wrapper.rs" "$build/lib.rs"
    cp "$fix/driver.cpp" "$build/driver.cpp"
    echo "borrowed-e2e: building C++ FFI staticlib…"
    # This crate is synthesized STANDALONE under tmp/ from `Cargo.toml.in`,
    # outside the root workspace, so nano-ros's custom profiles
    # (`nros-relwithdebinfo`, …) are not defined for it: asking the resolver
    # here yields `--profile nros-…` and cargo errors "profile is not defined".
    # It is a throwaway link-proof, not a shipped artifact.
    # profile-literal-ok: unprofiled
    ( cd "$build" && cargo build --release >/dev/null )
    # profile-literal-ok: unprofiled: pairs with the standalone `cargo build
    # --release` above — same reason, and the two must name the same dir.
    cpp_lib="$build/target/release/libborrowed_cpp_e2e.a"
    [ -f "$cpp_lib" ] || { echo "FAIL: C++ FFI staticlib missing" >&2; exit 1; }
    echo "borrowed-e2e: compiling C++ proof binary…"
    g++ -std=c++14 -D_DEFAULT_SOURCE -DNROS_PLATFORM_POSIX -Wall \
        -I "$platform_inc" \
        -I "$repo_root/packages/api/nros-cpp/include" \
        -I "$repo_root/packages/api/nros-c/include" \
        -I "$cfg_dir" -I "$build" \
        "$build/driver.cpp" "$anchor_o" "$cpp_lib" -lpthread -ldl -lm \
        -o "$out_dir/borrowed_cpp_e2e"
    echo "   built $out_dir/borrowed_cpp_e2e"
else
    echo "borrowed-e2e: g++ absent — skipping C++ proof binary"
fi

date -u +%Y-%m-%dT%H:%M:%SZ > "$out_dir/.compile-ok"
echo "borrowed-e2e: done"
