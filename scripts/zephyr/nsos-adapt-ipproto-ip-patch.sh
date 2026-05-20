#!/usr/bin/env bash
# scripts/zephyr/nsos-adapt-ipproto-ip-patch.sh
#
# Phase 11W.12 — add the host (bottom-half) IPPROTO_IP setsockopt
# handler to NSOS. `native-sim-ipproto-ip-patch.sh` added the
# guest-side (nsos_sockets.c) IP_ADD_MEMBERSHIP / IP_MULTICAST_*
# handling but NOT the host-side forwarder in nsos_adapt.c — so the
# midplane call hit `return -NSOS_MID_EOPNOTSUPP` and the membership
# join never reached the host kernel. SPDP multicast discovery
# therefore failed on every interface (loopback AND real).
#
# Adds a `case NSOS_MID_IPPROTO_IP:` to nsos_adapt_setsockopt that
# reconstructs the host `struct ip_mreq` (for ADD/DROP_MEMBERSHIP) /
# `struct in_addr` (MULTICAST_IF) / int (MULTICAST_TTL/LOOP) and calls
# the host setsockopt(IPPROTO_IP, ...).
#
# Must run AFTER native-sim-ipproto-ip-patch.sh (needs the NSOS_MID_IP_*
# constants + struct nsos_mid_ip_mreq in nsos_socket.h). Idempotent.

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

TARGET="$WORKSPACE/zephyr/drivers/net/nsos_adapt.c"
[ -f "$TARGET" ] || { echo "ERROR: $TARGET missing" >&2; exit 1; }
if ! grep -q 'NSOS_MID_IPPROTO_TCP' "$TARGET"; then
    echo "ERROR: nsos_adapt_setsockopt not in expected shape" >&2; exit 1
fi
if grep -q "nano-ros: nsos_adapt IPPROTO_IP" "$TARGET"; then
    echo "[nsos-adapt-ipproto-ip-patch] already applied"
    exit 0
fi
# Phase 177.1 — `native-sim-ipproto-ip-patch.sh` (Phase 11W) already adds
# a complete `case NSOS_MID_IPPROTO_IP:` to `nsos_adapt_setsockopt`
# (every IP_* multicast/membership optname + the getsockopt side), so
# this 11W.12 patch is redundant with it. Emitting a second label
# produced `error: duplicate case value` and broke all 54
# cyclonedds-zephyr fixtures. Skip whenever the case is already present
# (it always is — this patch runs after native-sim per the header note).
if grep -q "case NSOS_MID_IPPROTO_IP" "$TARGET"; then
    echo "[nsos-adapt-ipproto-ip-patch] IPPROTO_IP case already present (native-sim-ipproto-ip-patch.sh) — skip"
    exit 0
fi

python3 - "$TARGET" <<'PYEOF'
import sys
from pathlib import Path
path = Path(sys.argv[1])
src = path.read_text()

# Insert a NSOS_MID_IPPROTO_IP case just before the IPPROTO_IPV6 case
# in nsos_adapt_setsockopt.
anchor = '''	case NSOS_MID_IPPROTO_IPV6:
		switch (nsos_mid_optname) {
		case NSOS_MID_IPV6_V6ONLY:
			return nsos_adapt_setsockopt_int(fd, IPPROTO_IPV6, IPV6_V6ONLY,
							 nsos_mid_optval, nsos_mid_optlen);
		}
		break;
	}

	return -NSOS_MID_EOPNOTSUPP;'''
block = '''	/* nano-ros: nsos_adapt IPPROTO_IP — Phase 11W.12. Host-side
	 * forwarder for the IPv4 multicast setsockopts the guest side
	 * marshals; without it the midplane returned EOPNOTSUPP and the
	 * SPDP multicast join never reached the host kernel. */
	case NSOS_MID_IPPROTO_IP:
		switch (nsos_mid_optname) {
		case NSOS_MID_IP_ADD_MEMBERSHIP:
		case NSOS_MID_IP_DROP_MEMBERSHIP: {
			const struct nsos_mid_ip_mreq *m = nsos_mid_optval;
			struct ip_mreq mreq;
			int ret;

			memset(&mreq, 0, sizeof(mreq));
			mreq.imr_multiaddr.s_addr = m->imr_multiaddr;
			mreq.imr_interface.s_addr = m->imr_interface;
			ret = setsockopt(fd, IPPROTO_IP,
					 (nsos_mid_optname == NSOS_MID_IP_ADD_MEMBERSHIP)
						 ? IP_ADD_MEMBERSHIP : IP_DROP_MEMBERSHIP,
					 &mreq, sizeof(mreq));
			if (ret < 0) {
				return -errno_to_nsos_mid(errno);
			}
			return 0;
		}
		case NSOS_MID_IP_MULTICAST_IF: {
			struct in_addr ia;
			int ret;

			memset(&ia, 0, sizeof(ia));
			ia.s_addr = *(const unsigned int *)nsos_mid_optval;
			ret = setsockopt(fd, IPPROTO_IP, IP_MULTICAST_IF,
					 &ia, sizeof(ia));
			if (ret < 0) {
				return -errno_to_nsos_mid(errno);
			}
			return 0;
		}
		case NSOS_MID_IP_MULTICAST_TTL:
			return nsos_adapt_setsockopt_int(fd, IPPROTO_IP, IP_MULTICAST_TTL,
							 nsos_mid_optval, nsos_mid_optlen);
		case NSOS_MID_IP_MULTICAST_LOOP:
			return nsos_adapt_setsockopt_int(fd, IPPROTO_IP, IP_MULTICAST_LOOP,
							 nsos_mid_optval, nsos_mid_optlen);
		}
		break;

	case NSOS_MID_IPPROTO_IPV6:
		switch (nsos_mid_optname) {
		case NSOS_MID_IPV6_V6ONLY:
			return nsos_adapt_setsockopt_int(fd, IPPROTO_IPV6, IPV6_V6ONLY,
							 nsos_mid_optval, nsos_mid_optlen);
		}
		break;
	}

	return -NSOS_MID_EOPNOTSUPP;'''
if anchor not in src:
    sys.stderr.write("ERROR: nsos_adapt_setsockopt IPV6 anchor not found\n")
    sys.exit(1)
src = src.replace(anchor, block, 1)
path.write_text(src)
PYEOF

echo "[nsos-adapt-ipproto-ip-patch] patched $TARGET"
