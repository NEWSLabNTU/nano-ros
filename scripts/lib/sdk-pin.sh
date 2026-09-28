# shellcheck shell=bash
# Sourced, not executed — hence no shebang.
#
# The version `nros-sdk-index.toml` PINS for a `[tool.<name>]` — issue 1546.
#
# A provisioned tool lives at `<store>/<tool>/<version>` because `nros setup`
# put it there, having read `<version>` from the index. A consumer builds the
# same path from the same two inputs; it never lists the store and picks one.
# The store is SHARED between checkouts and ACCUMULATES (issue 0500), while the
# pin is per-checkout, so "the newest version present" is a sibling checkout's
# answer as often as it is ours (phase-365; `check-sdk-store-not-enumerated`).
#
# This is the shell reader of that pin. Its twins read the same key the same
# way, because the build systems cannot call each other:
#
#   cmake  — `nros_sdk_pin()` in cmake/NanoRosSdkPin.cmake
#   rust   — `nros_build_paths::sdk_pinned_version()`
#   CLI    — `sdk_store::tool_dir()` (what `nros sdk-path <tool>` prints)
#
# Where the `nros` CLI is available and the caller's store root is the CLI's,
# `nros sdk-path <tool>` is the better answer. This exists for the callers
# whose store root is `NROS_SDK_STORE` (the cross-toolchain family), which the
# CLI does not read, and for jobs that run before the CLI is built.
#
# Plain awk, not python: a `tomllib`-less python3 (stock 22.04) fails with
# empty output, which a caller cannot tell from "no pin" (issue 1264).

_NROS_SDK_PIN_LIB_DIR="$(cd "$(dirname "${BASH_SOURCE[0]:-$0}")" && pwd)"

# nros_sdk_pinned_version <tool> [<index>]
#
# Prints the pinned `version` of `[tool.<tool>]`, or nothing (status 1) when the
# index or the section is absent. The section runs from its header to the next
# line that STARTS a table — the same bound `nros_sdk_pin()` uses, so an
# inline array such as `smoke = [` inside the section does not end it.
nros_sdk_pinned_version() {
    local tool="${1:?nros_sdk_pinned_version: tool name}"
    local index="${2:-$_NROS_SDK_PIN_LIB_DIR/../../nros-sdk-index.toml}"
    [ -r "$index" ] || return 1
    local ver
    ver="$(awk -v hdr="[tool.$tool]" '
        $0 == hdr { inside = 1; next }
        /^\[/     { inside = 0 }
        inside && /^version[ \t]*=/ {
            v = $0
            sub(/^version[ \t]*=[ \t]*"/, "", v)
            sub(/".*$/, "", v)
            print v
            exit
        }
    ' "$index")"
    [ -n "$ver" ] || return 1
    printf '%s' "$ver"
}
