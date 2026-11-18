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
# Issue 2692 -- and every OBJECT of every archive the build wrote for the
# target (cc-rs's C from every crate, rustc's own, the cmake lane's) must say
#   Tag_CPU_arch_profile: Realtime, Tag_FP_arch: VFPv3-D16,
#   Tag_ABI_VFP_args: VFP registers
# because GNU ld links an M-profile object into an R5F image without a word.
# The image's arch profile reaches every cc-rs build through ONE carrier
# (`CFLAGS_<triple>` in the image's cargo config); the C build also runs with a
# user `CFLAGS` exported, and must say it merged it.
# Selftest, every run: the same attribute check is handed a SOFT-float object
# (the fixture's C compiled -mfloat-abi=softfp) and must refuse it, so a check
# that read the wrong file or an empty attribute set could not pass.
#
# Build tier: three cross builds (zenoh-pico, lwIP, FreeRTOS, a Rust entry),
# several minutes cold. Needs an arm-none-eabi-gcc, cmake and the
# `armv7r-none-eabihf` rust-std.
set -euo pipefail
repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
# shellcheck source=../lib/grep-q.sh
source "$repo/scripts/lib/grep-q.sh"
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

# Every ELF member of every ARM archive under <dir>. SUCCEEDS when it found a
# problem, printing it: a member whose CPU profile, FPU or float ABI is not the
# R5F's, or no member at all (a check that read nothing must not pass).
#
# The float ABI is asked only of a member that states a calling convention
# (any Tag_ABI_* attribute). A compiled object always states one, so a
# softfp C object is still refused. Hand-written assembly states none: GNU as
# writes no Tag_ABI_VFP_args even with -mfloat-abi=hard (measured, gcc 13.2),
# so FreeRTOS's portASM.S makes no ABI claim and the linker accepts it either
# way. Its profile and FPU are still checked.
member_problems() { # <dir>
    local a n=0 bad=0 hdr archives=()
    mapfile -t archives < <(find "$1" -type f -name '*.a' 2>/dev/null | sort)
    wait $! || { echo "    could not list the archives under $1"; return 0; }
    for a in "${archives[@]}"; do
        # Read whole, then match: a matcher that stops early can SIGPIPE
        # readelf, and pipefail would read that as no match.
        hdr="$("$readelf" -h "$a" 2>/dev/null || true)"
        [[ $hdr == *Machine:*ARM* ]] || continue
        # shellcheck disable=SC2016
        out="$("$readelf" -A "$a" 2>/dev/null | awk -v ar="$a" '
            function flush() {
                if (f != "" && seen) {
                    n++
                    if (prof != "Realtime" || fp != "VFPv3-D16" || (abi && vfp != "VFP registers"))
                        printf "BAD %s: profile=%s fp=%s vfp_args=%s\n", f, prof, fp, vfp
                }
            }
            /^File: / { flush(); f = $2; seen = 0; abi = 0; prof = "absent"; fp = "absent"; vfp = "absent"; next }
            /Tag_ABI_/ { abi = 1 }
            /Tag_CPU_arch_profile:/ { sub(/.*Tag_CPU_arch_profile: */, ""); prof = $0; seen = 1 }
            /Tag_FP_arch:/ { sub(/.*Tag_FP_arch: */, ""); fp = $0; seen = 1 }
            /Tag_ABI_VFP_args:/ { sub(/.*Tag_ABI_VFP_args: */, ""); vfp = $0; seen = 1 }
            END { flush(); print "N " n }')"
        n=$((n + $(sed -n 's/^N //p' <<<"$out")))
        if [[ $'\n'$out == *$'\n''BAD '* ]]; then
            bad=1
            grep '^BAD ' <<<"$out" | head -5 | sed 's/^BAD /    /'
        fi
    done
    [ "$n" -gt 0 ] || { echo "    no ARM archive member carrying attributes under $1"; return 0; }
    [ "$bad" -eq 0 ] || return 0
    return 1
}

fail=0
for lang in c cpp rust; do
    ex="$work/boards/nros-board-fixture-r5f-freertos/examples/$lang-talker"
    # The C build carries a user CFLAGS, which the carrier must merge, and say so.
    user_cflags=()
    [ "$lang" = c ] && user_cflags=(env CFLAGS=-fno-common)
    if ! (cd "$ex" && "${user_cflags[@]}" "$nros" build fixture-r5f-freertos) > "$work/$lang.log" 2>&1; then
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
    if why="$(member_problems "$ex/build")"; then
        say "$lang: an archive member is not built for the R5F:"
        echo "$why"
        fail=1
    fi
    if [ "$lang" = c ]; then
        rc=0
        nros_grep_q 'CFLAGS_armv7r_none_eabihf is the image.s arch profile followed by your CFLAGS (`-fno-common`)' "$work/$lang.log" || rc=$?
        case $rc in
            0) ;;
            1)
                say "c: a user CFLAGS was set, and the build did not say it merged it into CFLAGS_armv7r_none_eabihf"
                fail=1
                ;;
            *)
                say "c: could not read the build log $work/$lang.log"
                fail=1
                ;;
        esac
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
    # The member check: an archive holding one M-profile object is refused,
    # and one holding only R5F objects passes.
    local m="$work/selftest-m" r="$work/selftest-r"
    mkdir -p "$m" "$r"
    "$cc" -mcpu=cortex-r5 -mfpu=vfpv3-d16 -mfloat-abi=hard -c -o "$r/ok.o" \
        "$fixture/sdk/src/fxsdk.c" -I"$fixture/sdk/include" 2>> "$work/soft.log" \
        || { say "selftest: the R5F object did not compile"; return 1; }
    "$cc" -mcpu=cortex-m4 -mthumb -mfpu=fpv4-sp-d16 -mfloat-abi=hard -c -o "$m/m4.o" \
        "$fixture/sdk/src/fxsdk.c" -I"$fixture/sdk/include" 2>> "$work/soft.log" \
        || { say "selftest: the M-profile object did not compile"; return 1; }
    # A hard-float assembly object: no ABI claim, and it must still pass.
    printf '.text\n.global f\nf: bx lr\n' > "$r/asm.S"
    "$cc" -mcpu=cortex-r5 -mfpu=vfpv3-d16 -mfloat-abi=hard -c -o "$r/asm.o" "$r/asm.S" \
        2>> "$work/soft.log" || { say "selftest: the R5F assembly object did not compile"; return 1; }
    local s="$work/selftest-s"
    mkdir -p "$s"
    "${readelf%readelf}ar" rcs "$r/libok.a" "$r/ok.o" "$r/asm.o"
    "${readelf%readelf}ar" rcs "$m/libmixed.a" "$r/ok.o" "$m/m4.o"
    "${readelf%readelf}ar" rcs "$s/libsoft.a" "$r/ok.o" "$soft"
    rm -f "$r/ok.o" "$r/asm.o" "$r/asm.S" "$m/m4.o"
    member_problems "$r" > /dev/null && { say "selftest: an all-R5F archive (with an assembly member) was refused"; return 1; }
    member_problems "$m" > /dev/null || { say "selftest: an archive holding an M-profile object passed"; return 1; }
    member_problems "$s" > /dev/null || { say "selftest: an archive holding a softfp C object passed"; return 1; }
    return 0
}
selftest || fail=1

[ "$fail" -eq 0 ] || exit 1
say "OK — C, C++ and Rust images from the out-of-tree board package (copied, reached by NROS_EXTRA_BOARD_PATH and FIXTURE_R5F_SDK_ROOT) all carry Tag_ABI_VFP_args: VFP registers and Tag_FP_arch: VFPv3-D16, and every member of every target archive is R-profile with that FPU, and every member stating a calling convention uses VFP registers; a user CFLAGS is merged and reported; selftest: a softfp object, an archive with an M-profile member and one with a softfp member are refused, an archive with a hard-float assembly member passes"
