#!/usr/bin/env bash
# Shared helpers for the ROS 2 interop e2e scripts in this directory.
#
# Sourced, never executed. `add_test` runs these scripts from
# `CMAKE_CURRENT_SOURCE_DIR`, so a sibling file is always present.

# issue 0580 — pick a ROS domain no CONCURRENT copy of this test will pick.
#
# These scripts used to hardcode a fallback (`${ROS_DOMAIN_ID:-117}` for pubsub,
# 118 for services). A fixed domain is a shared bus: two overlapping runs
# discover each other's writers, and the failure reads as a delivery bug rather
# than a collision. Observed under a tier-2 sweep — the subscriber captured
# `hello-from-nros`, which is the payload the OTHER copy's case-A.1 publisher
# emits, while solo runs of the same suite passed 17/17 twice.
#
# Mirrors `nros_tests::unique_ros_domain_id` (packages/testing/nros-tests/src/lib.rs)
# rather than inventing a second scheme: prefer nextest's global slot when the
# runner provides one, else the pid, folded into 1..=232 (0 is the default
# everyone else lands on, and the ROS 2 range tops out well below 255).
#
# An explicit `ROS_DOMAIN_ID` in the environment still wins — pinning one is how
# you reproduce a failure by hand.
# issue 0703 — the modulus is 101, shared with the Rust and C++ assigners.
# Cyclone derives its RTPS ports from the domain (`7400 + 250*D`), and Linux
# hands out ephemeral ports from 32768, so from domain 102 up
# (`7400 + 250*102 = 32900`) the port a participant must have is one the OS may
# already have given away — the bind fails and the session never opens. See
# `nros_test_domain.h` for the measurement.
NROS_TEST_DOMAIN_MAX=101

# issue 0747 — probe before taking the bus, the third of the three assigners.
#
# Issue 0707 taught the Rust one to read /proc/net/udp and step past an occupied
# domain, and the header above has claimed "one scheme" throughout — but the fix
# landed in Rust only, so C++ and this file kept picking blind. Measured cost on
# 2026-08-21: `check-rmw-cyclonedds` 3 red in ~6 in-sweep runs, 0 red in 3 solo,
# a different test each time, once printing `failed to bind to ANY:8650: address
# in use` (domain 5). With 101 buckets and a 32-way fan-out over hundreds of
# short-lived processes on sequential PIDs, a collision inside one sweep is the
# expected case.
#
# `false` when /proc is unreadable: unknown must not read as busy, or every
# domain looks taken and the stepping degrades to its fallback for nothing.
nros_domain_busy() {
    local domain="$1"
    local want
    want=$(printf '%04X' $(( 7400 + 250 * domain )))
    local table
    for table in /proc/net/udp /proc/net/udp6; do
        [ -r "$table" ] || continue
        # Column 2 is `HEXADDR:HEXPORT`. Compared as UPPERCASE HEX TEXT rather
        # than converted to a number, because `strtonum` is a gawk extension and
        # Ubuntu's default awk is mawk — there it is a fatal error, awk exits
        # non-zero, and this function would answer "not busy" for every domain
        # on the very hosts CI runs. /proc prints the port as fixed 4-digit
        # uppercase hex, which `printf '%04X'` matches exactly.
        if awk -v want="$want" 'NR > 1 {
                split($2, a, ":")
                if (a[2] == want) { found = 1; exit }
            }
            END { exit(found ? 0 : 1) }' "$table"; then
            return 0
        fi
    done
    return 1
}

nros_unique_ros_domain_id() {
    local first
    if [ -n "${NEXTEST_TEST_GLOBAL_SLOT:-}" ]; then
        first=$(( (NEXTEST_TEST_GLOBAL_SLOT % NROS_TEST_DOMAIN_MAX) + 1 ))
    else
        first=$(( ($$ % NROS_TEST_DOMAIN_MAX) + 1 ))
    fi
    if ! nros_domain_busy "$first"; then
        echo "$first"
        return 0
    fi
    # Bounded step, then give up and return the first candidate: a box where
    # every domain looks busy is not something this function can fix, and
    # returning nothing would break every caller (issue 0707's contract).
    local step candidate
    for (( step = 1; step <= NROS_TEST_DOMAIN_MAX; step++ )); do
        candidate=$(( ((first - 1 + step) % NROS_TEST_DOMAIN_MAX) + 1 ))
        if ! nros_domain_busy "$candidate"; then
            echo "$candidate"
            return 0
        fi
    done
    echo "$first"
}

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
# The recipe is `dds_isolation.rs`'s CYCLONE_LOOPBACK_XML, copied rather than
# re-derived: `AllowMulticast=false` PLUS an explicit localhost peer. The
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
# Usage: `nros_export_cyclone_config <path-to-write>`. Exports CYCLONEDDS_URI
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

    {
        echo '<?xml version="1.0" encoding="UTF-8" ?>'
        echo '<!-- Generated by ros2_e2e_common.sh (issues 1009 / 1139). Mirrors'
        echo '     nros_tests::dds_isolation CYCLONE_LOOPBACK_XML. -->'
        echo '<CycloneDDS xmlns="https://cdds.io/config">'
        echo '  <Domain id="any">'
        echo '    <General>'
        echo '      <Interfaces>'
        echo "        <NetworkInterface address=\"${NROS_E2E_LOOPBACK_ADDRESS:-127.0.0.1}\" priority=\"default\" multicast=\"false\" />"
        echo '      </Interfaces>'
        echo '      <AllowMulticast>false</AllowMulticast>'
        echo '    </General>'
        echo '    <Discovery>'
        echo '      <ParticipantIndex>auto</ParticipantIndex>'
        echo '      <Peers>'
        echo "        <Peer address=\"${NROS_E2E_LOOPBACK_PEER:-localhost}\"/>"
        echo '      </Peers>'
        echo '    </Discovery>'
        echo '  </Domain>'
        echo '</CycloneDDS>'
    } > "$path"
    export CYCLONEDDS_URI="file://$path"
    echo "  bus pinned to loopback (issue 1009; NROS_DDS_ALLOW_LAN=1 to opt out)"
    return 0
}
