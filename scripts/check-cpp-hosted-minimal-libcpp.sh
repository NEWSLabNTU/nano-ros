#!/usr/bin/env bash
# Issue 1432 — compile every public nros-cpp header in the (hosted compiler,
# minimal libcpp) quadrant that no C++ fixture occupies.
#
# WHY THIS CELL. Whether an `__STDC_HOSTED__`-guarded hosted STL include fires
# is decided by two INDEPENDENT facts: `__STDC_HOSTED__` is a property of the
# COMPILER INVOCATION, and whether `<map>` exists is a property of the INCLUDE
# PATH. The tree's C++ fixtures are `native_sim` (hosted, full libstdc++ — the
# arm fires and costs nothing) and `mps2_an385` (`-ffreestanding` — the arm
# never fires). A real Zephyr board built `-nostdinc++` against Zephyr's
# minimal libcpp is hosted=1 with no `<map>`, and that is where issue 1431
# lived, found by an outside team. `check-cpp-freestanding-includes` is a TEXT
# gate and can only refuse spellings someone anticipated; this is the compile
# that does not depend on anticipating the spelling.
#
# THE INVOCATION. The host `c++` (hosted, `__STDC_HOSTED__` == 1 — asserted
# below, not assumed), `-nostdinc++` so the host libstdc++ is gone, and
# `zephyr/cxx-compat/` on the include path as the C++ library. The C library
# stays the host's: the question is about C++ headers.
#
# THE CONFIG HEADERS. Every header reaches `nros_cpp_config_generated.h` /
# `nros_config_generated.h`, whose source-tree files are stubs that `#error`
# unless a build supplies the per-build header. This gate builds nothing, so it
# writes PROBE-ONLY forwarding headers into a temp dir that sits FIRST on the
# include path and nowhere else, each including the COMMITTED buildless
# snapshot (`nros_{cpp_,}config_generated_buildless.h`, issue 1569). Those
# snapshots carry every macro the per-build header defines —
# `check-config-fallback-macros` holds them to that — so no macro here is
# invented. (Forwarders rather than `-DNROS_CONFIG_BUILDLESS` is history: the
# define used to be `NROS_PLATFORM_NUTTX`, and the forwarder kept every OTHER
# NuttX-conditional on its non-NuttX arm.)
#
# Buildless, ~seconds: one `-fsyntax-only` per header. Fast line.
#
# Mutation-tested (issue 1432's resolution): moving `#include <map>` in
# node.hpp back under `#if defined(NROS_CPP_STD) || (__STDC_HOSTED__ + 0)`
# fails this gate with `map: No such file or directory`.

set -uo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$script_dir/.." && pwd)"
cd "$repo_root"

tag="cpp-hosted-minimal-libcpp"
CXX="${CXX:-c++}"
compat="zephyr/cxx-compat"
cpp_inc="packages/api/nros-cpp/include"

command -v "$CXX" >/dev/null 2>&1 || { echo "$tag: FAIL — no C++ compiler ($CXX)" >&2; exit 1; }
[ -d "$compat" ] || { echo "$tag: FAIL — $compat is MISSING; the probe would test nothing" >&2; exit 1; }
for snap in "$cpp_inc/nros/nros_cpp_config_generated_buildless.h" \
            packages/api/nros-c/include/nros/nros_config_generated_buildless.h; do
    [ -f "$snap" ] || { echo "$tag: FAIL — $snap is MISSING" >&2; exit 1; }
done

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
mkdir -p "$tmp/nros"
printf '%s\n' \
    '/* PROBE-ONLY (scripts/check-cpp-hosted-minimal-libcpp.sh, issue 1432). */' \
    '#include "nros/nros_cpp_config_generated_buildless.h"' \
    > "$tmp/nros/nros_cpp_config_generated.h"
printf '%s\n' \
    '/* PROBE-ONLY (scripts/check-cpp-hosted-minimal-libcpp.sh, issue 1432). */' \
    '#include "nros/nros_config_generated_buildless.h"' \
    > "$tmp/nros/nros_config_generated.h"

flags=(-fsyntax-only -std=c++14 -nostdinc++ -fno-exceptions -fno-rtti
       -I"$tmp" -I"$compat" -I"$cpp_inc"
       -Ipackages/api/nros-c/include
       -Ipackages/platform/nros-platform-api/include)

# One header, the exact invocation the gate uses. Output on stdout; rc of cxx.
probe_header() {
    "$CXX" "${flags[@]}" -include "$1" -x c++ /dev/null 2>&1
}

# --- selftest, on every run ---------------------------------------------------
# The cell is only this cell if the compiler is hosted AND the host libstdc++ is
# really gone, and the gate is only a gate if issue 1431's exact spelling FAILS
# through `probe_header` while the both-probes spelling (CLAUDE.md, issues 0112
# + 1240) passes. Any of these going the wrong way means the loop below answers
# a different question, so it is a failure, not a skip.
selftest() {
    local hosted out rc
    hosted="$(printf '__STDC_HOSTED__\n' | "$CXX" -E -P -x c++ - 2>/dev/null | tr -d '[:space:]')"
    if [ "$hosted" != "1" ]; then
        echo "$tag: SELFTEST FAIL — $CXX reports __STDC_HOSTED__=$hosted; this probe needs a hosted compiler" >&2
        return 1
    fi
    printf '%s\n' '#if defined(NROS_CPP_STD) || (__STDC_HOSTED__ + 0)' '#include <map>' '#endif' \
        > "$tmp/defect_1431.hpp"
    rc=0; out="$(probe_header "$tmp/defect_1431.hpp")" || rc=$?
    case "$rc:$out" in
        0:*) echo "$tag: SELFTEST FAIL — issue 1431's __STDC_HOSTED__-only <map> compiled; the include path is not minimal" >&2
             return 1 ;;
        *"map: No such file or directory"*) ;;
        *) echo "$tag: SELFTEST FAIL — the 1431 control failed for another reason:" >&2
           printf '%s\n' "$out" >&2
           return 1 ;;
    esac
    printf '%s\n' \
        '#if defined(NROS_CPP_STD) || (defined(__STDC_HOSTED__) && __STDC_HOSTED__ && __has_include(<map>))' \
        '#include <map>' '#endif' '#include <cstdint>' > "$tmp/both_probes.hpp"
    rc=0; out="$(probe_header "$tmp/both_probes.hpp")" || rc=$?
    if [ "$rc" -ne 0 ]; then
        echo "$tag: SELFTEST FAIL — the both-probes spelling did not compile:" >&2
        printf '%s\n' "$out" >&2
        return 1
    fi
}
selftest || exit 1

n=0
fails=0
for hdr in "$cpp_inc"/nros/*.hpp; do
    n=$((n + 1))
    rc=0
    out="$(probe_header "$hdr")" || rc=$?
    if [ "$rc" -ne 0 ]; then
        fails=$((fails + 1))
        echo "  FAIL  $hdr" >&2
        printf '%s\n' "$out" | grep -E 'error' | head -5 | sed 's/^/        /' >&2
    fi
done

if [ "$n" -eq 0 ]; then
    echo "$tag: FAIL — no headers under $cpp_inc/nros; the probe would pass on absence" >&2
    exit 1
fi
if [ "$fails" -gt 0 ]; then
    echo "$tag: FAILED — $fails of $n nros-cpp header(s) do not compile hosted against Zephyr's" >&2
    echo "  minimal libcpp. A hosted STL include needs BOTH probes (issues 0112 + 1240):" >&2
    echo "  #if defined(NROS_CPP_STD) || (defined(__STDC_HOSTED__) && __STDC_HOSTED__ && __has_include(<hdr>))" >&2
    exit 1
fi
echo "$tag: $n nros-cpp headers compile hosted (__STDC_HOSTED__=1) with -nostdinc++ -I $compat."
exit 0
