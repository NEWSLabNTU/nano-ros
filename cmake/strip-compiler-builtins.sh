#!/bin/bash
# Strip soft-float compiler_builtins objects from a Rust staticlib.
#
# Rust's compiler_builtins for riscv64gc uses soft-float ABI, which
# conflicts with lp64d hard-float objects. This script removes only
# the objects that have soft-float ABI, preserving all hard-float objects.
#
# Usage: strip-compiler-builtins.sh <llvm-ar> <archive>

set -e
LLVM_AR="$1"
ARCHIVE="$2"

if [ ! -f "$ARCHIVE" ]; then
    echo "Archive not found: $ARCHIVE"
    exit 1
fi

TMPDIR=$(mktemp -d)
trap "rm -rf $TMPDIR" EXIT

count=0
for obj in $("$LLVM_AR" t "$ARCHIVE"); do
    "$LLVM_AR" p "$ARCHIVE" "$obj" > "$TMPDIR/$obj" 2>/dev/null || continue
    # Check if this object has soft-float ABI (flag 0x0000 or RVC-only 0x0001)
    flags=$(riscv64-unknown-elf-readelf -h "$TMPDIR/$obj" 2>/dev/null | grep 'Flags:' | head -1)
    if echo "$flags" | grep -q 'soft-float'; then
        "$LLVM_AR" d "$ARCHIVE" "$obj" 2>/dev/null
        count=$((count + 1))
    fi
done

if [ $count -gt 0 ]; then
    echo "Stripped $count soft-float compiler_builtins objects from $(basename "$ARCHIVE")"
fi
