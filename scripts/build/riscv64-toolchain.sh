# The riscv64 bare-metal toolchain PREFIX — issue 0657.
#
# `[board.qemu-riscv64-threadx]` in `nros-sdk-index.toml` provisions
# `riscv-none-elf-gcc` (the xPack build, with `dist.<host>` rows for
# linux-x86_64, linux-arm64 and macos-arm64 — it is the portable choice, and the
# one `nros setup` actually installs on every supported host). Twenty files then
# spelled the compiler `riscv64-unknown-elf-*`, which is Ubuntu's
# `gcc-riscv64-unknown-elf` package and nothing else.
#
# So a host provisioned entirely by `nros setup` could not build this platform:
# the lane's precondition looked for a binary provisioning never installs, found
# nothing, and skipped — reporting OK until issue 0650 made that visible.
#
# ONE resolver, in the three spellings its callers can consume:
#
#   shell  — this file (`nros_riscv64_prefix`, `nros_riscv64_tool`)
#   cmake  — `_nros_riscv64_find_prefix()` in cmake/toolchain/riscv64-threadx.cmake
#   rust   — `nros_build_helpers::riscv64::prefix()`
#
# All three read the same candidate order, and all three honour
# `NROS_RISCV64_PREFIX` first so a host with a fourth toolchain can say so
# without a patch.
#
# Order: the SDK store before `PATH`, because the store holds the version the
# index PINS — the same rule issue 0500 established for Corrosion, where a stale
# `PATH` copy shadowing the pinned one cost an afternoon.

# nros_riscv64_prefix
#
# Prints the tool prefix (e.g. `riscv-none-elf`), or nothing when no toolchain
# is present. Never guesses: a caller that gets an empty string must skip or
# fail, and say which.
nros_riscv64_prefix() {
    if [ -n "${NROS_RISCV64_PREFIX:-}" ]; then
        printf '%s' "$NROS_RISCV64_PREFIX"
        return 0
    fi

    # 1. the SDK store, newest version first (`sort -Vr`, the 0500 rule).
    local store="${NROS_SDK_STORE:-$HOME/.nros/sdk}/riscv-none-elf-gcc"
    if [ -d "$store" ]; then
        local ver
        for ver in $(ls -1 "$store" 2>/dev/null | sort -Vr); do
            if [ -x "$store/$ver/bin/riscv-none-elf-gcc" ]; then
                printf 'riscv-none-elf'
                return 0
            fi
        done
    fi

    # 2. whatever is on PATH, in the order a bare-metal rv64 build can use.
    local cand
    for cand in riscv-none-elf riscv64-unknown-elf riscv64-none-elf riscv64-elf; do
        if command -v "${cand}-gcc" >/dev/null 2>&1; then
            printf '%s' "$cand"
            return 0
        fi
    done
    return 0
}

# nros_riscv64_bindir
#
# The directory holding the resolved prefix's binaries, when it came from the
# SDK store (empty when it came from `PATH`, where the caller needs no hint).
nros_riscv64_bindir() {
    local store="${NROS_SDK_STORE:-$HOME/.nros/sdk}/riscv-none-elf-gcc"
    [ -d "$store" ] || return 0
    local ver
    for ver in $(ls -1 "$store" 2>/dev/null | sort -Vr); do
        if [ -x "$store/$ver/bin/riscv-none-elf-gcc" ]; then
            printf '%s' "$store/$ver/bin"
            return 0
        fi
    done
    return 0
}

# nros_riscv64_tool <suffix>
#
# An absolute path when the toolchain came from the store, a bare name when it
# came from `PATH`. Empty when there is none — check before use.
nros_riscv64_tool() {
    local suffix="${1:?nros_riscv64_tool: suffix (gcc, g++, ar, …)}"
    local prefix bindir
    prefix="$(nros_riscv64_prefix)"
    [ -n "$prefix" ] || return 0
    bindir="$(nros_riscv64_bindir)"
    if [ -n "$bindir" ] && [ -x "$bindir/${prefix}-${suffix}" ]; then
        printf '%s' "$bindir/${prefix}-${suffix}"
    else
        printf '%s' "${prefix}-${suffix}"
    fi
}

# nros_riscv64_export_cc
#
# cc-rs reads `CC_<target-with-underscores>`, and rustc needs a linker for the
# same target. Exported here so the lane's cargo invocations and the cmake one
# agree on a compiler — before this, cc-rs fell back to its built-in default for
# `riscv64gc-unknown-none-elf`, which IS `riscv64-unknown-elf-gcc`.
nros_riscv64_export_cc() {
    local gcc ar
    gcc="$(nros_riscv64_tool gcc)"
    [ -n "$gcc" ] || return 0
    ar="$(nros_riscv64_tool ar)"
    export CC_riscv64gc_unknown_none_elf="$gcc"
    export CXX_riscv64gc_unknown_none_elf="$(nros_riscv64_tool g++)"
    export AR_riscv64gc_unknown_none_elf="$ar"
    # The ABI, explicitly. A multilib riscv toolchain picks a DEFAULT `-march`/
    # `-mabi` when told neither, and the two toolchains that build this board
    # pick differently — so cc-rs objects came out with one float ABI while the
    # cmake side passed `-mabi=lp64d`, and lld refused the link with "cannot
    # link object files with different floating-point ABI". Naming the ABI on
    # both sides is the fix; inheriting a default on either is the bug.
    # Must stay identical to CMAKE_{C,CXX,ASM}_FLAGS_INIT in
    # cmake/toolchain/riscv64-threadx.cmake.
    local _abi="-march=rv64gc -mabi=lp64d -mcmodel=medany"
    export CFLAGS_riscv64gc_unknown_none_elf="$_abi"
    export CXXFLAGS_riscv64gc_unknown_none_elf="$_abi"

    # NOT the linker. Rust images on this target link with `rust-lld`, and the
    # link args they pass (`--nmagic`, `--gc-sections`) are lld's own spelling —
    # routing them through gcc fails with "unrecognized command-line option".
    # cc-rs needs the cross compiler; rustc does not.
}
