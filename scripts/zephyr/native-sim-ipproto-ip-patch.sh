#!/usr/bin/env bash
# scripts/zephyr/native-sim-ipproto-ip-patch.sh
#
# Phase 97.4.zephyr-native_sim — patch upstream Zephyr's NSOS (Native
# Simulator Offloaded Sockets) driver to forward `IPPROTO_IP` setsockopt
# / getsockopt options to the host kernel. Without this, guest
# `setsockopt(IP_ADD_MEMBERSHIP)` returns `EOPNOTSUPP`, the listener
# never joins `239.255.0.1` host-side, and SPDP multicast frames are
# never delivered → DDS discovery on `native_sim` is dead-on-arrival.
#
# Verified against Zephyr v3.7.0 + main (drivers/net/nsos_*.[ch] shape
# unchanged across the lineage).
#
# What lands:
#   1. drivers/net/nsos_socket.h
#      - Add `NSOS_MID_IPPROTO_IP` block with the five IPv4 mcast
#        constants (Linux raw values: 32–36).
#      - Add `struct nsos_mid_ip_mreq` (8-byte fixed-shape wire struct).
#   2. drivers/net/nsos_sockets.c
#      - Guest `nsos_setsockopt` / `nsos_getsockopt`: add
#        `case IPPROTO_IP:` switch that marshals `struct ip_mreq` →
#        `struct nsos_mid_ip_mreq` and forwards via NSOS midplane.
#   3. drivers/net/nsos_adapt.c
#      - Host `nsos_adapt_setsockopt` / `nsos_adapt_getsockopt`:
#        add `case NSOS_MID_IPPROTO_IP:` switch that unmarshals back
#        and calls host `setsockopt(SOL_SOCKET, IP_*, ...)`.
#
# Idempotent: re-running detects prior application via grep + skips.
#
# Usage:
#   scripts/zephyr/native-sim-ipproto-ip-patch.sh [<workspace-dir>]
#
# If <workspace-dir> is omitted, falls back to the ZEPHYR_WORKSPACE env
# var, then to ../nano-ros-workspace relative to this script.

set -euo pipefail

# ---- Resolve workspace directory ----
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
NANO_ROS_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
DEFAULT_WORKSPACE="$(cd "$NANO_ROS_ROOT/.." && pwd)/nano-ros-workspace"

WORKSPACE="${1:-${ZEPHYR_WORKSPACE:-$DEFAULT_WORKSPACE}}"

if [ ! -d "$WORKSPACE/zephyr" ]; then
    echo "ERROR: $WORKSPACE doesn't look like a Zephyr workspace (missing zephyr/)" >&2
    echo "       Run \`just zephyr setup\` first, or pass the workspace dir explicitly." >&2
    exit 1
fi

NSOS_HEADER="$WORKSPACE/zephyr/drivers/net/nsos_socket.h"
NSOS_SOCKETS="$WORKSPACE/zephyr/drivers/net/nsos_sockets.c"
NSOS_ADAPT="$WORKSPACE/zephyr/drivers/net/nsos_adapt.c"

for f in "$NSOS_HEADER" "$NSOS_SOCKETS" "$NSOS_ADAPT"; do
    if [ ! -f "$f" ]; then
        echo "ERROR: expected file not found: $f" >&2
        exit 1
    fi
done

# ---- Patch 1: drivers/net/nsos_socket.h ----
if grep -q 'NSOS_MID_IP_ADD_MEMBERSHIP' "$NSOS_HEADER"; then
    echo "[skip] nsos_socket.h already has IPv4 mcast constants"
else
    echo "[apply] nsos_socket.h += IPv4 mcast options + nsos_mid_ip_mreq"
    python3 - "$NSOS_HEADER" <<'PY'
import sys
path = sys.argv[1]
with open(path) as f:
    src = f.read()

# Insert the IPv4 block + struct just before the closing IPv6 @} comment
# block, anchored on `/** @} */\n\n#endif`.
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

/** IPv4 mreq wire-format (mirrors `struct ip_mreq`). */
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

# ---- Patch 2: drivers/net/nsos_sockets.c ----
# Match a string only the patched body contains; the file already has
# `case IPPROTO_IP:` in the proto-to-mid mapping but no IP_MULTICAST_TTL.
if grep -q 'IP_MULTICAST_TTL' "$NSOS_SOCKETS"; then
    echo "[skip] nsos_sockets.c already handles IPPROTO_IP setsockopt/getsockopt"
else
    echo "[apply] nsos_sockets.c += IPPROTO_IP setsockopt + getsockopt"
    python3 - "$NSOS_SOCKETS" <<'PY'
import sys
path = sys.argv[1]
with open(path) as f:
    src = f.read()

# Setsockopt + getsockopt anchors differ only in the helper name.
# Continuation indent = 6 tabs + 3 spaces (matches `nsos_*sockopt_int(`).
sets_anchor = (
    "\tcase IPPROTO_IPV6:\n"
    "\t\tswitch (optname) {\n"
    "\t\tcase IPV6_V6ONLY:\n"
    "\t\t\treturn nsos_setsockopt_int(sock,\n"
    "\t\t\t\t\t\t   NSOS_MID_IPPROTO_IPV6, NSOS_MID_IPV6_V6ONLY,\n"
    "\t\t\t\t\t\t   optval, optlen);\n"
    "\t\t}\n"
    "\t\tbreak;\n"
)
gets_anchor = (
    "\tcase IPPROTO_IPV6:\n"
    "\t\tswitch (optname) {\n"
    "\t\tcase IPV6_V6ONLY:\n"
    "\t\t\treturn nsos_getsockopt_int(sock,\n"
    "\t\t\t\t\t\t   NSOS_MID_IPPROTO_IPV6, NSOS_MID_IPV6_V6ONLY,\n"
    "\t\t\t\t\t\t   optval, optlen);\n"
    "\t\t}\n"
    "\t\tbreak;\n"
)
sets_insert = (
    "\tcase IPPROTO_IP:\n"
    "\t\tswitch (optname) {\n"
    "\t\tcase IP_MULTICAST_TTL:\n"
    "\t\t\treturn nsos_setsockopt_int(sock,\n"
    "\t\t\t\t\t   NSOS_MID_IPPROTO_IP, NSOS_MID_IP_MULTICAST_TTL,\n"
    "\t\t\t\t\t   optval, optlen);\n"
    "\t\t/* Zephyr's `<zephyr/net/socket.h>` doesn't define\n"
    "\t\t * `IP_MULTICAST_IF`; nano-ros guest code passes the raw\n"
    "\t\t * Linux value (32) so we match on that here. The host\n"
    "\t\t * `nsos_adapt` translates back to `IPPROTO_IP, IP_MULTICAST_IF`.\n"
    "\t\t */\n"
    "\t\tcase 32: {\n"
    "\t\t\tconst struct in_addr *iface = optval;\n"
    "\t\t\tuint32_t addr_be;\n"
    "\t\t\tint err;\n"
    "\n"
    "\t\t\tif (optlen != sizeof(struct in_addr)) {\n"
    "\t\t\t\terrno = EINVAL;\n"
    "\t\t\t\treturn -1;\n"
    "\t\t\t}\n"
    "\t\t\taddr_be = iface->s_addr;\n"
    "\n"
    "\t\t\terr = nsos_adapt_setsockopt(sock->poll.mid.fd, NSOS_MID_IPPROTO_IP,\n"
    "\t\t\t\t\t\t    NSOS_MID_IP_MULTICAST_IF, &addr_be,\n"
    "\t\t\t\t\t\t    sizeof(addr_be));\n"
    "\t\t\tif (err) {\n"
    "\t\t\t\terrno = errno_from_nsos_mid(-err);\n"
    "\t\t\t\treturn -1;\n"
    "\t\t\t}\n"
    "\t\t\treturn 0;\n"
    "\t\t}\n"
    "\t\tcase IP_ADD_MEMBERSHIP:\n"
    "\t\tcase IP_DROP_MEMBERSHIP: {\n"
    "\t\t\t/* Zephyr's socket.h declares `struct ip_mreqn` (12 bytes,\n"
    "\t\t\t * 4-tuple) as the only IPv4 mreq shape. We marshal that\n"
    "\t\t\t * into the wire-format `nsos_mid_ip_mreq` (8 bytes,\n"
    "\t\t\t * matches host-side `struct ip_mreq`) and let the host\n"
    "\t\t\t * adapt forward it to the host kernel.\n"
    "\t\t\t */\n"
    "\t\t\tconst struct ip_mreqn *mreq = optval;\n"
    "\t\t\tstruct nsos_mid_ip_mreq nsos_mid_mreq;\n"
    "\t\t\tint nsos_mid_optname = (optname == IP_ADD_MEMBERSHIP)\n"
    "\t\t\t\t? NSOS_MID_IP_ADD_MEMBERSHIP\n"
    "\t\t\t\t: NSOS_MID_IP_DROP_MEMBERSHIP;\n"
    "\t\t\tint err;\n"
    "\n"
    "\t\t\tif (optlen != sizeof(struct ip_mreqn)) {\n"
    "\t\t\t\terrno = EINVAL;\n"
    "\t\t\t\treturn -1;\n"
    "\t\t\t}\n"
    "\n"
    "\t\t\tnsos_mid_mreq.imr_multiaddr = mreq->imr_multiaddr.s_addr;\n"
    "\t\t\tnsos_mid_mreq.imr_interface = mreq->imr_address.s_addr;\n"
    "\n"
    "\t\t\terr = nsos_adapt_setsockopt(sock->poll.mid.fd, NSOS_MID_IPPROTO_IP,\n"
    "\t\t\t\t\t\t    nsos_mid_optname, &nsos_mid_mreq,\n"
    "\t\t\t\t\t\t    sizeof(nsos_mid_mreq));\n"
    "\t\t\tif (err) {\n"
    "\t\t\t\terrno = errno_from_nsos_mid(-err);\n"
    "\t\t\t\treturn -1;\n"
    "\t\t\t}\n"
    "\t\t\treturn 0;\n"
    "\t\t}\n"
    "\t\t}\n"
    "\t\tbreak;\n"
    "\n"
)

# Getsockopt insert (only IP_MULTICAST_TTL — membership state not
# readable, IP_MULTICAST_LOOP not declared in Zephyr's socket.h).
gets_insert = (
    "\tcase IPPROTO_IP:\n"
    "\t\tswitch (optname) {\n"
    "\t\tcase IP_MULTICAST_TTL:\n"
    "\t\t\treturn nsos_getsockopt_int(sock,\n"
    "\t\t\t\t\t   NSOS_MID_IPPROTO_IP, NSOS_MID_IP_MULTICAST_TTL,\n"
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

# ---- Patch 3: drivers/net/nsos_adapt.c ----
# Same disambiguation as patch 2: NSOS_MID_IPPROTO_IP appears in the
# proto-mapping but NSOS_MID_IP_MULTICAST_TTL only in the patched body.
if grep -q 'NSOS_MID_IP_MULTICAST_TTL' "$NSOS_ADAPT"; then
    echo "[skip] nsos_adapt.c already handles NSOS_MID_IPPROTO_IP setsockopt/getsockopt"
else
    echo "[apply] nsos_adapt.c += NSOS_MID_IPPROTO_IP setsockopt + getsockopt"
    python3 - "$NSOS_ADAPT" <<'PY'
import sys
path = sys.argv[1]
with open(path) as f:
    src = f.read()

# Anchor: the IPV6_V6ONLY return arm in both setsockopt and getsockopt
# bodies. Same as nsos_sockets.c.
anchor = (
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
    "\tcase NSOS_MID_IPPROTO_IP:\n"
    "\t\tswitch (nsos_mid_optname) {\n"
    "\t\tcase NSOS_MID_IP_MULTICAST_IF: {\n"
    "\t\t\tconst uint32_t *iface_addr = nsos_mid_optval;\n"
    "\t\t\tstruct in_addr in;\n"
    "\t\t\tint ret;\n"
    "\n"
    "\t\t\tif (nsos_mid_optlen != sizeof(uint32_t)) {\n"
    "\t\t\t\treturn -NSOS_MID_EINVAL;\n"
    "\t\t\t}\n"
    "\t\t\tin.s_addr = *iface_addr;\n"
    "\n"
    "\t\t\tret = setsockopt(fd, IPPROTO_IP, IP_MULTICAST_IF, &in, sizeof(in));\n"
    "\t\t\tif (ret < 0) {\n"
    "\t\t\t\treturn -errno_to_nsos_mid(errno);\n"
    "\t\t\t}\n"
    "\t\t\treturn 0;\n"
    "\t\t}\n"
    "\t\tcase NSOS_MID_IP_MULTICAST_TTL:\n"
    "\t\t\treturn nsos_adapt_setsockopt_int(fd, IPPROTO_IP, IP_MULTICAST_TTL,\n"
    "\t\t\t\t\t\t\t nsos_mid_optval, nsos_mid_optlen);\n"
    "\t\tcase NSOS_MID_IP_MULTICAST_LOOP:\n"
    "\t\t\treturn nsos_adapt_setsockopt_int(fd, IPPROTO_IP, IP_MULTICAST_LOOP,\n"
    "\t\t\t\t\t\t\t nsos_mid_optval, nsos_mid_optlen);\n"
    "\t\tcase NSOS_MID_IP_ADD_MEMBERSHIP:\n"
    "\t\tcase NSOS_MID_IP_DROP_MEMBERSHIP: {\n"
    "\t\t\tconst struct nsos_mid_ip_mreq *m = nsos_mid_optval;\n"
    "\t\t\tstruct ip_mreq mreq;\n"
    "\t\t\tint host_optname = (nsos_mid_optname == NSOS_MID_IP_ADD_MEMBERSHIP)\n"
    "\t\t\t\t? IP_ADD_MEMBERSHIP\n"
    "\t\t\t\t: IP_DROP_MEMBERSHIP;\n"
    "\t\t\tint ret;\n"
    "\n"
    "\t\t\tif (nsos_mid_optlen != sizeof(struct nsos_mid_ip_mreq)) {\n"
    "\t\t\t\treturn -NSOS_MID_EINVAL;\n"
    "\t\t\t}\n"
    "\n"
    "\t\t\tmreq.imr_multiaddr.s_addr = m->imr_multiaddr;\n"
    "\t\t\tmreq.imr_interface.s_addr = m->imr_interface;\n"
    "\n"
    "\t\t\tret = setsockopt(fd, IPPROTO_IP, host_optname, &mreq, sizeof(mreq));\n"
    "\t\t\tif (ret < 0) {\n"
    "\t\t\t\treturn -errno_to_nsos_mid(errno);\n"
    "\t\t\t}\n"
    "\t\t\treturn 0;\n"
    "\t\t}\n"
    "\t\t}\n"
    "\t\tbreak;\n"
    "\n"
)

gets_insert = (
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

if anchor not in src:
    sys.exit(f"setsockopt anchor missing in nsos_adapt.c — file shape changed")
if gets_anchor not in src:
    sys.exit(f"getsockopt anchor missing in nsos_adapt.c — file shape changed")

src = src.replace(anchor, sets_insert + anchor, 1)
src = src.replace(gets_anchor, gets_insert + gets_anchor, 1)

with open(path, "w") as f:
    f.write(src)
PY
fi

echo "[done] NSOS IPPROTO_IP patch applied to $WORKSPACE/zephyr"
