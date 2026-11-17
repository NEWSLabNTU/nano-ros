#!/usr/bin/env bash
# check-r5f-hard-float-abi: the hard-float Cortex-R5F profile produces hard-float
# objects on every half of an image (phase-575 W1).
#
# `[arch.cortex-r5f]` (nros-platform-freertos) and
# cmake/toolchain/arm-freertos-armcr5f.cmake state `-mfloat-abi=hard`, and the
# Rust half is `armv7r-none-eabihf`. If any one of them drifts, a float crosses
# the C/Rust boundary in the wrong register file. This builds the
# `r5f_hard_float_abi` fixture's C and C++ through that toolchain file and its
# Rust for `armv7r-none-eabihf`, links them into one relocatable object, and
# reads the EABI attributes back:
#   - every input and the linked object say `Tag_ABI_VFP_args: VFP registers`;
#   - the C flags the toolchain file sets are the profile's flags.
# Selftest, on every run (the negative control): the same C compiled `softfp`
# must NOT link with the hard-float Rust half, and ld must name the VFP
# argument mismatch. Without it, an attribute check that read the wrong file
# would pass.
#
# No board and no FreeRTOS: those images are W2's. Needs an
# arm-none-eabi-gcc, cmake, and the `armv7r-none-eabihf` rust-std.
set -euo pipefail
repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
fixture="$repo/packages/testing/nros-tests/fixtures/r5f_hard_float_abi"
toolchain="$repo/cmake/toolchain/arm-freertos-armcr5f.cmake"
profile="$repo/packages/platform/nros-platform-freertos/nros-platform.toml"
mkdir -p "$repo/tmp"
work="$(mktemp -d "$repo/tmp/r5f-hard-float-abi.XXXXXX")"
trap 'rm -rf "$work"' EXIT
fail=0
say() { echo "check-r5f-hard-float-abi: $*"; }

# The profile's flags, as the resolver reads them.
profile_flags="$(python3 - "$profile" <<'PYEOF'
import sys
try:
    import tomllib
except ModuleNotFoundError:
    import tomli as tomllib
d = tomllib.load(open(sys.argv[1], "rb"))
print(" ".join(d["arch"]["cortex-r5f"]["cflags"]))
PYEOF
)"

# C and C++ through the toolchain file.
# The fixture is a plain C/C++ project that never includes nano-ros; only its
# toolchain file is nano-ros's, named by absolute path.
configure=(cmake -S "$fixture" -B "$work/c" -G Ninja -DCMAKE_TOOLCHAIN_FILE="$toolchain")  # copy-out-root-ok: not a nano-ros consumer
if ! "${configure[@]}" > "$work/configure.log" 2>&1 \
   || ! cmake --build "$work/c" > "$work/build.log" 2>&1; then
    say "the C/C++ half did not build:"; tail -20 "$work"/*.log | sed 's/^/    /'
    exit 1
fi
cc="$(sed -n 's/^CMAKE_C_COMPILER:[A-Z]*=//p' "$work/c/CMakeCache.txt" | tr -d '"')"
[ -n "$cc" ] || cc="$(grep -m1 -o '[^ "]*arm-none-eabi-gcc' "$work/c/CMakeFiles"/*/CMakeCCompiler.cmake)"
objcopy_dir="$(dirname "$cc")"
readelf="$objcopy_dir/arm-none-eabi-readelf"
ar="$objcopy_dir/arm-none-eabi-ar"
command -v "$readelf" >/dev/null || readelf=arm-none-eabi-readelf
# The toolchain file's C flags carry the profile's, in order.
c_flags="$(sed -n 's/^CMAKE_C_FLAGS:STRING=//p' "$work/c/CMakeCache.txt")"
case " $c_flags " in
    *" $profile_flags "*) ;;
    *) say "the toolchain file's C flags ($c_flags) do not carry the profile's ($profile_flags)"; fail=1 ;;
esac

# The Rust half: one staticlib, no cargo (no lock, no registry).
rustc --edition 2024 --crate-type staticlib --target armv7r-none-eabihf \
    -C opt-level=s -C panic=abort --crate-name rs_abi \
    -o "$work/librs_abi.a" "$fixture/rs_abi.rs" 2> "$work/rustc.log" \
    || { say "the Rust half did not build:"; sed 's/^/    /' "$work/rustc.log"; exit 1; }

vfp_args() { "$readelf" -A "$1" 2>/dev/null | sed -n 's/.*Tag_ABI_VFP_args: *//p' | sort -u; }

clib="$(find "$work/c" -name 'libr5f_abi_c.a' | head -1)"
for obj in "$clib" "$work/librs_abi.a"; do
    got="$(vfp_args "$obj")"
    if [ "$got" != "VFP registers" ]; then
        say "$(basename "$obj"): Tag_ABI_VFP_args is '${got:-absent}', want 'VFP registers'"
        fail=1
    fi
done

# Link the halves: ld merges the attributes and refuses a mismatch.
# shellcheck disable=SC2086
if ! "$cc" $profile_flags -nostdlib -r -o "$work/image.o" \
        -Wl,--whole-archive "$clib" "$work/librs_abi.a" -Wl,--no-whole-archive \
        2> "$work/link.log"; then
    say "the hard-float halves did not link:"; sed 's/^/    /' "$work/link.log"; fail=1
else
    got="$(vfp_args "$work/image.o")"
    [ "$got" = "VFP registers" ] \
        || { say "linked object: Tag_ABI_VFP_args is '${got:-absent}', want 'VFP registers'"; fail=1; }
fi

selftest() {
    local soft="${profile_flags/-mfloat-abi=hard/-mfloat-abi=softfp}"
    # shellcheck disable=SC2086
    "$cc" $soft -c -o "$work/soft.o" "$fixture/abi.c" 2> "$work/soft-cc.log" \
        || { say "selftest: the softfp C did not compile"; return 1; }
    # shellcheck disable=SC2086
    if "$cc" $soft -nostdlib -r -o "$work/mixed.o" "$work/soft.o" \
            -Wl,--whole-archive "$work/librs_abi.a" -Wl,--no-whole-archive \
            > "$work/mixed.log" 2>&1; then
        say "selftest: a softfp C half LINKED with the hard-float Rust half; the ABI check cannot be trusted"
        return 1
    fi
    grep -q "VFP register arguments" "$work/mixed.log" \
        || { say "selftest: the mixed link failed, but not on the VFP argument rule:"; sed 's/^/    /' "$work/mixed.log"; return 1; }
}
selftest || fail=1

if [ "$fail" -ne 0 ]; then
    exit 1
fi
say "OK — C, C++ and Rust built for [arch.cortex-r5f] all say 'Tag_ABI_VFP_args: VFP registers', link together, and the toolchain file carries the profile's flags; selftest: a softfp C half is refused"
