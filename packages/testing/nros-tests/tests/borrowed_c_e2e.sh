#!/usr/bin/env bash
# Phase 235.4 — C borrowed (RFC-0033, issue 0021) runtime E2E.
#
# Generates a message with `mode = "borrowed"` string/byte/numeric fields,
# compiles the generated C + a driver against libnros_c.a, and runs it: the
# driver owned-serializes a message then deserialize_borrowed's it and asserts
# every borrowed view points INTO the CDR buffer (zero-copy) with correct
# values. Proves the full C path: generated serialize + deserialize_borrowed +
# nros/borrowed.h helpers + the nros-c CDR readers.
#
# Skips cleanly (exit 0) if gcc is unavailable.

set -euo pipefail

cd "$(git rev-parse --show-toplevel)"
ROOT="$(pwd)"

if ! command -v gcc >/dev/null 2>&1; then
    echo "[SKIP] borrowed_c_e2e: gcc not found"
    exit 0
fi

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

# 1) Build nros-c → libnros_c.a (CDR readers) + the per-build config header.
echo "borrowed_c_e2e: building nros-c…"
cargo build -p nros-c >/dev/null
LIB="$ROOT/target/debug/libnros_c.a"
CFG_DIR="$ROOT/target/nros-c-generated"
test -f "$LIB" || { echo "FAIL: $LIB missing"; exit 1; }
test -f "$CFG_DIR/nros/nros_config_generated.h" || { echo "FAIL: config header missing"; exit 1; }

# 2) Emit the generated borrowed C (header + source) into $ROOT/tmp/borrowed_e2e.
echo "borrowed_c_e2e: emitting generated code…"
( cd packages/cli && cargo test -p rosidl-codegen emit_c_borrowed_e2e -- --ignored >/dev/null 2>&1 )
GEN="$ROOT/tmp/borrowed_e2e"
test -f "$GEN/e2e_msgs_msg_borrowed.h" || { echo "FAIL: generated header missing"; exit 1; }

# 3) Compile driver + generated source, link nros-c, run.
DRIVER="$ROOT/packages/testing/nros-tests/fixtures/borrowed-c-e2e/driver.c"
echo "borrowed_c_e2e: compiling + running…"
gcc -std=c11 -D_DEFAULT_SOURCE -Wall -DNROS_PLATFORM_POSIX \
    -I "$CFG_DIR" \
    -I "$ROOT/packages/core/nros-c/include" \
    -I "$GEN" \
    "$DRIVER" "$GEN/e2e_msgs_msg_borrowed.c" \
    "$LIB" -lpthread -ldl -lm \
    -o "$WORK/borrowed_e2e"

"$WORK/borrowed_e2e"
echo "borrowed_c_e2e: PASS"
