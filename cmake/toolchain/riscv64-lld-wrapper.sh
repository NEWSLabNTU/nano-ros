#!/bin/bash
# Wrapper around rust-lld that strips soft-float compiler_builtins
# from all .a archives before linking. Workaround for:
# https://github.com/rust-lang/rust/issues/83229
#
# The wrapper finds all .a arguments, strips objects with soft-float ABI
# (16-hex-char hash prefix from compiler_builtins), then calls rust-lld.
#
# Tool paths come from the environment (NROS_RUST_LLD / NROS_LLVM_AR),
# set by the riscv64-threadx cmake toolchain. Earlier revisions resolved
# them as siblings of this script via SCRIPT_DIR/_real_lld /
# SCRIPT_DIR/_llvm_ar — those symlinks lived in the in-source toolchain
# directory and raced when two cmake configures ran concurrently against
# different build dirs.

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
RUST_LLD="${NROS_RUST_LLD}"
LLVM_AR="${NROS_LLVM_AR}"
STRIP_SCRIPT="$SCRIPT_DIR/../../cmake/strip-compiler-builtins.sh"

if [ -z "$RUST_LLD" ]; then
    echo "$0: NROS_RUST_LLD not set in environment" >&2
    exit 1
fi
if [ -z "$LLVM_AR" ]; then
    echo "$0: NROS_LLVM_AR not set in environment" >&2
    exit 1
fi

# Strip soft-float compiler_builtins AND Rust mem functions from .a files.
# Rust's compiler_builtins provides memset/memcpy/memmove but they can be
# buggy on RISC-V (recursive implementation). picolibc provides correct ones.
for arg in "$@"; do
    if [[ "$arg" == *.a ]] && [ -f "$arg" ] && [ -w "$arg" ]; then
        bash "$STRIP_SCRIPT" "$LLVM_AR" "$arg" 2>/dev/null
        # Also remove Rust compiler_builtins mem functions (they have weak
        # linkage but lld picks them over picolibc due to archive processing
        # order). Snapshot first / restore mtime if no change so a no-op rerun
        # doesn't bump the archive mtime and trigger downstream relinks.
        snap=$(mktemp)
        cp -p "$arg" "$snap"
        for sym in memset memcpy memmove memcmp bcmp strlen; do
            obj=$("$LLVM_AR" t "$arg" 2>/dev/null | grep "compiler_builtins.*mem\|compiler_builtins.*$sym" | head -1)
            if [ -n "$obj" ]; then
                "$LLVM_AR" d "$arg" "$obj" 2>/dev/null || true
            fi
        done
        if cmp -s "$arg" "$snap"; then
            touch -r "$snap" "$arg" 2>/dev/null || true
        fi
        rm -f "$snap"
    fi
done

exec "$RUST_LLD" "$@"
