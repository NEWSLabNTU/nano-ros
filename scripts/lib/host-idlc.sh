# shellcheck shell=bash
# Sourced, not executed — hence no shebang.
#
# Is there a host Cyclone `idlc`? — issue 1777.
#
# A Cyclone image's configure FATALs without one (`host Cyclone idlc not found`
# in `zephyr/cmake/nros_rmw_cyclonedds.cmake`), so a lane that is about to
# configure a Cyclone row asks HERE first and fails before configuring, with the
# remedy, instead of N configure logs later. Each lane used to spell the
# question itself, and the spellings disagreed with the configure they guard:
# one enumerated `<store>/cyclonedds/*/bin` (a SIBLING checkout's pin can answer
# — issue 1546), none honoured `IDLC_EXECUTABLE`, which the configure reads
# first. So a lane could refuse a host the configure accepts, and the reverse.
#
# The ORDER mirrors that configure's search, so the two cannot answer
# differently about the same host:
#
#   1. $IDLC_EXECUTABLE, when it names an existing file
#   2. the SDK store's PINNED cyclonedds: <store>/sdk/cyclonedds/<pin>/bin/idlc
#      (constructed from the index pin, never enumerated — issue 1546)
#   3. host PATH (a ROS 2 / CycloneDDS install)
#   4. <repo>/build/cyclonedds/bin, <repo>/build/install/bin (last resort)
#
# No `nros` CLI is needed: the store path is built from `store-root.sh` +
# `sdk-pin.sh`, the shell twins of what `nros sdk-path cyclonedds` prints.

_NROS_HOST_IDLC_LIB_DIR="$(cd "$(dirname "${BASH_SOURCE[0]:-$0}")" && pwd)"
# shellcheck source=scripts/lib/sdk-pin.sh
. "$_NROS_HOST_IDLC_LIB_DIR/sdk-pin.sh"
# shellcheck source=scripts/lib/store-root.sh
. "$_NROS_HOST_IDLC_LIB_DIR/store-root.sh"

# nros_host_idlc [<repo-root>]
#
# Prints the idlc a Cyclone configure would use; status 1 (nothing printed)
# when there is none.
nros_host_idlc() {
    local root="${1:-$_NROS_HOST_IDLC_LIB_DIR/../..}"
    local cand pin store
    if [ -n "${IDLC_EXECUTABLE:-}" ] && [ -e "$IDLC_EXECUTABLE" ]; then
        printf '%s\n' "$IDLC_EXECUTABLE"
        return 0
    fi
    if pin="$(nros_sdk_pinned_version cyclonedds "$root/nros-sdk-index.toml")" &&
        store="$(nros_store_root 2>/dev/null)"; then
        cand="$store/sdk/cyclonedds/$pin/bin/idlc"
        if [ -x "$cand" ]; then
            printf '%s\n' "$cand"
            return 0
        fi
    fi
    if cand="$(command -v idlc 2>/dev/null)"; then
        printf '%s\n' "$cand"
        return 0
    fi
    for cand in "$root/build/cyclonedds/bin/idlc" "$root/build/install/bin/idlc"; do
        if [ -x "$cand" ]; then
            printf '%s\n' "$cand"
            return 0
        fi
    done
    return 1
}

# nros_host_idlc_missing_reason <board> <what>
#
# The one sentence every lane prints when `nros_host_idlc` finds nothing: names
# the tool, what needed it, and the provisioning command.
nros_host_idlc_missing_reason() {
    local board="${1:?nros_host_idlc_missing_reason: board}"
    local what="${2:?nros_host_idlc_missing_reason: what needs it}"
    printf '%s need a host Cyclone `idlc` and none was found (IDLC_EXECUTABLE, the SDK store pin, PATH, build/{cyclonedds,install}/bin) — run `nros setup %s --rmw cyclonedds` (its dist ships bin/idlc), install ROS 2, or set IDLC_EXECUTABLE' \
        "$what" "$board"
}
