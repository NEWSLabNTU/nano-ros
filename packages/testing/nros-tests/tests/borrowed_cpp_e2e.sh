#!/usr/bin/env bash
# Phase 235.8 — C++ borrowed (RFC-0033, issue 0021) runtime E2E.
#
# Compiles the generated C++ FFI glue into a staticlib (the real Rust
# nros_cpp_deserialize_*_borrowed) and links a C++ driver that serializes an
# owned message then deserialize_borrowed's it, asserting every borrowed view
# (nros::Span / StringView / LeSpan) points INTO the CDR buffer with correct
# values. Proves the FFI-offset seam + the repr(C) layout match between the Rust
# {Msg}ViewRepr and the C++ {Msg}View, end-to-end.
#
# Skips cleanly (exit 0) if g++ is unavailable.

set -euo pipefail

cd "$(git rev-parse --show-toplevel)"
ROOT="$(pwd)"

if ! command -v g++ >/dev/null 2>&1; then
    echo "[SKIP] borrowed_cpp_e2e: g++ not found"
    exit 0
fi

# 1) Build nros-c → the per-build config header (pulled by the C++ heap/platform
#    headers). libnros_c.a itself isn't linked — the FFI lives in our staticlib.
echo "borrowed_cpp_e2e: building nros-c (config header)…"
cargo build -p nros-c >/dev/null
CFG_DIR="$ROOT/target/nros-c-generated"
test -f "$CFG_DIR/nros/nros_config_generated.h" || { echo "FAIL: config header missing"; exit 1; }

# 2) Emit the generated borrowed C++ header + Rust FFI glue into the build dir.
echo "borrowed_cpp_e2e: emitting generated code…"
( cd packages/cli && cargo test -p rosidl-codegen emit_cpp_borrowed_e2e -- --ignored >/dev/null 2>&1 )
BUILD="$ROOT/tmp/borrowed_cpp_e2e"
test -f "$BUILD/e2e_msgs_msg_borrowed.hpp" || { echo "FAIL: generated hpp missing"; exit 1; }
test -f "$BUILD/e2e_msgs_msg_borrowed_ffi.rs" || { echo "FAIL: generated ffi.rs missing"; exit 1; }

# 3) Drop the staticlib-crate scaffolding + driver next to the emitted ffi.rs.
FIX="$ROOT/packages/testing/nros-tests/fixtures/borrowed-cpp-e2e"
cp "$FIX/Cargo.toml.in" "$BUILD/Cargo.toml"
cp "$FIX/ffi_wrapper.rs" "$BUILD/lib.rs"
cp "$FIX/driver.cpp" "$BUILD/driver.cpp"

# 4) Build the FFI staticlib (compiles the generated ffi.rs).
echo "borrowed_cpp_e2e: building FFI staticlib…"
( cd "$BUILD" && cargo build --release >/dev/null )
LIB="$BUILD/target/release/libborrowed_cpp_e2e.a"
test -f "$LIB" || { echo "FAIL: staticlib missing"; exit 1; }

# 5) Compile + link the C++ driver, run.
echo "borrowed_cpp_e2e: compiling + running driver…"
g++ -std=c++14 -D_DEFAULT_SOURCE -DNROS_PLATFORM_POSIX -Wall \
    -I "$ROOT/packages/core/nros-cpp/include" \
    -I "$ROOT/packages/core/nros-c/include" \
    -I "$CFG_DIR" \
    -I "$BUILD" \
    "$BUILD/driver.cpp" "$LIB" -lpthread -ldl -lm \
    -o "$BUILD/borrowed_cpp_e2e"

"$BUILD/borrowed_cpp_e2e"
echo "borrowed_cpp_e2e: PASS"
