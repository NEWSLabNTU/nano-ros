#!/usr/bin/env bash
# scripts/zephyr/nsos-adapt-ipproto-ip-patch-4.4.sh
#
# Phase 180.A (4.4 port of Phase 11W.12) — add the HOST (bottom-half)
# IPPROTO_IP setsockopt / getsockopt handler to Zephyr 4.4's NSOS.
# native-sim-ipproto-ip-patch-4.4.sh added the guest-side
# (nsos_sockets.c) IP_ADD_MEMBERSHIP / IP_MULTICAST_* marshalling and
# the NSOS_MID_IP_* constants + struct nsos_mid_ip_mreq in
# nsos_socket.h, but NOT the host forwarder in nsos_adapt.c — so the
# midplane call hit `return -NSI_ERRNO_MID_EOPNOTSUPP` and the
# membership join never reached the host kernel. SPDP multicast
# discovery therefore fails on native_sim (`cyclone: ... multicast join
# failed ... continuing unicast-only`).
#
# Adds a `case NSOS_MID_IPPROTO_IP:` to nsos_adapt_setsockopt that
# unmarshals struct nsos_mid_ip_mreq back into the host `struct ip_mreq`
# (for ADD/DROP_MEMBERSHIP) / `struct in_addr` (MULTICAST_IF) / int
# (MULTICAST_TTL/LOOP) and calls the host setsockopt(IPPROTO_IP, ...),
# plus the read-side arm in nsos_adapt_getsockopt.
#
# 3.7 -> 4.4 differences handled here:
#   - 4.4's NSOS errno bridge is `nsi_errno_to_mid(errno)` (returns the
#     positive mid code; arms negate it) — was `errno_to_nsos_mid`.
#   - 4.4's "unsupported" sentinel is `NSI_ERRNO_MID_EOPNOTSUPP` (was
#     `NSOS_MID_EOPNOTSUPP`); EINVAL is `NSI_ERRNO_MID_EINVAL`.
#   - The host adapt half compiles against the real host libc, so it
#     uses the real `struct ip_mreq` / `struct in_addr` / `IP_*`
#     (from <netinet/in.h>, already included) — NOT the net_* shims.
#
# Must run AFTER native-sim-ipproto-ip-patch-4.4.sh (needs the
# NSOS_MID_IP_* constants + struct nsos_mid_ip_mreq). Idempotent.
#
# Usage: nsos-adapt-ipproto-ip-patch-4.4.sh <workspace-dir>
set -euo pipefail

WORKSPACE="${1:?usage: nsos-adapt-ipproto-ip-patch-4.4.sh <workspace-dir>}"
TARGET="$WORKSPACE/zephyr/drivers/net/nsos_adapt.c"
if [ ! -f "$TARGET" ]; then
    echo "ERROR: $TARGET missing" >&2
    exit 1
fi
if ! grep -q 'NSOS_MID_IPPROTO_IPV6' "$TARGET"; then
    echo "ERROR: nsos_adapt_setsockopt not in expected shape (no NSOS_MID_IPPROTO_IPV6)" >&2
    exit 1
fi
if grep -q "nano-ros: nsos_adapt IPPROTO_IP" "$TARGET"; then
    echo "[nsos-adapt-ipproto-ip-patch-4.4] already applied"
    exit 0
fi

python3 - "$TARGET" <<'PYEOF'
import sys
from pathlib import Path
path = Path(sys.argv[1])
src = path.read_text()

# Anchor: the IPV6_V6ONLY return arm in both nsos_adapt_setsockopt and
# nsos_adapt_getsockopt. Disambiguated by helper name.
sets_anchor = (
    "\tcase NSOS_MID_IPPROTO_IPV6:\n"
    "\t\tswitch (nsos_mid_optname) {\n"
    "\t\tcase NSOS_MID_IPV6_V6ONLY:\n"
    "\t\t\treturn nsos_adapt_setsockopt_int(fd, IPPROTO_IPV6, IPV6_V6ONLY,\n"
    "\t\t\t\t\t\t\t nsos_mid_optval, nsos_mid_optlen);\n"
    "\t\t}\n"
    "\t\tbreak;\n"
)
gets_anchor = (
    "\tcase NSOS_MID_IPPROTO_IPV6:\n"
    "\t\tswitch (nsos_mid_optname) {\n"
    "\t\tcase NSOS_MID_IPV6_V6ONLY:\n"
    "\t\t\treturn nsos_adapt_getsockopt_int(fd, IPPROTO_IPV6, IPV6_V6ONLY,\n"
    "\t\t\t\t\t\t\t nsos_mid_optval, nsos_mid_optlen);\n"
    "\t\t}\n"
    "\t\tbreak;\n"
)

sets_insert = (
    "\t/* nano-ros: nsos_adapt IPPROTO_IP — Phase 180.A (4.4 port of\n"
    "\t * Phase 11W.12). Host-side forwarder for the IPv4 multicast\n"
    "\t * setsockopts the guest marshals; without it the midplane\n"
    "\t * returned EOPNOTSUPP and the SPDP multicast join never reached\n"
    "\t * the host kernel. Uses the real host struct ip_mreq / in_addr\n"
    "\t * (from <netinet/in.h>). */\n"
    "\tcase NSOS_MID_IPPROTO_IP:\n"
    "\t\tswitch (nsos_mid_optname) {\n"
    "\t\tcase NSOS_MID_IP_ADD_MEMBERSHIP:\n"
    "\t\tcase NSOS_MID_IP_DROP_MEMBERSHIP: {\n"
    "\t\t\tconst struct nsos_mid_ip_mreq *m = nsos_mid_optval;\n"
    "\t\t\tstruct ip_mreq mreq;\n"
    "\t\t\tint ret;\n"
    "\n"
    "\t\t\tif (nsos_mid_optlen != sizeof(struct nsos_mid_ip_mreq)) {\n"
    "\t\t\t\treturn -NSI_ERRNO_MID_EINVAL;\n"
    "\t\t\t}\n"
    "\n"
    "\t\t\tmemset(&mreq, 0, sizeof(mreq));\n"
    "\t\t\tmreq.imr_multiaddr.s_addr = m->imr_multiaddr;\n"
    "\t\t\tmreq.imr_interface.s_addr = m->imr_interface;\n"
    "\t\t\tret = setsockopt(fd, IPPROTO_IP,\n"
    "\t\t\t\t\t (nsos_mid_optname == NSOS_MID_IP_ADD_MEMBERSHIP)\n"
    "\t\t\t\t\t\t ? IP_ADD_MEMBERSHIP : IP_DROP_MEMBERSHIP,\n"
    "\t\t\t\t\t &mreq, sizeof(mreq));\n"
    "\t\t\tif (ret < 0) {\n"
    "\t\t\t\treturn -nsi_errno_to_mid(errno);\n"
    "\t\t\t}\n"
    "\t\t\treturn 0;\n"
    "\t\t}\n"
    "\t\tcase NSOS_MID_IP_MULTICAST_IF: {\n"
    "\t\t\tstruct in_addr ia;\n"
    "\t\t\tint ret;\n"
    "\n"
    "\t\t\tif (nsos_mid_optlen != sizeof(uint32_t)) {\n"
    "\t\t\t\treturn -NSI_ERRNO_MID_EINVAL;\n"
    "\t\t\t}\n"
    "\t\t\tmemset(&ia, 0, sizeof(ia));\n"
    "\t\t\tia.s_addr = *(const unsigned int *)nsos_mid_optval;\n"
    "\t\t\tret = setsockopt(fd, IPPROTO_IP, IP_MULTICAST_IF,\n"
    "\t\t\t\t\t &ia, sizeof(ia));\n"
    "\t\t\tif (ret < 0) {\n"
    "\t\t\t\treturn -nsi_errno_to_mid(errno);\n"
    "\t\t\t}\n"
    "\t\t\treturn 0;\n"
    "\t\t}\n"
    "\t\tcase NSOS_MID_IP_MULTICAST_TTL:\n"
    "\t\t\treturn nsos_adapt_setsockopt_int(fd, IPPROTO_IP, IP_MULTICAST_TTL,\n"
    "\t\t\t\t\t\t\t nsos_mid_optval, nsos_mid_optlen);\n"
    "\t\tcase NSOS_MID_IP_MULTICAST_LOOP:\n"
    "\t\t\treturn nsos_adapt_setsockopt_int(fd, IPPROTO_IP, IP_MULTICAST_LOOP,\n"
    "\t\t\t\t\t\t\t nsos_mid_optval, nsos_mid_optlen);\n"
    "\t\t}\n"
    "\t\tbreak;\n"
    "\n"
)

gets_insert = (
    "\t/* nano-ros: nsos_adapt IPPROTO_IP — Phase 180.A. */\n"
    "\tcase NSOS_MID_IPPROTO_IP:\n"
    "\t\tswitch (nsos_mid_optname) {\n"
    "\t\tcase NSOS_MID_IP_MULTICAST_TTL:\n"
    "\t\t\treturn nsos_adapt_getsockopt_int(fd, IPPROTO_IP, IP_MULTICAST_TTL,\n"
    "\t\t\t\t\t\t\t nsos_mid_optval, nsos_mid_optlen);\n"
    "\t\tcase NSOS_MID_IP_MULTICAST_LOOP:\n"
    "\t\t\treturn nsos_adapt_getsockopt_int(fd, IPPROTO_IP, IP_MULTICAST_LOOP,\n"
    "\t\t\t\t\t\t\t nsos_mid_optval, nsos_mid_optlen);\n"
    "\t\t}\n"
    "\t\tbreak;\n"
)

if sets_anchor not in src:
    sys.stderr.write("ERROR: nsos_adapt_setsockopt IPV6 anchor not found\n")
    sys.exit(1)
if gets_anchor not in src:
    sys.stderr.write("ERROR: nsos_adapt_getsockopt IPV6 anchor not found\n")
    sys.exit(1)

src = src.replace(sets_anchor, sets_insert + sets_anchor, 1)
src = src.replace(gets_anchor, gets_insert + gets_anchor, 1)

path.write_text(src)
PYEOF

echo "[nsos-adapt-ipproto-ip-patch-4.4] patched $TARGET"
