#!/usr/bin/env bash
# The ONE Zephyr SDK resolver — RFC-0095 D2, issue 1254, phase-449 W3.
#
# The sibling of `scripts/lib/zephyr-workspace.sh`, for the other half of the
# same provisioning. The workspace moved to the store in phase-440 W4; the SDK
# did not, and until this file existed the checkout-relative path was
# constructed in seven places (the sweep is in issue 1254):
#
#   scripts/zephyr/setup.sh:73          the install target, `$SCRIPT_DIR/sdk`
#   just/zephyr-setup.just  x4          `$(pwd)/scripts/zephyr/sdk/zephyr-sdk-0.16.8`
#   packages/testing/nros-tests/src/zephyr.rs  globs `zephyr-sdk-*` under it
#   scripts/ci/runner-doctor.sh:198     the ladder's "checkout default" arm
#
# and the `0.16.8` in four of those is a SECOND copy of the per-line version
# that `setup.sh`'s `case "$MANIFEST"` already owns.
#
# ## What that cost
#
# `setup.sh` passed `--prefix "$SCRIPT_DIR/sdk"` to `nros setup --tool`, and
# `--prefix` is the documented out-of-store escape hatch — a prefix install is
# not recorded in `nros-sdk.lock`, so `nros sdk-path` cannot find it and
# `nros store gc` does not know it exists. A downstream project's generated
# `env.sh` then carried
#
#   export ZEPHYR_SDK_INSTALL_DIR=".../nano-ros/scripts/zephyr/sdk/zephyr-sdk-1.0.1"
#
# naming a SIBLING clone. Every board build of that project depended on a second
# checkout existing and on nobody running `just clean-setup` there — which
# removes exactly that directory. Nothing reported the dependency.
#
# ## The ladder, and why the store is FIRST here
#
# `zephyr-workspace.sh` documents its store arm as LAST, because promoting it
# would have moved every already-provisioned host at once. This one is the
# reverse and for the same reason: the store arm is only taken when the store
# copy EXISTS, so a host provisioned before this keeps resolving to its checkout
# copy and keeps building. Nothing moves until something installs to the store.
#
# A resolved path is not an installed tree — `nros sdk-path zephyr-sdk` happily
# prints `~/.nros/sdk/zephyr-sdk/0.16.8` on a host that has never installed it.
# So every arm here is EXISTENCE-CHECKED, and `default` (the install target) is
# the one caller that deliberately wants the unchecked answer.
#
# Usable BOTH ways: `source` it for the functions, or run it as a command —
# a `just` expression and a shell function cannot share code, so `just` calls
# it rather than restating the path.
#
#   scripts/lib/zephyr-sdk.sh [--version V] [--root DIR] <resolve|default|store-dir|checkout-dir|version|tool>
#
# shellcheck shell=bash

nros_zephyr_sdk_default_root() {
    (cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
}

# The SDK version for the selected Zephyr line. ONE table, the same split
# `scripts/zephyr/setup.sh` makes from `case "$MANIFEST"`: the 3.7 LTS line
# wants 0.16.8, the 4.4 line wants 1.0.1. Callers that already know the version
# pass it; nobody should spell the mapping again.
nros_zephyr_sdk_version() {
    case "${NROS_ZEPHYR_VERSION:-3.7}" in
        4.4) printf '%s\n' "1.0.1" ;;
        *)   printf '%s\n' "0.16.8" ;;
    esac
}

# The `[tool.*]` key in `nros-sdk-index.toml` for that line. Two entries exist
# because the two lines are two different downloads.
nros_zephyr_sdk_tool() {
    case "${NROS_ZEPHYR_VERSION:-3.7}" in
        4.4) printf '%s\n' "zephyr-sdk-1-0-1" ;;
        *)   printf '%s\n' "zephyr-sdk" ;;
    esac
}

# Same resolution order as `nros_zephyr_ws_store_root`, deliberately — one
# store, one spelling of where it is.
nros_zephyr_sdk_store_root() {
    printf '%s\n' "${NROS_STORE:-${NROS_HOME:-$HOME/.nros}}"
}

# `$NROS_STORE/sdk/<tool>/<version>/zephyr-sdk-<version>`.
#
# The trailing component is not decoration: the tarball carries a top-level
# directory and is unpacked WITHOUT `--strip-components`, so the store holds
# `<version>/zephyr-sdk-<version>/`. `ZEPHYR_SDK_INSTALL_DIR` must name that
# inner directory.
#
# issue 1259 — `nros sdk-path` NOW ANSWERS THAT DIRECTORY. The index states the
# tarball's own top-level directory as `[tool.*] subdir`, so the layout is one
# fact in the file that already owns the URL and the checksum. This function
# therefore stops appending: "the one thing a consumer cannot get from
# `nros sdk-path` alone" (issue 1254) was a gap in the CLI, and appending here
# was a second copy of the pin living in a shell script — one bump away from
# naming a directory nothing installed.
nros_zephyr_sdk_store_dir() {
    local version="${1:-}" tool="${2:-}" base=""
    [ -n "$version" ] || version="$(nros_zephyr_sdk_version)"
    [ -n "$tool" ] || tool="$(nros_zephyr_sdk_tool)"

    # ASK the CLI rather than restating its layout. `nros sdk-path` is the
    # producer of that answer, and it answers without touching the disk, so an
    # absent SDK still resolves.
    if command -v nros >/dev/null 2>&1; then
        base="$(nros sdk-path "$tool" 2>/dev/null || true)"
    fi
    if [ -n "$base" ]; then
        printf '%s\n' "$base"
        return 0
    fi

    # The fallback is for the one context that has no CLI yet: early
    # provisioning, and `runner-doctor` on a host where `nros` is not on PATH.
    # It is the same layout — INCLUDING the subdir, which the CLI's answer
    # already carries — and it is NOT the primary answer.
    printf '%s\n' \
        "$(nros_zephyr_sdk_store_root)/sdk/$tool/$version/zephyr-sdk-$version"
}

# The legacy in-checkout location. Kept as an ARM, not as a default: a host
# provisioned before issue 1254 has its SDK here and must keep building.
nros_zephyr_sdk_checkout_dir() {
    local version="${1:-}" root="${2:-}"
    [ -n "$version" ] || version="$(nros_zephyr_sdk_version)"
    [ -n "$root" ] || root="$(nros_zephyr_sdk_default_root)"
    printf '%s\n' "$root/scripts/zephyr/sdk/zephyr-sdk-$version"
}

# THE LADDER. Prints the first arm that EXISTS, or nothing.
#
#   1. `ZEPHYR_SDK_INSTALL_DIR` — the operator override, both spellings seen in
#      the wild (the SDK itself, or its parent), the same two
#      `runner-doctor.sh` already accepts.
#   2. the store   — RFC-0095 D2, where provisioning puts it from now on.
#   3. the checkout — legacy, so an older host is not stranded.
nros_zephyr_sdk_resolve() {
    local version="${1:-}" root="${2:-}" cand
    [ -n "$version" ] || version="$(nros_zephyr_sdk_version)"

    if [ -n "${ZEPHYR_SDK_INSTALL_DIR:-}" ]; then
        case "$(basename "${ZEPHYR_SDK_INSTALL_DIR%/}")" in
            zephyr-sdk-*) cand="${ZEPHYR_SDK_INSTALL_DIR%/}" ;;
            *)            cand="${ZEPHYR_SDK_INSTALL_DIR%/}/zephyr-sdk-$version" ;;
        esac
        [ -d "$cand" ] && { printf '%s\n' "$cand"; return 0; }
    fi

    cand="$(nros_zephyr_sdk_store_dir "$version")"
    [ -d "$cand" ] && { printf '%s\n' "$cand"; return 0; }

    cand="$(nros_zephyr_sdk_checkout_dir "$version" "$root")"
    [ -d "$cand" ] && { printf '%s\n' "$cand"; return 0; }

    return 1
}

# Where provisioning INSTALLS — the store arm, existence-unchecked, because it
# is the answer to "where should this go" rather than "where is it".
nros_zephyr_sdk_default() {
    nros_zephyr_sdk_store_dir "${1:-}"
}

if [ "${BASH_SOURCE[0]}" = "$0" ]; then
    set -uo pipefail
    _v="" _root=""
    while [ "$#" -gt 0 ]; do
        case "$1" in
            --version) _v="$2"; shift 2 ;;
            --root)    _root="$2"; shift 2 ;;
            *) break ;;
        esac
    done
    case "${1:-resolve}" in
        resolve)      nros_zephyr_sdk_resolve "$_v" "$_root" ;;
        default)      nros_zephyr_sdk_default "$_v" ;;
        store-dir)    nros_zephyr_sdk_store_dir "$_v" ;;
        checkout-dir) nros_zephyr_sdk_checkout_dir "$_v" "$_root" ;;
        version)      [ -n "$_v" ] && printf '%s\n' "$_v" || nros_zephyr_sdk_version ;;
        tool)         nros_zephyr_sdk_tool ;;
        *) echo "zephyr-sdk.sh: unknown subcommand '${1:-}'" >&2; exit 2 ;;
    esac
fi
