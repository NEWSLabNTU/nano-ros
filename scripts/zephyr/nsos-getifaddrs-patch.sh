#!/usr/bin/env bash
# scripts/zephyr/nsos-getifaddrs-patch.sh
#
# Phase 11W.12 — add an NSOS host trampoline that returns the host's
# primary multicast-capable IPv4 interface. Cyclone DDS' multicast
# discovery (SPDP) needs a real interface to join the group on; the
# synthetic `ddsrt_getifaddrs` returns 127.0.0.1, and Linux can't join
# a multicast group on the loopback interface, so two native_sim
# processes never discover each other.
#
# Adds:
#   - struct nsos_mid_ifaddr (fixed wire struct: one IPv4 interface)
#     + `int nsos_adapt_getifaddrs(struct nsos_mid_ifaddr *)` to nsos.h
#   - host-side impl in nsos_adapt.c: host getifaddrs(), pick the first
#     UP + MULTICAST + non-loopback AF_INET interface, fill the struct.
#
# The cyclonedds `ddsrt_getifaddrs` override (link_stubs.c) calls this
# to learn the real interface instead of hardcoding loopback.
#
# Idempotent.

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

NSOS_H="$WORKSPACE/zephyr/drivers/net/nsos.h"
NSOS_ADAPT="$WORKSPACE/zephyr/drivers/net/nsos_adapt.c"
for f in "$NSOS_H" "$NSOS_ADAPT"; do
    [ -f "$f" ] || { echo "ERROR: $f missing" >&2; exit 1; }
done
if grep -q "nano-ros: nsos getifaddrs" "$NSOS_ADAPT"; then
    echo "[nsos-getifaddrs-patch] already applied"
    exit 0
fi

# 1. nsos.h — struct + decl, after the getsockname decl (or recvfrom).
python3 - "$NSOS_H" <<'PYEOF'
import sys
from pathlib import Path
path = Path(sys.argv[1])
src = path.read_text()
anchor = "int nsos_adapt_getsockname(int fd, struct nsos_mid_sockaddr *addr, size_t *addrlen);"
if anchor not in src:
    # getsockname patch not applied yet — fall back to recvfrom decl.
    anchor = "int nsos_adapt_recvfrom(int fd, void *buf, size_t len, int flags,\n\t\t\tstruct nsos_mid_sockaddr *addr, size_t *addrlen);"
if anchor not in src:
    sys.stderr.write("ERROR: nsos.h anchor not found\n"); sys.exit(1)
add = anchor + '''

/* nano-ros: nsos getifaddrs — Phase 11W.12 */
struct nsos_mid_ifaddr {
	unsigned int addr;      /* IPv4 address, network byte order */
	unsigned int netmask;   /* network byte order */
	unsigned int flags;     /* host IFF_* (UP/LOOPBACK/MULTICAST) */
	unsigned int ifindex;
	char name[16];
};
int nsos_adapt_getifaddrs(struct nsos_mid_ifaddr *out);'''
src = src.replace(anchor, add, 1)
path.write_text(src)
PYEOF

# 2. nsos_adapt.c — host impl, appended near getsockname / EOF.
python3 - "$NSOS_ADAPT" <<'PYEOF'
import sys
from pathlib import Path
path = Path(sys.argv[1])
src = path.read_text()

# Ensure <ifaddrs.h> + <net/if.h> are included (host headers).
if "#include <ifaddrs.h>" not in src:
    src = src.replace("#include <netinet/in.h>",
                      "#include <ifaddrs.h>\n#include <net/if.h>\n#include <netinet/in.h>", 1)

impl = '''

/* nano-ros: nsos getifaddrs — Phase 11W.12. Host-side: enumerate
 * interfaces and return the first UP + MULTICAST + non-loopback IPv4
 * one so Cyclone DDS can join SPDP multicast on a real interface.
 * Returns 0 on success, -1 if no suitable interface, <0 NSOS errno on
 * a getifaddrs() failure. */
int nsos_adapt_getifaddrs(struct nsos_mid_ifaddr *out)
{
	struct ifaddrs *ifa = NULL;
	struct ifaddrs *p;
	int rc = -1;

	if (getifaddrs(&ifa) != 0) {
		return -errno_to_nsos_mid(errno);
	}
	for (p = ifa; p != NULL; p = p->ifa_next) {
		struct sockaddr_in *sin;
		struct sockaddr_in *snm;

		if (p->ifa_addr == NULL || p->ifa_addr->sa_family != AF_INET) {
			continue;
		}
		if (!(p->ifa_flags & IFF_UP) || (p->ifa_flags & IFF_LOOPBACK) ||
		    !(p->ifa_flags & IFF_MULTICAST)) {
			continue;
		}
		sin = (struct sockaddr_in *)p->ifa_addr;
		snm = (struct sockaddr_in *)p->ifa_netmask;
		out->addr = sin->sin_addr.s_addr;
		out->netmask = snm ? snm->sin_addr.s_addr : htonl(0xffffff00u);
		out->flags = (unsigned int)p->ifa_flags;
		out->ifindex = if_nametoindex(p->ifa_name);
		strncpy(out->name, p->ifa_name, sizeof(out->name) - 1);
		out->name[sizeof(out->name) - 1] = '\\0';
		rc = 0;
		break;
	}
	freeifaddrs(ifa);
	return rc;
}'''
src = src.rstrip() + "\n" + impl + "\n"
path.write_text(src)
PYEOF

echo "[nsos-getifaddrs-patch] patched $WORKSPACE"
