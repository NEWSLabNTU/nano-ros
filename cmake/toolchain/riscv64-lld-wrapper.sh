#!/bin/bash
# Wrapper around rust-lld that strips soft-float compiler_builtins
# from all .a archives before linking. Workaround for:
# https://github.com/rust-lang/rust/issues/83229
#
# The wrapper finds all .a arguments, strips objects with soft-float ABI
# (16-hex-char hash prefix from compiler_builtins), then calls rust-lld.

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
RUST_LLD="$SCRIPT_DIR/_real_lld"
LLVM_AR="$SCRIPT_DIR/_llvm_ar"
STRIP_SCRIPT="$SCRIPT_DIR/../../cmake/strip-compiler-builtins.sh"

# Strip soft-float compiler_builtins AND Rust mem functions from .a files.
# Rust's compiler_builtins provides memset/memcpy/memmove but they can be
# buggy on RISC-V (recursive implementation). picolibc provides correct ones.
for arg in "$@"; do
    if [[ "$arg" == *.a ]] && [ -f "$arg" ]; then
        bash "$STRIP_SCRIPT" "$LLVM_AR" "$arg" 2>/dev/null
        # Also remove Rust compiler_builtins mem functions (they have weak linkage
        # but lld picks them over picolibc due to archive processing order)
        for sym in memset memcpy memmove memcmp bcmp strlen; do
            obj=$("$LLVM_AR" t "$arg" 2>/dev/null | grep "compiler_builtins.*mem\|compiler_builtins.*$sym" | head -1)
            if [ -n "$obj" ]; then
                "$LLVM_AR" d "$arg" "$obj" 2>/dev/null || true
            fi
        done
    fi
done

exec "$RUST_LLD" "$@"
