#!/usr/bin/env bash
#
# Issue 2281 -- every `nros_cargo_build()` package that shares a cargo
# workspace root gets its OWN `--target-dir`.
#
# THE PROPERTY
#
# The Zephyr module builds `nros-c` and `nros-cpp` (their generated image
# crates, one cargo workspace) as two custom targets, each a separate
# `cargo build -p <one>`. Cargo unifies features PER INVOCATION, and the two
# invocations unify differently (measured with `--unit-graph`: the nros-cpp
# build turns on `nros/log`, `nros/macros`, `nros-rmw/log` and
# `nros-platform/alloc`, which the nros-c build does not). In one target dir
# both builds compile `nros-c` into the SAME fingerprint directory against
# different dependency units, so each rebuilt what the other had just built,
# and every `ninja` -- every `nros build`, every `nros run` -- relinked
# `zephyr.elf` with no source change.
#
# WHAT THIS ASSERTS
#
# `_nros_root_cargo_dir()`, the one answer to "which target dir", gives the
# two API crates different directories in both of its modes:
#
#   1. the default (no shared cargo root): nros-c keeps `<build>/nros-rust`,
#      where the per-image headers also live, and nros-cpp gets its own;
#   2. a shared cargo root (`NROS_SHARED_CARGO_ROOT`): the package is a field
#      of the key, so two packages never share a directory even when their
#      feature strings happen to be equal.
#
# WHY `check-fast`
#
# Buildless: one NONE-language configure that includes the module. No
# compiler, no cargo, no fixtures.

set -uo pipefail
cd "$(dirname "$0")/../../../.."
REPO="$PWD"

WORK="$(mktemp -d "${TMPDIR:-/tmp}/nros-api-target-dirs.XXXXXX")"
trap 'rm -rf "$WORK"' EXIT

cat >"$WORK/CMakeLists.txt" <<'EOF'
cmake_minimum_required(VERSION 3.22)
project(nros_api_target_dirs NONE)
include("${NROS_REPO}/zephyr/cmake/nros_cargo_build.cmake")

_nros_root_cargo_dir(_c   "rmw-cffi,platform-zephyr" nros-c)
_nros_root_cargo_dir(_cpp "rmw-cffi,platform-zephyr" nros-cpp)
message(STATUS "DEFAULT_C=[${_c}]")
message(STATUS "DEFAULT_CPP=[${_cpp}]")

# The shared mode, with the key recorded instead of hashed: the stub stands
# in for `nros_shared_cargo_dir`, whose own hashing is not under test.
function(nros_shared_cargo_dir out)
    cmake_parse_arguments(A "" "" "KEY" ${ARGN})
    string(SHA256 _h "${A_KEY}")
    set(${out} "${CMAKE_BINARY_DIR}/shared/${_h}" PARENT_SCOPE)
endfunction()
set(NROS_SHARED_CARGO_ROOT "${CMAKE_BINARY_DIR}/shared")
set(NROS_RESOLVED_KNOBS "")
_nros_root_cargo_dir(_sc   "same" nros-c)
_nros_root_cargo_dir(_scpp "same" nros-cpp)
message(STATUS "SHARED_C=[${_sc}]")
message(STATUS "SHARED_CPP=[${_scpp}]")
EOF

fail=0
bad() { echo "zephyr-api-cargo-target-dirs: FAIL -- $*" >&2; fail=1; }

log="$WORK/out.log"
# nros-cmake-prefix-exempt: a synthetic NONE-language project this gate writes
# itself; it includes one module and builds nothing.
if ! cmake -S "$WORK" -B "$WORK/b" -DNROS_REPO="$REPO" >"$log" 2>&1; then
    bad "configure failed"
    tail -15 "$log" | sed 's/^/      /' >&2
    exit 1
fi

val() { sed -n "s/.*$1=\[\(.*\)\]$/\1/p" "$log" | head -n 1; }
dc="$(val DEFAULT_C)"; dcpp="$(val DEFAULT_CPP)"
sc="$(val SHARED_C)"; scpp="$(val SHARED_CPP)"

[ "$dc" = "$WORK/b/nros-rust" ] \
    || bad "nros-c must keep <build>/nros-rust (the header root lives there), got [$dc]"
[ -n "$dcpp" ] && [ "$dcpp" != "$dc" ] \
    || bad "nros-c and nros-cpp share the default target dir [$dc] -- their builds rebuild each other's units (issue 2281)"
[ -n "$sc" ] && [ -n "$scpp" ] && [ "$sc" != "$scpp" ] \
    || bad "with a shared cargo root and equal feature strings, nros-c and nros-cpp got one dir [$sc] -- the package must be a key field (issue 2281)"

if [ "$fail" -ne 0 ]; then
    exit 1
fi
echo "zephyr-api-cargo-target-dirs: OK -- nros-c [$dc], nros-cpp [$dcpp]; shared mode keys on the package"
