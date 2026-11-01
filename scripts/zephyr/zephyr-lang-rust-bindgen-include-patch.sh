#!/usr/bin/env bash
# scripts/zephyr/zephyr-lang-rust-bindgen-include-patch.sh
#
# Issue 2203 — keep the C compiler's BUILTIN include directory out of the
# include path zephyr-lang-rust hands to bindgen.
#
# `rust_cargo_application()` passes `zephyr_interface`'s
# INTERFACE_INCLUDE_DIRECTORIES to `zephyr-sys/build.rs` as INCLUDE_DIRS, and
# build.rs turns each one into a `-I` for bindgen. Zephyr puts the compiler's
# own header dir on that target (`$<TARGET_PROPERTY:compiler,nostdinc_include>`,
# i.e. `arm-none-eabi-gcc -print-file-name=include`), because it builds with
# -nostdinc. That directory belongs to GCC, and bindgen is CLANG: a `-I` is
# searched before clang's own resource dir, so every compiler-private header
# resolves to GCC's copy.
#
# CMSIS_6 is what makes that fatal. Its m-profile headers include
# `<arm_acle.h>` unconditionally (cmsis_clang_m.h under clang), and GCC's
# arm_acle.h wraps `__builtin_arm_cdp` & co. in inline functions taking the
# coprocessor number as a parameter. Clang requires those arguments to be
# integer constants, so the PARSE fails:
#
#   arm_acle.h:41:3: error: argument to '__builtin_arm_cdp' must be a constant integer
#
# 18 times, and zephyr-sys panics "Unable to generate bindings". Measured with
# Arm GNU 13.2 (gnuarmemb) and the Zephyr SDK 1.0.1's GCC 14.3 alike, for
# thumbv7m / thumbv7em / thumbv8m.main under libclang 14 — so every Rust
# Cortex-M image on the 4.4 line. The 3.7 line carries CMSIS 5, whose headers
# never include arm_acle.h, so it only ever looked fine.
#
# The fix removes that one directory from bindgen's list. Clang then takes
# stddef.h / stdarg.h / arm_acle.h / ... from its own resource dir — the headers
# written for the compiler that is actually parsing — which is also what a
# Zephyr build with the clang toolchain does. Nothing else reads INCLUDE_DIRS,
# and the C build is untouched (its include path is the target's, not this
# copy).
#
# Both spellings are removed: the generator expression (how Zephyr 3.7 and 4.4
# put it there) and the evaluated paths, in case a later line stores them
# literally.
#
# Upstreamable and not nano-ros-specific; `zephyr/patches.yml` (module
# `zephyr-lang-rust`) carries the same change for `west patch`.
#
# Idempotent: guarded by a grep for its own marker.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
NROS_REPO_DIR="$(cd "$SCRIPT_DIR/../.." && pwd)"
# The workspace is an ARGUMENT, never resolved here: every caller already has
# it, and a fallback ladder in a patch script is the drift
# `.config/zephyr-workspace-resolvers.txt` exists to stop.
WORKSPACE="${1:?usage: zephyr-lang-rust-bindgen-include-patch.sh <zephyr-workspace-dir>}"

RUST_MODULE="$WORKSPACE/modules/lang/rust"
CMAKE_FILE="$RUST_MODULE/CMakeLists.txt"

# Absent module is NOT an error: a C/C++-only workspace legitimately has none,
# and the setup recipes call every patch script unconditionally.
if [ ! -d "$RUST_MODULE" ]; then
    echo "zephyr-lang-rust-bindgen-include-patch: no rust module at $RUST_MODULE — skipping"
    exit 0
fi
if [ ! -f "$CMAKE_FILE" ]; then
    echo "ERROR: $CMAKE_FILE missing (unexpected layout for zephyr-lang-rust)" >&2
    exit 1
fi

# `nros_grep_q`, not a bare `grep -q`: a grep that fails to run must not read as
# "not applied yet" (issue 0726).
# shellcheck source=scripts/lib/grep-q.sh
source "$NROS_REPO_DIR/scripts/lib/grep-q.sh"
_rc=0
nros_grep_q "nano-ros: bindgen is clang" "$CMAKE_FILE" || _rc=$?
if [ "$_rc" -eq 0 ]; then
    echo "zephyr-lang-rust-bindgen-include-patch: already applied"
    exit 0
elif [ "$_rc" -ne 1 ]; then
    echo "ERROR: could not read $CMAKE_FILE (grep exit $_rc)" >&2
    exit 1
fi

python3 - "$CMAKE_FILE" <<'PY'
import sys, pathlib
p = pathlib.Path(sys.argv[1])
t = p.read_text()
old = "  get_target_property(include_dirs ${target} INTERFACE_INCLUDE_DIRECTORIES)\n"
new = old + """\
  # nano-ros: bindgen is clang, so drop the C compiler's builtin include dir
  # (issue 2203). As a `-I` it shadows clang's resource dir, and GCC's
  # arm_acle.h (included by CMSIS_6) does not parse under clang.
  if(include_dirs)
    list(REMOVE_ITEM include_dirs "$<TARGET_PROPERTY:compiler,nostdinc_include>")
    if(TARGET compiler)
      get_property(_nros_nostdinc TARGET compiler PROPERTY nostdinc_include)
      if(_nros_nostdinc)
        list(REMOVE_ITEM include_dirs ${_nros_nostdinc})
      endif()
    endif()
  endif()
"""
assert t.count(old) == 1, "CMakeLists.txt: get_include_dirs() not in the expected shape (upstream moved?)"
p.write_text(t.replace(old, new, 1))
PY
echo "  [patched] $CMAKE_FILE — compiler builtin include dir kept out of bindgen's INCLUDE_DIRS"
echo "zephyr-lang-rust-bindgen-include-patch: done"
