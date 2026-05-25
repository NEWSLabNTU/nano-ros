#!/usr/bin/env bash
# scripts/zephyr/native-sim-ipproto-ip-patch-4.4.sh
#
# Phase 180.A (4.4 port of Phase 11W.10/11) — patch Zephyr 4.4's NSOS
# (Native Simulator Offloaded Sockets) GUEST side to forward IPPROTO_IP
# setsockopt / getsockopt options to the host kernel. Without this, the
# guest `setsockopt(IP_ADD_MEMBERSHIP)` returns EOPNOTSUPP, the listener
# never joins `239.255.0.1` host-side, SPDP multicast frames are never
# delivered, and cyclonedds discovery on `native_sim` is dead-on-arrival
# (boot shows `cyclone: error -1 in join conn (udp/239.255.0.1) ...
# multicast join failed ... continuing unicast-only`).
#
# What lands (this script — guest half):
#   1. drivers/net/nsos_socket.h
#      - Add `NSOS_MID_IP_{MULTICAST_IF,MULTICAST_TTL,MULTICAST_LOOP,
#        ADD_MEMBERSHIP,DROP_MEMBERSHIP}` constants (Linux raw values 32-36).
#      - Add `struct nsos_mid_ip_mreq` (8-byte fixed NSOS wire struct).
#   2. drivers/net/nsos_sockets.c
#      - Guest `nsos_setsockopt` / `nsos_getsockopt`: add a
#        `case NET_IPPROTO_IP:` switch that marshals the IPv4 multicast
#        options into the NSOS midplane and forwards via
#        nsos_adapt_setsockopt.
#
# The host half (drivers/net/nsos_adapt.c) is added by the sibling
# nsos-adapt-ipproto-ip-patch-4.4.sh — run it AFTER this one.
#
# 3.7 -> 4.4 differences handled here:
#   - 4.4 renamed the POSIX socket types: the guest uses `net_socklen_t`
#     and `NET_IPPROTO_IP` (level) + `ZSOCK_IP_*` (optname) rather than
#     `IPPROTO_IP` / `IP_*`. 4.4 already declares all five ZSOCK_IP_*
#     multicast constants (32-36), so we match on those directly — no
#     raw-32 fallback like the 3.7 script needed.
#   - 4.4's errno bridge is `nsi_errno_from_mid` (was `errno_from_nsos_mid`).
#   - 4.4 names the mreq structs `struct net_ip_mreq` (8B) / `struct
#     net_ip_mreqn` (12B) with `struct net_in_addr` members; both share
#     the same first 8 bytes (multiaddr, then the interface IP). We read
#     the two leading net_in_addrs (dual ip_mreq/ip_mreqn, folding in the
#     3.7 mcjoin-mreq patch) so a single arm covers both shapes.
#   - <zephyr/net/net_ip.h> (already included by nsos_sockets.c) provides
#     net_in_addr / net_ip_mreq{,n}, so no extra include is needed.
#
# Idempotent: re-running detects prior application via grep + skips.
#
# Usage: native-sim-ipproto-ip-patch-4.4.sh <workspace-dir>
set -euo pipefail

WORKSPACE="${1:?usage: native-sim-ipproto-ip-patch-4.4.sh <workspace-dir>}"
if [ ! -d "$WORKSPACE/zephyr" ]; then
    echo "ERROR: $WORKSPACE doesn't look like a Zephyr workspace (missing zephyr/)" >&2
    exit 1
fi

NSOS_HEADER="$WORKSPACE/zephyr/drivers/net/nsos_socket.h"
NSOS_SOCKETS="$WORKSPACE/zephyr/drivers/net/nsos_sockets.c"

for f in "$NSOS_HEADER" "$NSOS_SOCKETS"; do
    if [ ! -f "$f" ]; then
        echo "ERROR: expected file not found: $f" >&2
        exit 1
    fi
done

# ---- Patch 1: drivers/net/nsos_socket.h ----
if grep -q 'NSOS_MID_IP_ADD_MEMBERSHIP' "$NSOS_HEADER"; then
    echo "[native-sim-ipproto-ip-patch-4.4] nsos_socket.h already has IPv4 mcast constants — skip"
else
    echo "[native-sim-ipproto-ip-patch-4.4] nsos_socket.h += IPv4 mcast options + nsos_mid_ip_mreq"
    python3 - "$NSOS_HEADER" <<'PY'
import sys
path = sys.argv[1]
with open(path) as f:
    src = f.read()

# Insert the IPv4 block + struct just before the closing IPv6 @} / #endif.
anchor = "/** @} */\n\n#endif /* __DRIVERS_NET_NSOS_SOCKET_H__ */"
if anchor not in src:
    sys.exit(f"anchor missing: {anchor!r}")

insert = """
/**
 * @name IPv4 level options (NSOS_MID_IPPROTO_IP)
 * @{
 */
/* Socket options for NSOS_MID_IPPROTO_IP level */

/** Set the IPv4 multicast interface. */
#define NSOS_MID_IP_MULTICAST_IF 32

/** Set the multicast TTL for the socket. */
#define NSOS_MID_IP_MULTICAST_TTL 33

/** Toggle IPv4 multicast loopback. */
#define NSOS_MID_IP_MULTICAST_LOOP 34

/** Join an IPv4 multicast group. */
#define NSOS_MID_IP_ADD_MEMBERSHIP 35

/** Leave an IPv4 multicast group. */
#define NSOS_MID_IP_DROP_MEMBERSHIP 36

/** IPv4 mreq wire-format (mirrors host `struct ip_mreq`). */
struct nsos_mid_ip_mreq {
\tuint32_t imr_multiaddr;\t/* Multicast group (network byte order) */
\tuint32_t imr_interface;\t/* Local interface IP (network byte order) */
};

/** @} */

"""

new = src.replace(anchor, insert + anchor, 1)
with open(path, "w") as f:
    f.write(new)
PY
fi

# ---- Patch 2: drivers/net/nsos_sockets.c (guest) ----
if grep -q 'nano-ros: nsos IPPROTO_IP guest' "$NSOS_SOCKETS"; then
    echo "[native-sim-ipproto-ip-patch-4.4] nsos_sockets.c already handles NET_IPPROTO_IP — skip"
else
    echo "[native-sim-ipproto-ip-patch-4.4] nsos_sockets.c += NET_IPPROTO_IP setsockopt + getsockopt"
    python3 - "$NSOS_SOCKETS" <<'PY'
import sys
path = sys.argv[1]
with open(path) as f:
    src = f.read()

# 4.4 anchors: the IPV6_V6ONLY return arm in both nsos_setsockopt and
# nsos_getsockopt. Level is NET_IPPROTO_IPV6, optname ZSOCK_IPV6_V6ONLY.
sets_anchor = (
    "\tcase NET_IPPROTO_IPV6:\n"
    "\t\tswitch (optname) {\n"
    "\t\tcase ZSOCK_IPV6_V6ONLY:\n"
    "\t\t\treturn nsos_setsockopt_int(sock,\n"
    "\t\t\t\t\t\t   NSOS_MID_IPPROTO_IPV6, NSOS_MID_IPV6_V6ONLY,\n"
    "\t\t\t\t\t\t   optval, optlen);\n"
    "\t\t}\n"
    "\t\tbreak;\n"
)
gets_anchor = (
    "\tcase NET_IPPROTO_IPV6:\n"
    "\t\tswitch (optname) {\n"
    "\t\tcase ZSOCK_IPV6_V6ONLY:\n"
    "\t\t\treturn nsos_getsockopt_int(sock,\n"
    "\t\t\t\t\t\t   NSOS_MID_IPPROTO_IPV6, NSOS_MID_IPV6_V6ONLY,\n"
    "\t\t\t\t\t\t   optval, optlen);\n"
    "\t\t}\n"
    "\t\tbreak;\n"
)

# nano-ros: nsos IPPROTO_IP guest — Phase 180.A. Forward the IPv4
# multicast setsockopts (the marker grep keys on this comment).
sets_insert = (
    "\t/* nano-ros: nsos IPPROTO_IP guest — Phase 180.A (4.4 port of\n"
    "\t * Phase 11W.10/11). Marshal the IPv4 multicast setsockopts into\n"
    "\t * the NSOS midplane so the host adapt half can forward them to\n"
    "\t * the host kernel; without this SPDP multicast join fails and\n"
    "\t * cyclonedds discovery never works on native_sim. */\n"
    "\tcase NET_IPPROTO_IP:\n"
    "\t\tswitch (optname) {\n"
    "\t\tcase ZSOCK_IP_MULTICAST_TTL:\n"
    "\t\t\treturn nsos_setsockopt_int(sock,\n"
    "\t\t\t\t\t   NSOS_MID_IPPROTO_IP, NSOS_MID_IP_MULTICAST_TTL,\n"
    "\t\t\t\t\t   optval, optlen);\n"
    "\t\tcase ZSOCK_IP_MULTICAST_LOOP:\n"
    "\t\t\treturn nsos_setsockopt_int(sock,\n"
    "\t\t\t\t\t   NSOS_MID_IPPROTO_IP, NSOS_MID_IP_MULTICAST_LOOP,\n"
    "\t\t\t\t\t   optval, optlen);\n"
    "\t\tcase ZSOCK_IP_MULTICAST_IF: {\n"
    "\t\t\tconst struct net_in_addr *iface = optval;\n"
    "\t\t\tuint32_t addr_be;\n"
    "\t\t\tint err;\n"
    "\n"
    "\t\t\tif (optlen != sizeof(struct net_in_addr)) {\n"
    "\t\t\t\terrno = EINVAL;\n"
    "\t\t\t\treturn -1;\n"
    "\t\t\t}\n"
    "\t\t\taddr_be = iface->s_addr;\n"
    "\n"
    "\t\t\terr = nsos_adapt_setsockopt(sock->poll.mid.fd, NSOS_MID_IPPROTO_IP,\n"
    "\t\t\t\t\t\t    NSOS_MID_IP_MULTICAST_IF, &addr_be,\n"
    "\t\t\t\t\t\t    sizeof(addr_be));\n"
    "\t\t\tif (err) {\n"
    "\t\t\t\terrno = nsi_errno_from_mid(-err);\n"
    "\t\t\t\treturn -1;\n"
    "\t\t\t}\n"
    "\t\t\treturn 0;\n"
    "\t\t}\n"
    "\t\tcase ZSOCK_IP_ADD_MEMBERSHIP:\n"
    "\t\tcase ZSOCK_IP_DROP_MEMBERSHIP: {\n"
    "\t\t\t/* nano-ros: dual ip_mreq/ip_mreqn. Accept both\n"
    "\t\t\t * struct net_ip_mreq (8B: multiaddr+interface, what\n"
    "\t\t\t * Cyclone DDS passes) and struct net_ip_mreqn (12B:\n"
    "\t\t\t * multiaddr+address+ifindex). Both share the same\n"
    "\t\t\t * first 8 bytes (multiaddr, then the interface IP),\n"
    "\t\t\t * so read the two leading net_in_addrs and ignore any\n"
    "\t\t\t * trailing ifindex. Marshal into the 8-byte wire-format\n"
    "\t\t\t * nsos_mid_ip_mreq and let the host adapt forward it. */\n"
    "\t\t\tconst struct net_in_addr *mreq_addrs = optval;\n"
    "\t\t\tstruct nsos_mid_ip_mreq nsos_mid_mreq;\n"
    "\t\t\tint nsos_mid_optname = (optname == ZSOCK_IP_ADD_MEMBERSHIP)\n"
    "\t\t\t\t? NSOS_MID_IP_ADD_MEMBERSHIP\n"
    "\t\t\t\t: NSOS_MID_IP_DROP_MEMBERSHIP;\n"
    "\t\t\tint err;\n"
    "\n"
    "\t\t\tif (optlen < 2 * sizeof(struct net_in_addr)) {\n"
    "\t\t\t\terrno = EINVAL;\n"
    "\t\t\t\treturn -1;\n"
    "\t\t\t}\n"
    "\n"
    "\t\t\tnsos_mid_mreq.imr_multiaddr = mreq_addrs[0].s_addr;\n"
    "\t\t\tnsos_mid_mreq.imr_interface = mreq_addrs[1].s_addr;\n"
    "\n"
    "\t\t\terr = nsos_adapt_setsockopt(sock->poll.mid.fd, NSOS_MID_IPPROTO_IP,\n"
    "\t\t\t\t\t\t    nsos_mid_optname, &nsos_mid_mreq,\n"
    "\t\t\t\t\t\t    sizeof(nsos_mid_mreq));\n"
    "\t\t\tif (err) {\n"
    "\t\t\t\terrno = nsi_errno_from_mid(-err);\n"
    "\t\t\t\treturn -1;\n"
    "\t\t\t}\n"
    "\t\t\treturn 0;\n"
    "\t\t}\n"
    "\t\t}\n"
    "\t\tbreak;\n"
    "\n"
)

# Getsockopt insert: only IP_MULTICAST_TTL / IP_MULTICAST_LOOP are
# readable; membership state is not.
gets_insert = (
    "\t/* nano-ros: nsos IPPROTO_IP guest — Phase 180.A. */\n"
    "\tcase NET_IPPROTO_IP:\n"
    "\t\tswitch (optname) {\n"
    "\t\tcase ZSOCK_IP_MULTICAST_TTL:\n"
    "\t\t\treturn nsos_getsockopt_int(sock,\n"
    "\t\t\t\t\t   NSOS_MID_IPPROTO_IP, NSOS_MID_IP_MULTICAST_TTL,\n"
    "\t\t\t\t\t   optval, optlen);\n"
    "\t\tcase ZSOCK_IP_MULTICAST_LOOP:\n"
    "\t\t\treturn nsos_getsockopt_int(sock,\n"
    "\t\t\t\t\t   NSOS_MID_IPPROTO_IP, NSOS_MID_IP_MULTICAST_LOOP,\n"
    "\t\t\t\t\t   optval, optlen);\n"
    "\t\t}\n"
    "\t\tbreak;\n"
    "\n"
)

if sets_anchor not in src:
    sys.exit("setsockopt anchor missing in nsos_sockets.c — file shape changed")
if gets_anchor not in src:
    sys.exit("getsockopt anchor missing in nsos_sockets.c — file shape changed")

src = src.replace(sets_anchor, sets_insert + sets_anchor, 1)
src = src.replace(gets_anchor, gets_insert + gets_anchor, 1)

with open(path, "w") as f:
    f.write(src)
PY
fi

echo "[native-sim-ipproto-ip-patch-4.4] done (guest half) — $WORKSPACE/zephyr"
