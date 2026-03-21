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

# Strip compiler_builtins from .a files in the argument list
for arg in "$@"; do
    if [[ "$arg" == *.a ]] && [ -f "$arg" ]; then
        bash "$STRIP_SCRIPT" "$LLVM_AR" "$arg" 2>/dev/null
    fi
done

exec "$RUST_LLD" "$@"
