# Which Zephyr toolchain a board builds with — issue 0698.
#
# ONE rule, three callers (`zephyr-fixture-run-one.sh`, `west-fixtures.sh`,
# `just/zephyr-dev.just`). It used to be spelled three times, each saying
# "native_sim gets host, everything else says nothing" — and "says nothing" is
# the half that broke.
#
# WHY THE VARIANT MUST BE NAMED, NOT LEFT TO THE DEFAULT
#
# Zephyr 3.7 decides whether to look for its SDK with
#
#   if(("zephyr" STREQUAL ${ZEPHYR_TOOLCHAIN_VARIANT}) OR ...
#
# — the reference is UNQUOTED, so an unset variable expands to nothing and the
# condition becomes `if("zephyr" STREQUAL )`, missing its right operand. CMake
# 3.x tolerated that; CMake 4 rejects the whole `if()` as
# "Unknown arguments specified", and every SDK-toolchain board dies at
# configure. Measured on one snippet: 3.22.1 takes the branch, 4.4.2 errors.
#
# Naming the variant is therefore not a workaround for one cmake — it is the
# fix that lets ONE tree serve both, which this project needs: Ubuntu 22.04
# ships CMake 3.22 and ROS Humble is bound to it, while a rolling host is
# already on 4.x.
#
# `"zephyr"` is exactly the value Zephyr would have chosen for these boards, so
# both versions take the same branch and reach the same code:
#
#   3.22  unset -> `NOT DEFINED` true      | "zephyr" -> STREQUAL true
#   4.4   unset -> hard error              | "zephyr" -> STREQUAL true
#
# Setting it changes nothing else. With `ZEPHYR_SDK_INSTALL_DIR` still unset the
# lookup takes the same `else()` search branch as before, and the in-tree SDK is
# found the way it always was — through the CMake user package registry
# (`~/.cmake/packages/Zephyr-sdk/*`, written by `scripts/zephyr/setup.sh`),
# not through the hard-coded `/usr /opt $HOME` list, which never contained it.
# Verified: with the variant set and the SDK dir unset, an `mps2_an385`
# configure still reports `Found toolchain: zephyr 0.16.8`.
#
# Do NOT "fix" this by exporting `ZEPHYR_SDK_INSTALL_DIR` INSTEAD of the
# variant: it does not work (the whole `if()` argument list has to parse before
# any clause is evaluated, so the later `(DEFINED ZEPHYR_SDK_INSTALL_DIR)`
# clause cannot rescue the first one). `-DCMAKE_POLICY_VERSION_MINIMUM` does
# not work either — this is a parse error, not a policy. Exporting it AS WELL is
# a different matter, and since issue 1811 it is what an SDK board gets:
# `nros_zephyr_sdk_env` below.

# nros_zephyr_toolchain_variant <board>
#
# Prints the variant to build `<board>` with. Always non-empty, so a caller can
# export it unconditionally rather than reasoning about the unset case — that
# reasoning is what this file exists to hold.
#
#   caller's own       an externally-set `ZEPHYR_TOOLCHAIN_VARIANT` wins, so a
#                      third-party toolchain (gnuarmemb, llvm, cross-compile)
#                      still works. All three call sites already honoured this
#                      and it is preserved here.
#   native_sim*        `host` — native_sim is host gcc and needs no SDK, which
#                      is what lets an SDK-free host build that subset
#                      (issue 0087). Board-keyed, not version-keyed: native_sim
#                      on ANY Zephyr line takes it.
#   everything else    `zephyr` — the SDK. Includes the empty board (the FVP
#                      `board_import` entry, which names its board in
#                      `board.cmake`); it was SDK-gated before and stays so.
nros_zephyr_toolchain_variant() {
    local board="${1:-}"
    if [ -n "${ZEPHYR_TOOLCHAIN_VARIANT:-}" ]; then
        printf '%s' "$ZEPHYR_TOOLCHAIN_VARIANT"
        return 0
    fi
    case "$board" in
        native_sim | native_sim/* | native_sim*) printf 'host' ;;
        *) printf 'zephyr' ;;
    esac
}

# WHICH SDK — issue 1811.
#
# The registry search above is fine while the registry holds one SDK. It is
# wrong once it holds two, because Zephyr 3.7's `FindZephyr-sdk.cmake` sets
# `CMAKE_FIND_PACKAGE_SORT_ORDER NATURAL` / `DIRECTION DEC`: the HIGHEST
# registered version that satisfies the request wins, not the one the tree
# pins. The CI image bakes SDK 0.17.4 (for nothing in particular) and its jobs
# register it; `just setup` registers the pinned 0.16.8 beside it; a 3.7
# `mps2_an385` build then took 0.17.4, whose picolibc declares the retarget
# locks differently, and died in Zephyr's own `libc-hooks.c` with
# `conflicting types for '__lock___libc_recursive_mutex'` (live-peer run
# 38230566873). native_sim never noticed: it builds with host gcc.
#
# So an SDK board names its SDK: the version the Zephyr tree itself demands
# (`<zephyr>/SDK_VERSION`), resolved by the one resolver
# (`scripts/lib/zephyr-sdk.sh`, which honours a caller's
# `ZEPHYR_SDK_INSTALL_DIR` first). Nothing resolved ⇒ nothing exported, and the
# registry search runs as before — a host that provisioned the SDK some other
# way still builds.
#
# nros_zephyr_sdk_env <board> <zephyr tree>
#
# Prints `ZEPHYR_SDK_INSTALL_DIR=<sdk>` for an SDK board whose pinned SDK is
# installed, or nothing. Written as an `env` argument so a caller can splice it
# into the same array as the variant.
nros_zephyr_sdk_env() {
    local board="${1:-}" zephyr_tree="${2:-}" want sdk
    [ "$(nros_zephyr_toolchain_variant "$board")" = "zephyr" ] || return 0
    [ -n "$zephyr_tree" ] && [ -f "$zephyr_tree/SDK_VERSION" ] || return 0
    want="$(tr -d '[:space:]' < "$zephyr_tree/SDK_VERSION")"
    [ -n "$want" ] || return 0
    sdk="$(bash "$(dirname "${BASH_SOURCE[0]}")/../lib/zephyr-sdk.sh" --version "$want" resolve 2>/dev/null)" \
        || return 0
    [ -z "$sdk" ] || printf 'ZEPHYR_SDK_INSTALL_DIR=%s' "$sdk"
    return 0
}
