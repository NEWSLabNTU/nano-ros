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

# System-installed libs (e.g. /usr/lib/gcc/.../libgcc.a) are read-only and
# don't need stripping anyway — skip silently.
if [ ! -w "$ARCHIVE" ]; then
    exit 0
fi

# Snapshot original to detect no-op runs and preserve mtime — otherwise every
# rebuild bumps the archive mtime and cmake relinks downstream targets.
SNAPSHOT=$(mktemp)
cp -p "$ARCHIVE" "$SNAPSHOT"

TMPDIR=$(mktemp -d)
trap 'rm -rf "$TMPDIR" "$SNAPSHOT"' EXIT

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

# Localize Rust's weak memset/memcpy/memmove symbols so they don't override
# picolibc's implementations. Rust's compiler_builtins memset can crash on
# RISC-V due to recursive implementation + QEMU interaction issues.
LLVM_OBJCOPY="$(dirname "$LLVM_AR")/llvm-objcopy"
if [ -x "$LLVM_OBJCOPY" ]; then
    localized=0
    for sym in memset memcpy memmove memcmp bcmp strlen; do
        "$LLVM_OBJCOPY" --localize-symbol="$sym" "$ARCHIVE" 2>/dev/null && localized=$((localized + 1)) || true
    done
    if [ $localized -gt 0 ]; then
        echo "Localized $localized mem symbols in $(basename "$ARCHIVE")"
    fi
fi

# Restore mtime if the archive ended up byte-identical to the snapshot. Makes
# the script idempotent under cmake's mtime-driven dep tracking, so a no-op
# rerun no longer triggers downstream relinks.
if cmp -s "$ARCHIVE" "$SNAPSHOT"; then
    touch -r "$SNAPSHOT" "$ARCHIVE"
fi
