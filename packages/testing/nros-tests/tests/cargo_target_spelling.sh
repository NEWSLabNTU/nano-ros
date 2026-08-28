#!/usr/bin/env bash
#
# phase-340 W3 — ONE `--target` spelling for every cargo command cmake emits.
#
# THE PROPERTY
#
# `--target <host-triple>` and no `--target` at all are DIFFERENT cargo
# identities on the same machine. Measured on `nros-core`
# (`--no-default-features --features alloc,std`, `nros-relwithdebinfo`):
#
#   implicit host                        libnros_core-0f6269f7a00e4b29.rlib
#   --target x86_64-unknown-linux-gnu    libnros_core-842ac3b7840799eb.rlib
#
# and the two share NOTHING, not even through the compiler cache: on a private
# cold sccache, building one spelling then the other gave 0 hits / 7 misses for
# `nros-core` and 0 hits / 62 misses for `nros`, where an immediate repeat of
# the first spelling scored 7 and 44 hits. So the split is duplicated CPU, not
# just duplicated bytes.
#
# Corrosion — which builds most of any cmake tree here — hardcodes `--target`,
# because its artifact-path model is `<target-dir>/<triple>/<profile>/`. It is
# upstream and we do not fork it, so it is the fixed point: nano-ros' OWN cargo
# custom commands normalise TO the explicit spelling. Cost of doing so, also
# measured: none in work done. `cargo --unit-graph` for `nros-c`
# (`std,rmw-zenoh`) reports 165 units and 160 distinct compilation signatures
# with either spelling; the explicit form only relabels 37 of them from the
# host half to the target half.
#
# WHAT THIS ASSERTS
#
# `_nros_resolve_rust_target()` is the single answer to "which triple", and it
# has to survive the scopes cmake actually presents:
#
#   1. no Corrosion in scope at all      -> rustc's own host triple
#   2. a toolchain file's Rust_CARGO_TARGET (a normal/cache variable)
#   3. ONLY Corrosion's cache copy       -> the phase-155 scope, where the
#      normal variable was published PARENT_SCOPE and did not cross an
#      `add_subdirectory()` boundary. Reading only the normal variable there
#      built host x86_64 objects into an ARM link.
#   4. nothing readable and no rustc     -> FATAL, never a silent fall back to
#      the implicit spelling this work item exists to remove.
#
# and `_nros_ffi_cargo_args()` must reject an empty RUST_TARGET (5) while still
# supporting the one legitimate reason to omit the FLAG — a generated
# `.cargo/config.toml` that already carries `[build] target` (6).
#
# WHY `check-fast`
#
# Buildless: four `cmake -P`-scale configures of a NONE-language project that
# includes one module. No compiler, no cargo, no fixtures. Seconds.

set -uo pipefail
cd "$(dirname "$0")/../../../.."
REPO="$PWD"

# Issue 0726 — both needle searches below are `if ! grep -qF`, so a grep that
# failed to START would report "expected <needle>", i.e. that the resolver
# emitted the wrong triple, and send the reader into `_nros_resolve_rust_target`
# over a fork that never ran. `nros_grep_q` exits 2 on rc>=2 instead.
# shellcheck source=../../../../scripts/lib/grep-q.sh
. "$REPO/scripts/lib/grep-q.sh"

WORK="$(mktemp -d "${TMPDIR:-/tmp}/nros-target-spelling.XXXXXX")"
trap 'rm -rf "$WORK"' EXIT

cat >"$WORK/CMakeLists.txt" <<'EOF'
cmake_minimum_required(VERSION 3.22)
project(nros_target_spelling NONE)

include("${NROS_REPO}/cmake/NanoRosCodegenCore.cmake")

_nros_resolve_rust_target(_t)
message(STATUS "NROS_PROBE_TRIPLE=[${_t}]")

# The resolver caches; a second call must agree with the first.
_nros_resolve_rust_target(_t2)
if(NOT _t STREQUAL "${_t2}")
    message(FATAL_ERROR "resolver is not idempotent: [${_t}] vs [${_t2}]")
endif()

if(NROS_PROBE_ZEPHYR)
    # The Zephyr module ALONE — it must reach the resolver through its own
    # include chain, and its unknown-arch fallback must NAME the host triple
    # instead of leaving the variable empty (empty meant "omit --target").
    include("${NROS_REPO}/zephyr/cmake/nros_cargo_build.cmake")
    nros_detect_rust_target()
    message(STATUS "NROS_PROBE_ZEPHYR_FALLBACK=[${NROS_RUST_TARGET}]")
    return()
endif()

if(NROS_PROBE_EMPTY_TARGET)
    # The rejected shape: "host build" spelled as no triple at all.
    _nros_ffi_cargo_args(_args MANIFEST /x/Cargo.toml TARGET_DIR /x/target
        PROFILE nros-minsizerel RUST_TARGET "")
else()
    _nros_ffi_cargo_args(_args MANIFEST /x/Cargo.toml TARGET_DIR /x/target
        PROFILE nros-minsizerel RUST_TARGET "${_t}" ${NROS_PROBE_EXTRA_ARG})
endif()
message(STATUS "NROS_PROBE_ARGS=[${_args}]")
EOF

fail=0
note() { echo "  $*"; }
bad() { echo "cargo-target-spelling: FAIL — $*" >&2; fail=1; }

# configure <label> <expect-pass|expect-fail> <needle> <PATH-override|-> [-D…]
configure() {
    local label=$1 expect=$2 needle=$3 pathov=$4; shift 4
    local log="$WORK/out.log"
    rm -rf "$WORK/b"
    local rc=0
    # nros-cmake-prefix-exempt: a synthetic NONE-language project this gate
    # writes itself; one of its scopes asserts NO Corrosion is present, which a
    # prefix-path export would defeat.
    if [ "$pathov" = "-" ]; then
        cmake -S "$WORK" -B "$WORK/b" -DNROS_REPO="$REPO" "$@" >"$log" 2>&1 || rc=$?
    else
        PATH="$pathov" cmake -S "$WORK" -B "$WORK/b" -DNROS_REPO="$REPO" "$@" >"$log" 2>&1 || rc=$?
    fi
    if [ "$expect" = "expect-pass" ]; then
        if [ "$rc" -ne 0 ]; then
            bad "$label: configure failed"
            sed 's/^/      /' <"$log" | tail -8 >&2
            return
        fi
        if ! nros_grep_q -F -- "$needle" "$log"; then
            bad "$label: expected $needle"
            grep -E "NROS_PROBE_" "$log" | sed 's/^/      /' >&2
            return
        fi
    else
        if [ "$rc" -eq 0 ]; then
            bad "$label: configure SUCCEEDED but must fail"
            return
        fi
        if ! nros_grep_q -F -- "$needle" "$log"; then
            bad "$label: failed for the wrong reason (no '$needle')"
            sed 's/^/      /' <"$log" | tail -8 >&2
            return
        fi
    fi
    note "ok — $label"
}

host_triple="$(rustc -vV 2>/dev/null | sed -n 's/^host: //p')"
if [ -z "$host_triple" ]; then
    echo "[SKIP] cargo-target-spelling: no rustc on PATH to name a host triple."
    echo "       source ./activate.sh first."
    exit 0
fi
if ! command -v cmake >/dev/null 2>&1; then
    echo "[SKIP] cargo-target-spelling: cmake not installed."
    exit 0
fi

# 1 — no Corrosion anywhere: the resolver must still name the HOST triple, and
#     the assembled argv must carry it.
configure "host build spells its own triple" expect-pass \
    "NROS_PROBE_ARGS=[build;--manifest-path;/x/Cargo.toml;--target-dir;/x/target;--profile;nros-minsizerel;--target;$host_triple]" \
    -

# 2 — a toolchain file's variable wins.
configure "toolchain Rust_CARGO_TARGET wins" expect-pass \
    "NROS_PROBE_TRIPLE=[armv7a-nuttx-eabihf]" \
    - -DRust_CARGO_TARGET=armv7a-nuttx-eabihf

# 3 — the phase-155 scope: ONLY Corrosion's cache copy is readable.
configure "corrosion cache copy is read when the normal var is gone" expect-pass \
    "NROS_PROBE_TRIPLE=[thumbv7m-none-eabi]" \
    - -DRust_CARGO_TARGET_CACHED=thumbv7m-none-eabi

# 4 — nothing to read AND no rustc: fail, do NOT fall back to implicit.
#     Reached by handing cmake a PATH that holds cmake and not rustc. Skipped
#     rather than faked when the two live in the same directory.
cmake_dir="$(dirname "$(command -v cmake)")"
# The PATH handed to cmake must hide `rustc` and still let cmake pick a
# generator: with no `make`/`ninja` on it, configure dies at
# "CMAKE_MAKE_PROGRAM is not set" BEFORE reaching the resolver, and the arm
# fails for the wrong reason. That is not hypothetical — a pip-installed cmake
# lives in its own `.../site-packages/cmake/data/bin`, which carries cmake and
# nothing else, so this arm broke `ci-matrix` on such a host while the rule it
# checks was perfectly fine.
make_dir=""
for prog in make gmake ninja; do
    if p="$(command -v "$prog" 2>/dev/null)"; then
        d="$(dirname "$p")"
        # Only usable if it does not smuggle rustc back in.
        if [ ! -x "$d/rustc" ]; then
            make_dir="$d"
            break
        fi
    fi
done
if [ -x "$cmake_dir/rustc" ]; then
    note "skip — cmake and rustc share $cmake_dir, cannot hide rustc"
elif [ -z "$make_dir" ]; then
    note "skip — no build program on a rustc-free PATH, cannot reach the resolver"
else
    configure "no triple available is fatal, not implicit" expect-fail \
        "cannot determine the cargo target triple" \
        "$cmake_dir:$make_dir"
fi

# 5 — the retired shape is rejected at the shared helper.
configure "an empty RUST_TARGET is rejected" expect-fail \
    "RUST_TARGET is required" \
    - -DNROS_PROBE_EMPTY_TARGET=ON

# 6 — the ONE legitimate omission of the FLAG keeps the triple.
configure "TARGET_IN_CONFIG omits the flag, not the triple" expect-pass \
    "NROS_PROBE_ARGS=[build;--manifest-path;/x/Cargo.toml;--target-dir;/x/target;--profile;nros-minsizerel]" \
    - -DNROS_PROBE_EXTRA_ARG=TARGET_IN_CONFIG

# 6b — issue 0553: a STALE memo must not outrank an explicit target.
#      `_NROS_RUST_TARGET` is a permanent `CACHE INTERNAL` entry that nothing
#      invalidates, and the resolver used to short-circuit on it FIRST — so a
#      build tree configured host-first answered "host" forever, across every
#      later reconfigure, because the memo lives in the cache and not in a
#      target dir a clean rebuild would remove. That is how a workspace whose
#      own cache said `armv7a-nuttx-eabihf` built its message FFI glue under
#      `x86_64-unknown-linux-gnu` and died at the ARM link with "file format not
#      recognized" — and, downstream, how `nros_nuttx_include_root()` saw a host
#      triple, matched no NuttX arch, and fell back to the shared tree (0551).
#
#      This arm hands the configure BOTH a poisoned memo and the real target.
#      The memo must lose. The gate had no memo coverage at all, which is
#      exactly why the precedence could be wrong for as long as it was.
configure "a stale memo loses to an explicit target" expect-pass \
    "NROS_PROBE_TRIPLE=[armv7a-nuttx-eabihf]" \
    - -DRust_CARGO_TARGET=armv7a-nuttx-eabihf \
    -D_NROS_RUST_TARGET:INTERNAL=x86_64-unknown-linux-gnu

# 6c — and with NOTHING explicit, the memo is still the answer: it exists to
#      spare a `rustc -vV` per call and to give a scope that cannot see the
#      normal variable a consistent reading. Demoting it must not disable it.
configure "the memo is still used when nothing explicit is visible" expect-pass \
    "NROS_PROBE_TRIPLE=[thumbv7m-none-eabi]" \
    - -D_NROS_RUST_TARGET:INTERNAL=thumbv7m-none-eabi

# 6d — the memo also outranks corrosion's cache copy, which is what makes the
#      demotion safe: `Rust_CARGO_TARGET_CACHED` was the HOST triple in the very
#      tree whose requested target was ARM, so letting it climb above the memo
#      would be the same bug facing the other way.
configure "the memo outranks corrosion's cache copy" expect-pass \
    "NROS_PROBE_TRIPLE=[thumbv7m-none-eabi]" \
    - -D_NROS_RUST_TARGET:INTERNAL=thumbv7m-none-eabi \
    -DRust_CARGO_TARGET_CACHED=x86_64-unknown-linux-gnu

# 7 — the second and third sites of the class. The Zephyr generators key on
#     NROS_RUST_TARGET, whose unknown-arch fallback used to be the empty string;
#     it must now name the host triple, and the module must reach the resolver
#     through its OWN include chain rather than the caller's.
configure "the zephyr unknown-arch fallback names the host triple" expect-pass \
    "NROS_PROBE_ZEPHYR_FALLBACK=[$host_triple]" \
    - -DNROS_PROBE_ZEPHYR=ON

if [ "$fail" -ne 0 ]; then
    echo "cargo-target-spelling: FAILED" >&2
    exit 1
fi
echo "cargo-target-spelling: all checks passed"
