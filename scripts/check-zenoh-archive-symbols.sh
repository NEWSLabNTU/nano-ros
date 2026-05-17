#!/usr/bin/env bash
# Phase 134.5 — verify libnros_rmw_zenoh.a wrapper/impl symbol parity
#
# For every defined `_z_f_link_*_<transport>` wrapper symbol in the
# installed archive, the matching `_z_*_<transport>` impl symbol must
# also be defined. Pre-Phase-134 the POSIX CMake path deleted upstream's
# `system/unix/network.c` from its build copy without providing
# replacements in `platform_aliases.c`, so the multicast wrappers
# linked against undefined impls. Phase 134's header-canonical
# contract + the new multicast aliases close the gap; this script
# regression-guards the contract permanently.
#
# Usage:
#   ./scripts/check-zenoh-archive-symbols.sh [path/to/libnros_rmw_zenoh.a]
#
# Default path is `build/install/lib/libnros_rmw_zenoh.a` (the install
# tree produced by `just install-rmw-zenoh`).

set -euo pipefail

ARCHIVE="${1:-build/install/lib/libnros_rmw_zenoh.a}"

if [[ ! -f "$ARCHIVE" ]]; then
    echo "error: archive not found at $ARCHIVE" >&2
    echo "       run \`just install-rmw-zenoh\` first" >&2
    exit 2
fi

# Transports we gate. Bluetooth/raweth are intentionally off across every
# `LinkPolicy` today; if a transport ships fully off, BOTH wrapper and
# impl are absent from the archive — also acceptable, just not divergent.
TRANSPORTS=(tcp udp_unicast udp_multicast serial ivc)

# Dump nm output to a tempfile and grep the file rather than piping.
# `set -o pipefail` + `grep -q` on a large nm dump triggers SIGPIPE
# (exit 141) because grep closes the pipe on first match while nm
# is still writing; pipefail then propagates the SIGPIPE. File-based
# grep avoids the race entirely. Suppresses the harmless "no symbols"
# stderr nm emits for empty rcgu objects compiler_builtins ships
# inside the archive.
NM_TMP="$(mktemp)"
trap 'rm -f "$NM_TMP"' EXIT
nm "$ARCHIVE" 2>/dev/null >"$NM_TMP"

fail=0

for t in "${TRANSPORTS[@]}"; do
    # `_z_f_link_*_<t>` wrappers — defined when the transport is on
    # for the build.
    mapfile -t WRAPPERS < <(
        grep -E " T _z_f_link_[a-z_]+_${t}$" "$NM_TMP" \
            | awk '{print $3}' | sort -u
    )
    if (( ${#WRAPPERS[@]} == 0 )); then
        echo "ok: $t — wrappers absent (transport off in this build)"
        continue
    fi

    # For each wrapper we must find a matching impl. Wrappers are
    # `_z_f_link_<op>_<t>` and impls are `_z_<op>_<t>` (drop the
    # `_f_link` infix). `free` / `write_all` are wrapper-only —
    # they have no underlying `_z_*_<t>` impl in zenoh-pico (they
    # call other wrappers internally), so skip them in the parity
    # gate. `write` maps to `send` on the impl side.
    missing=()
    for w in "${WRAPPERS[@]}"; do
        op="${w#_z_f_link_}"; op="${op%_$t}"
        case "$op" in
            free|write_all)
                continue
                ;;
            write)
                impl="_z_send_${t}"
                ;;
            *)
                impl="_z_${op}_${t}"
                ;;
        esac
        # Defined if at least one row is `T impl` (text section).
        if ! grep -qE " T ${impl}$" "$NM_TMP"; then
            missing+=("$impl (wrapper $w)")
        fi
    done

    if (( ${#missing[@]} > 0 )); then
        echo "FAIL: $t — wrappers defined but impls missing:" >&2
        printf '  - %s\n' "${missing[@]}" >&2
        fail=1
    else
        echo "ok: $t — ${#WRAPPERS[@]} wrappers, all impls defined"
    fi
done

if (( fail != 0 )); then
    echo "" >&2
    echo "  Archive: $ARCHIVE" >&2
    echo "  Regression of Phase 134's header-canonical contract." >&2
    echo "  Inspect with: nm $ARCHIVE | grep _z_" >&2
    exit 1
fi

echo ""
echo "zenoh archive symbol parity: clean"
