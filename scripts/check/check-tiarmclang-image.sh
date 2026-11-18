#!/usr/bin/env bash
# check-tiarmclang-image: a FreeRTOS image built and linked by TI Arm Clang,
# with nano-ros's Rust staticlib built by rustc (phase-575 W3).
#
# The board is `fixture-r5f-tiarmclang`, the out-of-tree fixture board package
# (packages/testing/nros-tests/fixtures/out-of-tree-r5f/) built the way TI's
# Processor SDK builds an R5F: every C translation unit, nano-ros's included,
# by tiarmclang against TI's C library, and the image linked by tiarmlnk with
# the package's linker command file. The fixture is COPIED out first and
# reached only through NROS_EXTRA_BOARD_PATH, as a user's repository would be.
#
# Asserts, for the C and the C++ talker:
#   - `nros build fixture-r5f-tiarmclang` succeeds, and its image is a TI link
#     (`.out`, CMake's TIClang suffix);
#   - the image is cortex-r5 / VFPv3-D16 / Tag_ABI_VFP_args: VFP registers;
#   - the image's C library is TI's (its map names TI's libc.a), never newlib.
# And, every run, how tiarmlnk resolves a weak board hook between objects
# from the two compilers (nano-ros's weak default built by arm-none-eabi-gcc,
# a board's strong definition built by tiarmclang), the cases a board relies
# on (nros_board_wall_clock_ns, nros_board_register_netif,
# nros_board_network_wait):
#   A  weak default + the board's strong definition in an archive -> default
#      (a linker never pulls an archive member to replace a weak definition);
#   B  no weak default, strong definition in an archive -> the strong one;
#   C  no weak default, no strong definition -> unresolved symbol, an error;
#   D  weak default + strong definition in an object -> the strong one.
# GNU ld gives the same four answers; a TI release that changed one fails here.
#
# Needs TI Arm Clang (licence-gated, never fetched: TI_ARM_CLANG_ROOT names an
# install), an arm-none-eabi-gcc, cmake and the armv7r-none-eabihf rust-std.
set -euo pipefail
repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
fixture="$repo/packages/testing/nros-tests/fixtures/out-of-tree-r5f"
mkdir -p "$repo/tmp"
work="$(mktemp -d "$repo/tmp/tiarmclang-image.XXXXXX")"
trap 'rm -rf "$work"' EXIT
say() { echo "check-tiarmclang-image: $*"; }
# shellcheck source=../build/cargo.sh
. "$repo/scripts/build/cargo.sh"
# shellcheck source=../lib/grep-q.sh
. "$repo/scripts/lib/grep-q.sh"
if ! nros="$(nros_cli_bin)" || [ -z "$nros" ]; then
    say "no nros CLI (just setup-cli, or source ./activate.sh)"; exit 1
fi
ti="${TI_ARM_CLANG_ROOT:-}"
if [ -z "$ti" ] || [ ! -x "$ti/bin/tiarmclang" ]; then
    say "TI_ARM_CLANG_ROOT does not name a TI Arm Clang install (bin/tiarmclang);"
    say "  it is licence-gated: install it from TI and set TI_ARM_CLANG_ROOT"
    exit 1
fi
gcc="$(command -v arm-none-eabi-gcc || true)"
[ -n "$gcc" ] || { say "no arm-none-eabi-gcc on PATH"; exit 1; }
readelf="$(dirname "$gcc")/arm-none-eabi-readelf"
# The cargo road's C builds name the compiler by `tiarmclang` (the board's
# cargo_config), so its bin/ joins PATH, as the board definition says.
export PATH="$ti/bin:$PATH"

# What an image's attributes lack, if anything; empty when it is a hard-float
# Cortex-R5 object.
abi_problem() { # <elf>
    local a want
    a="$("$readelf" -A "$1" 2>/dev/null || true)"
    for want in 'Tag_CPU_name: "cortex-r5"' 'Tag_FP_arch: VFPv3-D16' 'Tag_ABI_VFP_args: VFP registers'; do
        if ! nros_grep_q -F "$want" <<<"$a"; then
            echo "lacks $want"; return
        fi
    done
}

# Negative control, every run: the same attribute check is handed a SOFT-float
# object tiarmclang built, and must refuse it, so a check that read the wrong
# file or an empty attribute set could not pass.
selftest() {
    local soft="$work/soft.o"
    printf '%s\n' 'float scale(float x) { return x * 2.0f; }' > "$work/soft.c"
    "$ti/bin/tiarmclang" -mcpu=cortex-r5 -mfpu=vfpv3-d16 -mfloat-abi=softfp \
        -c "$work/soft.c" -o "$soft" 2> "$work/soft.log" \
        || { say "selftest: the softfp object did not compile"; return 1; }
    [ -n "$(abi_problem "$soft")" ] \
        || { say "selftest: a SOFT-float object passed the attribute check"; return 1; }
}

fail=0
selftest || fail=1

# --- weak/strong resolution across the two compilers ---------------------
ws="$work/weak-strong"
mkdir -p "$ws"
printf '%s\n' '#include <stdbool.h>' '#include <stdint.h>' \
    'bool nros_board_wall_clock_ns(uint64_t *out_ns);' \
    'volatile int ok; volatile uint64_t v;' \
    'int main(void) { uint64_t t = 0; ok = nros_board_wall_clock_ns(&t); v = t; return 0; }' > "$ws/main.c"
printf '%s\n' '#include <stdbool.h>' '#include <stdint.h>' \
    '__attribute__((weak)) bool nros_board_wall_clock_ns(uint64_t *out_ns) { (void)out_ns; return false; }' > "$ws/weak.c"
printf '%s\n' '#include <stdbool.h>' '#include <stdint.h>' \
    'bool nros_board_wall_clock_ns(uint64_t *out_ns) { *out_ns = 42; return true; }' > "$ws/hook.c"
printf '%s\n' '--entry_point=main' '--stack_size=0x400' '--heap_size=0x400' \
    'MEMORY { RAM (RWX) : origin = 0x00100000, length = 0x00100000 }' \
    'SECTIONS { .text : {} > RAM  .rodata : {} > RAM  .data : {} > RAM  .bss : {} > RAM' \
    '  .cinit : {} > RAM  .stack : {} > RAM  .sysmem : {} > RAM  .init_array : {} > RAM' \
    '  .ARM.exidx : {} > RAM  .ARM.extab : {} > RAM }' > "$ws/lnk.cmd"
tflags=(-mcpu=cortex-r5 -mfpu=vfpv3-d16 -mfloat-abi=hard)
"$ti/bin/tiarmclang" "${tflags[@]}" -c "$ws/main.c" -o "$ws/main.o"
"$ti/bin/tiarmclang" "${tflags[@]}" -c "$ws/hook.c" -o "$ws/hook.o"
"$gcc" "${tflags[@]}" -c "$ws/weak.c" -o "$ws/weak.o"
"$ti/bin/tiarmar" rc "$ws/libboard.a" "$ws/hook.o" >/dev/null
# Which definition the image took: the strong one stores 42.
took() { # <out>
    local dis body
    dis="$("$ti/bin/tiarmobjdump" -d "$1")"
    body="$(grep -A6 '<nros_board_wall_clock_ns>:' <<<"$dis" || true)"
    if nros_grep_q -E '#(42|0x2a)\b' <<<"$body"; then
        echo strong
    else
        echo weak
    fi
}
link() { # <out> <inputs...>
    local out="$1"; shift
    "$ti/bin/tiarmclang" "${tflags[@]}" "$@" -Wl,--rom_model "-Wl,$ws/lnk.cmd" -o "$out" > "$out.log" 2>&1
}
if ! link "$ws/A.out" "$ws/main.o" "$ws/weak.o" "$ws/libboard.a"; then
    say "case A: did not link"; fail=1
elif [ "$(took "$ws/A.out")" != weak ]; then
    say "case A: an archived strong hook replaced a linked weak default"; fail=1
fi
if link "$ws/B.out" "$ws/main.o" "$ws/libboard.a" && [ "$(took "$ws/B.out")" = strong ]; then :; else
    say "case B: with no weak default, the archived strong hook was not pulled"; fail=1
fi
if link "$ws/C.out" "$ws/main.o"; then
    say "case C: no definition at all linked"; fail=1
elif ! nros_grep_q 'nros_board_wall_clock_ns' "$ws/C.out.log"; then
    say "case C: the link failed without naming the missing hook"; fail=1
fi
if link "$ws/D.out" "$ws/main.o" "$ws/weak.o" "$ws/hook.o" && [ "$(took "$ws/D.out")" = strong ]; then :; else
    say "case D: a strong hook in an object did not replace the weak default"; fail=1
fi

# --- the image -------------------------------------------------------------
cp -r "$fixture/boards" "$fixture/sdk" "$work/"
for ex in "$work"/boards/nros-board-fixture-r5f-freertos/examples/*-talker; do
    rm -rf "${ex:?}/build" "${ex:?}/generated" "${ex:?}/.cargo"
done
export NROS_EXTRA_BOARD_PATH="$work/boards"
export FIXTURE_R5F_SDK_ROOT="$work/sdk"
export TI_ARM_CLANG_ROOT="$ti"

for lang in c cpp; do
    ex="$work/boards/nros-board-fixture-r5f-freertos/examples/$lang-talker"
    if ! (cd "$ex" && "$nros" build fixture-r5f-tiarmclang) > "$work/$lang.log" 2>&1; then
        say "$lang: \`nros build fixture-r5f-tiarmclang\` failed:"
        grep -E -i '(^|[[:space:]])error|FATAL|undefined|unresolved' "$work/$lang.log" | head -12 | sed 's/^/    /'
        fail=1
        continue
    fi
    if ! elf="$(find "$ex/build" -type f -name fixture_r5f_tiarmclang_entry.out | head -1)"; then
        elf=""
    fi
    if [ -z "$elf" ]; then
        say "$lang: built, but no TI image (fixture_r5f_tiarmclang_entry.out) under $ex/build"
        fail=1
        continue
    fi
    why="$(abi_problem "$elf")"
    [ -z "$why" ] || { say "$lang: $why ($elf)"; fail=1; }
    map="${elf%.out}.map"
    if [ ! -f "$map" ] || ! nros_grep_q 'libc\.a' "$map"; then
        say "$lang: no tiarmlnk map naming TI's libc.a beside the image ($map)"; fail=1
    fi
done

[ "$fail" -eq 0 ] || exit 1
say "OK — C and C++ images for fixture-r5f-tiarmclang: C by tiarmclang, Rust by rustc, linked by tiarmlnk against TI's libc, cortex-r5 / VFPv3-D16 / VFP registers; weak board hooks resolve as on GNU ld (A default, B pulled, C error, D strong); selftest: a softfp object is refused"
