#!/usr/bin/env bash
# Issue 0583 — drop the NuttX `-Z build-std` artifacts when the vendored libc
# fork moves.
#
# WHY A WIPE AND NOT A FINGERPRINT. Every NuttX Rust row builds `std` from source
# against the fork at `third-party/nuttx/libc`, whose `__PTHREAD_ATTR_SIZE__` (and
# siblings) mirror NuttX's opaque structs BY SIZE. That size is compiled into
# `std::sys::thread::unix::Thread::new`, which puts a `pthread_attr_t` on its own
# stack frame and hands its address to `pthread_attr_init` / `pthread_create` /
# `pthread_attr_destroy` — kernel functions that memset the KERNEL's size into it
# whatever Rust reserved. Undersize the mirror and every thread spawn writes past
# the attr into the caller's saved registers.
#
# Issue 0570 fixed the constant (5 -> 14, i.e. 20 -> 56 bytes). Issue 0583 is what
# happened next: the leaf target dirs still held a `std` compiled days earlier
# against the 20-byte mirror, and nothing rebuilt it. The boot task's spawn
# returned to ~0, silently. The observable was a tier that "stopped scheduling".
#
# `workspace-fixture-signature.sh` now hashes the pin, so the STAMP is honest —
# but a stale stamp only forces `cargo build` to run again, and that reuses the
# build-std units it already has. The artifacts have to go.
#
# Scope: NuttX Rust artifact roots only, and only when the pin actually changed.
# A first run with no stamp records the pin WITHOUT wiping — the artifacts on
# disk may be perfectly good, and a gratuitous full rebuild of every NuttX row is
# expensive enough to be its own problem.
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$script_dir/../.." && pwd)"

libc_dir="$repo_root/third-party/nuttx/libc"
[ -d "$libc_dir" ] || exit 0

# The submodule commit is the pin. A non-git checkout (tarball, vendored copy)
# falls back to hashing the file that carries the sizes rather than to "assume
# unchanged" — the failure this guards is silent, so the fallback must not be.
if pin="$(git -C "$libc_dir" rev-parse HEAD 2>/dev/null)" && [ -n "$pin" ]; then
    pin="commit:$pin"
elif [ -f "$libc_dir/src/unix/nuttx/mod.rs" ]; then
    pin="content:$(sha256sum "$libc_dir/src/unix/nuttx/mod.rs" | awk '{print $1}')"
else
    echo "nuttx-libc-pin-guard: cannot pin $libc_dir — refusing to guess" >&2
    exit 2
fi

stamp="$repo_root/build/nuttx-libc-pin"
mkdir -p "$(dirname "$stamp")"
previous="$(cat "$stamp" 2>/dev/null || true)"

if [ "$previous" = "$pin" ]; then
    exit 0
fi

if [ -n "$previous" ]; then
    echo "nuttx-libc-pin-guard: vendored NuttX libc moved"
    echo "    was: $previous"
    echo "    now: $pin"
    echo "  dropping the NuttX build-std artifacts (issue 0583: a stale \`std\`"
    echo "  carries the OLD opaque-struct sizes and smashes the caller's frame"
    echo "  on every thread spawn)."

    # Workspace rows: `<dir>/<target_dir>` from the manifest, so this follows the
    # SSOT rather than a glob that would rot the next time a row moves.
    while IFS=$'\x1f' read -r _id _lang dir _bringup _entry _build_subdir target_dir _rest; do
        [ -n "${dir:-}" ] && [ -n "${target_dir:-}" ] || continue
        victim="$repo_root/$dir/$target_dir"
        if [ -d "$victim" ]; then
            echo "    rm -rf $dir/$target_dir"
            rm -rf "$victim"
        fi
    done < <(python3 "$repo_root/scripts/build/fixtures-manifest.py" list-workspaces \
                 --platform nuttx --lang rust 2>/dev/null
             python3 "$repo_root/scripts/build/fixtures-manifest.py" list-workspaces \
                 --platform nuttx-riscv --lang rust 2>/dev/null)

    # Plain `[[fixture]]` cargo rows. Their artifact roots come from
    # `fixture-groups` (phase-340 B2 — field 1 is the leaf artifact root), but
    # THAT SUBCOMMAND IGNORES --platform/--lang: it prints every row. Asking it
    # for "nuttx rows" returns the native ones too, and this script deletes what
    # it is handed — caught here by deleting four native `target/` dirs.
    #
    # So the row set comes from `list`, which does filter, and `fixture-groups`
    # is used only to look UP the artifact root of a row we already know is
    # NuttX. Intersection, not trust.
    nuttx_leaves="$(
        python3 "$repo_root/scripts/build/fixtures-manifest.py" list \
            --platform nuttx --lang rust 2>/dev/null
        python3 "$repo_root/scripts/build/fixtures-manifest.py" list \
            --platform nuttx-riscv --lang rust 2>/dev/null
    )" || nuttx_leaves=""

    while IFS=$'\x1f' read -r artifact_root _rest; do
        [ -n "${artifact_root:-}" ] || continue
        # Keep only artifact roots belonging to a leaf the FILTERED list named.
        leaf_ok=0
        while IFS=$'\x1f' read -r leaf_dir _leaf_rest; do
            [ -n "${leaf_dir:-}" ] || continue
            case "$artifact_root" in "$leaf_dir"/*|"$leaf_dir") leaf_ok=1; break ;; esac
        done <<< "$nuttx_leaves"
        [ "$leaf_ok" = 1 ] || continue

        victim="$repo_root/$artifact_root"
        [ -d "$victim" ] || continue
        echo "    rm -rf $artifact_root"
        rm -rf "$victim"
    done < <(python3 "$repo_root/scripts/build/fixtures-manifest.py" fixture-groups 2>/dev/null)
else
    echo "nuttx-libc-pin-guard: recording the vendored NuttX libc pin ($pin);"
    echo "  no wipe on a first run — nothing on disk is known to disagree with it."
fi

printf '%s' "$pin" > "$stamp"
