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
#   ./scripts/check-zenoh-archive-symbols.sh [path/to/libnros_rmw_zenoh_staticlib.a]
#
# Default path is `target/release/libnros_rmw_zenoh_staticlib.a` (the
# Corrosion-emitted staticlib produced by
# `cargo build -p nros-rmw-zenoh-staticlib --release`).

set -euo pipefail

ARCHIVE="${1:-target/release/libnros_rmw_zenoh_staticlib.a}"

if [[ ! -f "$ARCHIVE" ]]; then
    echo "error: archive not found at $ARCHIVE" >&2
    echo "       run \`cargo build -p nros-rmw-zenoh-staticlib --release\` first" >&2
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
    #
    # Serial is special: zenoh-pico's `src/link/unicast/serial.c`
    # wrappers (`_z_f_link_open_serial`, `_z_f_link_listen_serial`)
    # call `_z_open_serial_from_{dev,pins}` /
    # `_z_listen_serial_from_{dev,pins}` rather than a single
    # `_z_open_serial`; `_z_f_link_read_socket_serial` calls
    # `_z_read_serial_internal`; `write` maps to
    # `_z_send_serial_internal`. Encode the divergence so the
    # parity check measures the right contract.
    missing=()
    for w in "${WRAPPERS[@]}"; do
        op="${w#_z_f_link_}"; op="${op%_$t}"
        case "$op" in
            free|write_all)
                continue
                ;;
        esac
        # Each wrapper may map to one or more candidate impls; the
        # parity check passes if ANY candidate is defined as `T`.
        impls=()
        if [[ "$t" == "serial" ]]; then
            case "$op" in
                open|listen)
                    impls=("_z_${op}_serial_from_dev" "_z_${op}_serial_from_pins")
                    ;;
                read_socket)
                    impls=("_z_read_serial_internal")
                    ;;
                close)
                    impls=("_z_close_serial")
                    ;;
                write)
                    impls=("_z_send_serial_internal")
                    ;;
                *)
                    impls=("_z_${op}_${t}")
                    ;;
            esac
        else
            case "$op" in
                write)
                    impls=("_z_send_${t}")
                    ;;
                *)
                    impls=("_z_${op}_${t}")
                    ;;
            esac
        fi
        found=0
        for impl in "${impls[@]}"; do
            if grep -qE " T ${impl}$" "$NM_TMP"; then
                found=1
                break
            fi
        done
        if (( ! found )); then
            missing+=("${impls[*]} (wrapper $w)")
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
