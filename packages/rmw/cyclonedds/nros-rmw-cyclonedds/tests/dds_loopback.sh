# shellcheck shell=bash
#
# The Cyclone DDS loopback pin for this directory's tests, in shell (issues
# 1009, 1139). Sourced, never run: by `dds_loopback_exec.sh`, the launcher
# every ctest entry here goes through, and by `ros2_e2e_common.sh`.
#
# The profile itself is a committed file shared with the Rust harness
# (`nros_tests::dds_isolation`), never a document spelled out here.

NROS_DDS_CYCLONE_LOOPBACK_XML="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../../../testing/nros-tests/dds" && pwd)/cyclone-loopback.xml"
if [ ! -f "$NROS_DDS_CYCLONE_LOOPBACK_XML" ]; then
    echo "dds_loopback.sh: no loopback profile at $NROS_DDS_CYCLONE_LOOPBACK_XML" >&2
    return 1
fi

# issue 1139 / issue 1009 -- confine this pair's DDS bus to loopback.
#
# Both scripts in this directory carried their OWN copy of a Cyclone config
# that pinned the bus to a real ETHERNET interface with multicast left on,
# which is the LAN. Every other DDS lane in this repo is confined to loopback
# by `nros_tests::dds_isolation`, and these two cells sat outside that helper's
# reach for one reason only: they are shell, not Rust. So a foreign participant
# on another host -- the thing that cost issue 0741 five wrong diagnoses and 4
# of 15 failures -- is a peer of this cell and of no other DDS lane here.
#
# The recipe is the ONE committed profile, `NROS_DDS_CYCLONE_LOOPBACK_XML`
# below, which `nros_tests::dds_isolation` also reads (it was a hand-kept copy
# of the Rust string until the ctest suite needed it too, and three copies is
# how a profile drifts): `AllowMulticast=false` PLUS an explicit localhost peer. The
# interface pin alone still leaves SPDP announcing to a multicast group any
# participant on the LAN can join, so it is the pair that does the work.
#
# Symmetric by construction, which issue 1137 says is the whole game:
# `CYCLONEDDS_URI` is EXPORTED from this shell, so the `ros2` CLI peer and our
# own binaries both read the same file. Half a pin is no discovery, not a
# half-isolated bus.
#
# Two escapes, both copied from the Rust helper rather than invented here:
#   * `NROS_DDS_ALLOW_LAN=1` -- keep the old ethernet-interface config, for a
#     peer that must genuinely reach another host.
#   * an operator's own `CYCLONEDDS_URI` is never overwritten -- somebody
#     debugging a transport has a reason, and a harness that clobbers it makes
#     the debugging session lie. (The old inline config overwrote it.)
#
# `NROS_E2E_LOOPBACK_ADDRESS` / `NROS_E2E_LOOPBACK_PEER` exist for ONE purpose:
# the negative control. Issue 1009's measurement is the model -- a config that
# still delivers proves nothing on its own, because an INERT config delivers
# too. Point these at an unroutable address (1009 used `10.255.255.254`) and
# the cell must FAIL; that failure is what makes the passing run mean the file
# is read and the pin is real. Never set them to isolate something for real.
#
# Usage: `nros_export_cyclone_config <path-to-write>` (written only for the
# negative control or the LAN path). Exports CYCLONEDDS_URI
# and echoes one line saying what it chose. Returns 1, having echoed a
# `[SKIPPED]` reason, only on the LAN-allowed path with no usable interface.

# Pick a multicast-capable interface for SPDP. Reachable only on the
# `NROS_DDS_ALLOW_LAN=1` path now -- `lo` is not multicast capable on Linux,
# which is why a LAN config needs a real ethernet interface, and is also why
# the loopback config below must switch multicast off rather than pick `lo`.
# Override via NROS_RMW_CYCLONEDDS_E2E_IFACE if the auto-pick is wrong.
nros_e2e_iface() {
    if [ -n "${NROS_RMW_CYCLONEDDS_E2E_IFACE:-}" ]; then
        printf '%s\n' "$NROS_RMW_CYCLONEDDS_E2E_IFACE"; return 0
    fi
    ip -o -br link show | awk '
        /BROADCAST,MULTICAST/ && / UP / &&
        $1 !~ /^(docker|veth|tap|qemu|tailscale)/ { print $1; exit }'
}

nros_export_cyclone_config() {
    local path="$1"
    local iface

    if [ -n "${CYCLONEDDS_URI:-}" ]; then
        echo "  using operator CYCLONEDDS_URI=$CYCLONEDDS_URI"
        return 0
    fi

    if [ -n "${NROS_DDS_ALLOW_LAN:-}" ]; then
        iface=$(nros_e2e_iface)
        if [ -z "$iface" ]; then
            echo "[SKIPPED] NROS_DDS_ALLOW_LAN=1 and no multicast-capable ethernet interface for SPDP"
            return 1
        fi
        {
            echo '<?xml version="1.0" encoding="UTF-8" ?>'
            echo '<CycloneDDS xmlns="https://cdds.io/config">'
            echo '  <Domain id="any">'
            echo '    <General>'
            echo '      <Interfaces>'
            echo "        <NetworkInterface name=\"$iface\" priority=\"default\" multicast=\"default\" />"
            echo '      </Interfaces>'
            echo '    </General>'
            echo '  </Domain>'
            echo '</CycloneDDS>'
        } > "$path"
        export CYCLONEDDS_URI="file://$path"
        echo "  NROS_DDS_ALLOW_LAN=1 -- bus on the LAN via interface=$iface"
        return 0
    fi

    # The committed profile, as is; a rewritten copy only for the negative
    # control below, which must change the very lines that do the pinning.
    if [ -z "${NROS_E2E_LOOPBACK_ADDRESS:-}" ] && [ -z "${NROS_E2E_LOOPBACK_PEER:-}" ]; then
        export CYCLONEDDS_URI="file://$NROS_DDS_CYCLONE_LOOPBACK_XML"
    else
        sed -e "s|address=\"127.0.0.1\"|address=\"${NROS_E2E_LOOPBACK_ADDRESS:-127.0.0.1}\"|" \
            -e "s|<Peer address=\"localhost\"/>|<Peer address=\"${NROS_E2E_LOOPBACK_PEER:-localhost}\"/>|" \
            "$NROS_DDS_CYCLONE_LOOPBACK_XML" > "$path"
        export CYCLONEDDS_URI="file://$path"
    fi
    echo "  bus pinned to loopback (issue 1009; NROS_DDS_ALLOW_LAN=1 to opt out)"
    return 0
}
