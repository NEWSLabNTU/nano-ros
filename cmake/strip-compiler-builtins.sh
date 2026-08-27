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

# issue 0805 — skip an archive we have already processed and that has not
# changed since.
#
# This script runs from the LINK WRAPPER, so it is invoked once per archive per
# link: ~190 times in one warm `threadx_riscv64` rebuild. Each invocation
# extracts EVERY member (`llvm-ar p` per object) and runs a reader on it, then
# makes six `llvm-objcopy` passes — measured at 4.3 s on a 1.6 MB archive, and
# NOT faster the second time, because nothing recorded that the work was already
# done. That is ~817 s of work in a build whose own compile step is 6.7 s, and
# it is what the leaves were blocked on: `llvm-ar` and `llvm-objcopy` in
# `rq_qos_wait`, i.e. block-layer writeback throttling, 36% of leaf samples in D.
#
# The stamp records size+mtime of the archive AS THIS SCRIPT LEFT IT. A rebuilt
# archive gets a new mtime and is reprocessed; an untouched one is skipped. This
# composes with the mtime restore at the bottom: after a no-op the archive keeps
# its original mtime, so the stamp written here matches on the next run.
STAMP="$ARCHIVE.nros-strip-stamp"
_archive_id() {
    stat -c '%s %Y %y' "$ARCHIVE" 2>/dev/null
}
if [ -f "$STAMP" ] && [ "$(cat "$STAMP" 2>/dev/null)" = "$(_archive_id)" ]; then
    exit 0
fi

# Snapshot original to detect no-op runs and preserve mtime — otherwise every
# rebuild bumps the archive mtime and cmake relinks downstream targets.
SNAPSHOT=$(mktemp)
cp -p "$ARCHIVE" "$SNAPSHOT"

TMPDIR=$(mktemp -d)
trap 'rm -rf "$TMPDIR" "$SNAPSHOT"' EXIT

# issue 0657 — the READER, resolved. This asked `riscv64-unknown-elf-readelf`
# by name: Ubuntu's package, not the xPack `riscv-none-elf-*` that
# `nros setup` provisions for this board. On a provisioned host the command
# does not exist, `flags` comes back EMPTY, nothing matches "soft-float", and
# the loop strips zero objects — silently, because the probe's stderr goes to
# /dev/null and an empty result is indistinguishable from "no soft-float here".
#
# That is why the C/C++ riscv64 link failed on `bswapsi2.o` with "cannot link
# object files with different floating-point ABI" even though this exact
# workaround was running on every archive: the tool that decides what to strip
# was missing, so it decided nothing.
#
# `llvm-readobj` ships beside the `llvm-ar` this script is already handed, so
# prefer it and fall back to whichever cross readelf exists.
# Prints `soft-float`, `hard-float`, or `STRIP_NO_READER`.
#
# The two readers state the same fact differently and only one states it in
# words. GNU readelf decodes the e_flags into "soft-float ABI"; llvm-readobj
# prints the RAW value (`Flags [ (0x1)`) and names only the bits it has names
# for — soft-float is the ABSENCE of the float bits, so there is nothing to
# grep for. Decode instead: e_flags & 0x6 is the float ABI field (0 = soft,
# 2 = single, 4 = double, 6 = quad).
_riscv_float_abi() {
    local obj="$1"
    local candidate
    for candidate in riscv-none-elf-readelf riscv64-unknown-elf-readelf riscv64-none-elf-readelf; do
        if command -v "$candidate" >/dev/null 2>&1; then
            if "$candidate" -h "$obj" 2>/dev/null | grep -q 'soft-float'; then
                echo soft-float
            else
                echo hard-float
            fi
            return
        fi
    done
    local llvm_readobj="$(dirname "$LLVM_AR")/llvm-readobj"
    if [ -x "$llvm_readobj" ]; then
        local raw
        raw=$("$llvm_readobj" --file-headers "$obj" 2>/dev/null \
              | sed -n 's/.*Flags \[ (\(0x[0-9a-fA-F]*\)).*/\1/p' | head -1)
        if [ -n "$raw" ]; then
            if [ $(( raw & 0x6 )) -eq 0 ]; then echo soft-float; else echo hard-float; fi
            return
        fi
    fi
    echo "STRIP_NO_READER"
}

count=0
no_reader=0
for obj in $("$LLVM_AR" t "$ARCHIVE"); do
    "$LLVM_AR" p "$ARCHIVE" "$obj" > "$TMPDIR/$obj" 2>/dev/null || continue
    # Check if this object has soft-float ABI (flag 0x0000 or RVC-only 0x0001).
    # llvm-readobj spells it `EF_RISCV_FLOAT_ABI_SOFT`; GNU readelf spells it
    # `soft-float`. Match either — the two readers word the same fact
    # differently, and keying on one spelling is how this broke.
    flags=$(_riscv_float_abi "$TMPDIR/$obj")
    if [ "$flags" = "STRIP_NO_READER" ]; then
        no_reader=1
        break
    fi
    if [ "$flags" = "soft-float" ]; then
        "$LLVM_AR" d "$ARCHIVE" "$obj" 2>/dev/null
        count=$((count + 1))
    fi
done

if [ "$no_reader" -eq 1 ]; then
    echo "$0: no ELF reader for riscv64 objects (looked for llvm-readobj beside" >&2
    echo "  $LLVM_AR, then riscv-none-elf-readelf / riscv64-unknown-elf-readelf)." >&2
    echo "  Cannot tell soft-float objects from hard-float ones, so nothing was" >&2
    echo "  stripped and the link will fail on a float-ABI mismatch (issue 0657)." >&2
    exit 1
fi

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

# Record the archive's identity AFTER the mtime restore above, so the next
# invocation on an unchanged archive can skip (issue 0805). Written last: if any
# step above failed, `set -e` has already exited and no stamp claims work that
# did not happen.
_archive_id > "$STAMP" 2>/dev/null || true
