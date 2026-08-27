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
        # issue 0850 — MEMOISE the whole per-archive pipeline on the archive's
        # CONTENT, and restore by copy on a hit.
        #
        # Everything below rewrites the archive in place, so its own output is
        # never a valid cache key: Corrosion re-copies the UNLOCALIZED
        # `libnros_cpp.a` into each leaf every build, which is why issue 0805's
        # size+mtime stamp inside the strip script cannot hold — the copy resets
        # both. Content survives that, because `copy_if_different` writes the
        # same bytes each time.
        #
        # Measured cost of NOT doing this: leaf state 85% D (uninterruptible
        # disk wait) with `llvm-ar rq_qos_wait` the dominant blocker by an order
        # of magnitude, on a warm rebuild that compiles nothing. The work per
        # archive is small alone (~0.07 s for the mem-symbol pass, ~4.3 s for a
        # full strip) — it is doing it ~260 times across 29 leaves, concurrently,
        # that saturates the disk queue.
        memo_sha="$arg.nros-linkmemo.sha"
        memo_out="$arg.nros-linkmemo.out"
        in_hash="$(sha256sum "$arg" 2>/dev/null | cut -d' ' -f1)"
        if [ -n "$in_hash" ] && [ -f "$memo_out" ] \
           && [ "$(cat "$memo_sha" 2>/dev/null)" = "$in_hash" ]; then
            # Same input as last time — restore the known result instead of
            # recomputing it. `-p` keeps the mtime stable so downstream targets
            # do not relink.
            cp -p "$memo_out" "$arg"
            continue
        fi

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

        # Record the result against the INPUT hash. Written last, so a failure
        # above leaves no memo claiming work that did not happen.
        if [ -n "$in_hash" ]; then
            cp -p "$arg" "$memo_out" 2>/dev/null \
                && printf '%s\n' "$in_hash" > "$memo_sha" 2>/dev/null || true
        fi
    fi
done

lld_args=()
for arg in "$@"; do
    case "$arg" in
        -Wl,*)
            IFS=',' read -ra parts <<< "${arg#-Wl,}"
            for part in "${parts[@]}"; do
                lld_args+=("$part")
            done
            ;;
        # Phase 155.E — gcc-driver-only flags that lld doesn't
        # understand. Cmake board overlays pass these via
        # `target_link_options` for the gcc-as-linker case; the
        # bare flags reach lld too. Strip them — lld's "no
        # startup files / no default libs" behaviour is the
        # default, and the explicit `-T<script>` + `--gc-sections`
        # already cover the actual link controls.
        -nostartfiles|-nostdlib|-nodefaultlibs|-no-pie|-pie|--specs=*)
            ;;
        # `--allow-multiple-definition` is an ld/lld flag too;
        # pass it through. `--nmagic` likewise.
        *)
            lld_args+=("$arg")
            ;;
    esac
done

exec "$RUST_LLD" "${lld_args[@]}"
