#!/usr/bin/env bash
# check-out-of-tree-board-images: a board package kept OUTSIDE nano-ros builds
# hard-float FreeRTOS images in C, C++ and Rust with no nano-ros source patched
# (phase-575 W2, phase-577 W8, RFC-0135 D2).
#
# The fixture is packages/testing/nros-tests/fixtures/out-of-tree-r5f/: the
# board package `fixture-r5f-freertos` and its vendor SDK stand-in. Both are
# COPIED to a scratch directory first, the way a user's repository sits apart
# from nano-ros, and reached only through what a user has:
#   NROS_EXTRA_BOARD_PATH  -> the copied boards/ directory
#   FIXTURE_R5F_SDK_ROOT   -> the copied SDK root (the package's own variable)
# Then `nros build fixture-r5f-freertos` runs in each of the package's three
# examples, and every image ELF must say
#   Tag_ABI_VFP_args: VFP registers   (floats cross calls in VFP registers)
#   Tag_FP_arch: VFPv3-D16            (the R5F's FPU)
# Selftest, every run: the same attribute check is handed a SOFT-float object
# (the fixture's C compiled -mfloat-abi=softfp) and must refuse it, so a check
# that read the wrong file or an empty attribute set could not pass.
#
# Build tier: three cross builds (zenoh-pico, lwIP, FreeRTOS, a Rust entry),
# several minutes cold. Needs an arm-none-eabi-gcc, cmake and the
# `armv7r-none-eabihf` rust-std.
set -euo pipefail
repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
fixture="$repo/packages/testing/nros-tests/fixtures/out-of-tree-r5f"
mkdir -p "$repo/tmp"
work="$(mktemp -d "$repo/tmp/out-of-tree-board.XXXXXX")"
trap 'rm -rf "$work"' EXIT
say() { echo "check-out-of-tree-board-images: $*"; }
# shellcheck source=../build/cargo.sh
. "$repo/scripts/build/cargo.sh"
if ! nros="$(nros_cli_bin)" || [ -z "$nros" ]; then
    say "no nros CLI (just setup-cli, or source ./activate.sh)"; exit 1
fi
readelf="$(command -v arm-none-eabi-readelf || true)"
if [ -z "$readelf" ]; then
    gcc="$(command -v arm-none-eabi-gcc || true)"
    [ -n "$gcc" ] || { say "no arm-none-eabi-gcc on PATH"; exit 1; }
    readelf="$(dirname "$gcc")/arm-none-eabi-readelf"
fi

# The two attributes an image must carry; prints what is wrong, if anything.
abi_problem() { # <elf>
    local a vfp fp
    a="$("$readelf" -A "$1" 2>/dev/null)"
    vfp="$(sed -n 's/.*Tag_ABI_VFP_args: *//p' <<<"$a" | sort -u)"
    fp="$(sed -n 's/.*Tag_FP_arch: *//p' <<<"$a" | sort -u)"
    [ "$vfp" = "VFP registers" ] || { echo "Tag_ABI_VFP_args is '${vfp:-absent}', want 'VFP registers'"; return; }
    [ "$fp" = "VFPv3-D16" ] || { echo "Tag_FP_arch is '${fp:-absent}', want 'VFPv3-D16'"; return; }
}

cp -r "$fixture/boards" "$fixture/sdk" "$work/"
# A copied example must not carry the source tree's build output.
for ex in "$work"/boards/nros-board-fixture-r5f-freertos/examples/*-talker; do
    rm -rf "${ex:?}/build" "${ex:?}/generated" "${ex:?}/.cargo"
done
export NROS_EXTRA_BOARD_PATH="$work/boards"
export FIXTURE_R5F_SDK_ROOT="$work/sdk"

fail=0
for lang in c cpp rust; do
    ex="$work/boards/nros-board-fixture-r5f-freertos/examples/$lang-talker"
    if ! (cd "$ex" && "$nros" build fixture-r5f-freertos) > "$work/$lang.log" 2>&1; then
        say "$lang: \`nros build fixture-r5f-freertos\` failed:"
        grep -E -i '(^|[[:space:]])error|FATAL|undefined' "$work/$lang.log" | head -12 | sed 's/^/    /'
        fail=1
        continue
    fi
    if ! elf="$(find "$ex/build" -type f -name fixture_r5f_freertos_entry ! -path '*/deps/*' | head -1)"; then
        elf=""
    fi
    if [ -z "$elf" ]; then
        say "$lang: built, but no image ELF named fixture_r5f_freertos_entry under $ex/build"
        fail=1
        continue
    fi
    why="$(abi_problem "$elf")"
    if [ -n "$why" ]; then
        say "$lang: $why ($elf)"
        fail=1
    fi
done

selftest() {
    local cc="${readelf%readelf}gcc" soft="$work/soft.o" why
    "$cc" -mcpu=cortex-r5 -mfpu=vfpv3-d16 -mfloat-abi=softfp -c -o "$soft" \
        "$repo/packages/testing/nros-tests/fixtures/out-of-tree-r5f/sdk/src/fxsdk.c" \
        -I"$fixture/sdk/include" 2> "$work/soft.log" \
        || { say "selftest: the softfp object did not compile"; return 1; }
    why="$(abi_problem "$soft")"
    [ -n "$why" ] || { say "selftest: a SOFT-float object passed the attribute check"; return 1; }
}
selftest || fail=1

[ "$fail" -eq 0 ] || exit 1
say "OK — C, C++ and Rust images from the out-of-tree board package (copied, reached by NROS_EXTRA_BOARD_PATH and FIXTURE_R5F_SDK_ROOT) all carry Tag_ABI_VFP_args: VFP registers and Tag_FP_arch: VFPv3-D16; selftest: a softfp object is refused"
