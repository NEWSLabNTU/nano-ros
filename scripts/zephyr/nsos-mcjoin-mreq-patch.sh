#!/usr/bin/env bash
# scripts/zephyr/nsos-mcjoin-mreq-patch.sh
#
# Phase 11W.11 — make the NSOS IP_ADD_MEMBERSHIP / IP_DROP_MEMBERSHIP
# handler accept both `struct ip_mreq` (8 bytes) and `struct ip_mreqn`
# (12 bytes).
#
# `native-sim-ipproto-ip-patch.sh` added the membership handler but
# hard-coded `optlen != sizeof(struct ip_mreqn)` → EINVAL. Cyclone DDS
# passes `struct ip_mreq` (8 bytes: imr_multiaddr + imr_interface, via
# the nano-ros zephyr_ipv4_compat.h shim), so the SPDP multicast join
# failed with EINVAL and discovery never worked on NSOS.
#
# Both structs share the same first 8 bytes — imr_multiaddr (in_addr)
# then the interface IP (imr_interface for ip_mreq, imr_address for
# ip_mreqn). Read those two leading in_addrs and ignore any trailing
# ifindex, so a single handler covers both shapes without depending on
# either struct being declared in nsos_sockets.c scope.
#
# Must run AFTER native-sim-ipproto-ip-patch.sh. Idempotent.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
NANO_ROS_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
IN_TREE_WORKSPACE="$NANO_ROS_ROOT/zephyr-workspace"

if [ -n "${1:-}" ]; then
    WORKSPACE="$1"
elif [ -d "$IN_TREE_WORKSPACE/zephyr" ]; then
    WORKSPACE="$IN_TREE_WORKSPACE"
else
    WORKSPACE="$(cd "$NANO_ROS_ROOT/.." && pwd)/nano-ros-workspace"
fi

TARGET="$WORKSPACE/zephyr/drivers/net/nsos_sockets.c"
if [ ! -f "$TARGET" ]; then
    echo "ERROR: $TARGET missing" >&2
    exit 1
fi
if ! grep -q 'IP_ADD_MEMBERSHIP' "$TARGET"; then
    echo "ERROR: IP_ADD_MEMBERSHIP handler not present — run native-sim-ipproto-ip-patch.sh first" >&2
    exit 1
fi
if grep -q "nano-ros: dual ip_mreq/ip_mreqn" "$TARGET"; then
    echo "[nsos-mcjoin-mreq-patch] already applied"
    exit 0
fi

python3 - "$TARGET" <<'PYEOF'
import sys
from pathlib import Path

path = Path(sys.argv[1])
src = path.read_text()

old = '''			const struct ip_mreqn *mreq = optval;
			struct nsos_mid_ip_mreq nsos_mid_mreq;
			int nsos_mid_optname = (optname == IP_ADD_MEMBERSHIP)
				? NSOS_MID_IP_ADD_MEMBERSHIP
				: NSOS_MID_IP_DROP_MEMBERSHIP;
			int err;

			if (optlen != sizeof(struct ip_mreqn)) {
				errno = EINVAL;
				return -1;
			}

			nsos_mid_mreq.imr_multiaddr = mreq->imr_multiaddr.s_addr;
			nsos_mid_mreq.imr_interface = mreq->imr_address.s_addr;'''
new = '''			/* nano-ros: dual ip_mreq/ip_mreqn — Phase 11W.11.
			 * Accept both struct ip_mreq (8B: multiaddr+interface,
			 * what Cyclone DDS passes) and struct ip_mreqn (12B:
			 * multiaddr+address+ifindex). Both share the same first
			 * 8 bytes (multiaddr, then the interface IP), so read
			 * the two leading in_addrs and ignore any trailing
			 * ifindex. */
			const struct in_addr *mreq_addrs = optval;
			struct nsos_mid_ip_mreq nsos_mid_mreq;
			int nsos_mid_optname = (optname == IP_ADD_MEMBERSHIP)
				? NSOS_MID_IP_ADD_MEMBERSHIP
				: NSOS_MID_IP_DROP_MEMBERSHIP;
			int err;

			if (optlen < 2 * sizeof(struct in_addr)) {
				errno = EINVAL;
				return -1;
			}

			nsos_mid_mreq.imr_multiaddr = mreq_addrs[0].s_addr;
			nsos_mid_mreq.imr_interface = mreq_addrs[1].s_addr;'''
if old not in src:
    sys.stderr.write("ERROR: IP_ADD_MEMBERSHIP ip_mreqn block not found "
                     "(native-sim-ipproto-ip-patch shape changed?)\n")
    sys.exit(1)
src = src.replace(old, new, 1)
path.write_text(src)
PYEOF

echo "[nsos-mcjoin-mreq-patch] patched $TARGET"
