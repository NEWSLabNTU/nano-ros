#!/bin/sh
# prune-zephyr-sdk.sh <sdk-base> <subdir> <keep-toolchain>...
#
# Remove every cross toolchain of one unpacked Zephyr SDK except the ones
# named, so the zephyr-ci image carries only what the index's `post_install`
# asks for (phase-559 W5b: the zephyr tier-1 hosted shard's lowest free disk
# was 4.0 G, and the image's unpacked size comes out of that same disk).
#
# Runs at `docker build` time only. Guarded so a wrong argument can never
# delete outside the SDK:
#   - <sdk-base>, <subdir> and every keep name must be non-empty; <subdir>
#     is one path component (no `/`, not `.` or `..`);
#   - the SDK root must resolve (realpath) to exactly <sdk-base>/<subdir>,
#     i.e. it is not a symlink to somewhere else;
#   - candidates are depth-1 directories of the root and of `<root>/gnu`
#     only, named `*zephyr-elf` or `*zephyr-eabi`, found without following
#     symlinks (`find -P`, `-type d`), and a symlink candidate is refused;
#   - each candidate must resolve to a direct child of the root or of
#     `<root>/gnu`, else the script fails before deleting anything more.
# It prints the keep list and the SDK's `du -sh` before and after.
set -eu

die() { echo "prune-zephyr-sdk: $*" >&2; exit 1; }

[ "$#" -ge 3 ] || die "usage: prune-zephyr-sdk.sh <sdk-base> <subdir> <keep-toolchain>..."
base="$1"; subdir="$2"; shift 2
[ -n "$base" ] || die "empty SDK base"
[ -n "$subdir" ] || die "empty SDK subdir"
case "$subdir" in
    */* | . | ..) die "SDK subdir must be one path component, got '$subdir'" ;;
esac
for k in "$@"; do
    [ -n "$k" ] || die "empty toolchain name in the keep list"
done

base_real="$(realpath -e -- "$base")" || die "SDK base '$base' does not exist"
root="$base_real/$subdir"
[ -d "$root" ] && [ ! -L "$root" ] || die "'$root' is not a directory (or is a symlink)"
root_real="$(realpath -e -- "$root")"
[ "$root_real" = "$root" ] || die "'$root' resolves to '$root_real', outside the SDK base"

echo "prune-zephyr-sdk: $root keeps toolchains: $*"
echo "prune-zephyr-sdk: before: $(du -sh -- "$root" | cut -f1)"

parents="$root"
[ -d "$root/gnu" ] && [ ! -L "$root/gnu" ] && parents="$parents $root/gnu"
for parent in $parents; do
    find -P "$parent" -mindepth 1 -maxdepth 1 -type d \
        \( -name '*zephyr-elf' -o -name '*zephyr-eabi' \) -print
done | while IFS= read -r tc; do
    [ -n "$tc" ] || die "empty candidate path"
    name="$(basename -- "$tc")"
    keep=0
    for k in "$@"; do [ "$name" = "$k" ] && keep=1; done
    [ "$keep" = 1 ] && continue
    [ ! -L "$tc" ] || die "refusing symlink '$tc'"
    real="$(realpath -e -- "$tc")" || die "cannot resolve '$tc'"
    case "$(dirname -- "$real")" in
        "$root" | "$root/gnu") ;;
        *) die "'$tc' resolves to '$real', not a direct child of '$root' or '$root/gnu'" ;;
    esac
    echo "prune-zephyr-sdk: removing $real"
    rm -rf -- "$real"
done

echo "prune-zephyr-sdk: after: $(du -sh -- "$root" | cut -f1)"
