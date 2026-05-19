#!/usr/bin/env bash
# scripts/zephyr/nsos-getsockname-patch.sh
#
# Phase 11W.8 — add getsockname() support to Zephyr's Native
# Simulator Offloaded Sockets (NSOS). Upstream NSOS leaves the
# socket_op_vtable's `.getsockname` slot unpopulated, so any caller
# that needs the locally-bound address/port fails.
#
# Cyclone DDS needs this: after binding a UDP socket to an ephemeral
# port (port 0), it calls getsockname() to learn the kernel-assigned
# port and advertise it in the participant's unicast locator. Without
# getsockname the port reads back as 0 and discovery breaks, and the
# participant init aborts.
#
# The implementation mirrors the existing NSOS ops exactly:
#   - top half (nsos_sockets.c): translate via sockaddr_from_nsos_mid
#   - bottom half (nsos_adapt.c): call host getsockname(), translate
#     the result via sockaddr_to_nsos_mid
#   - declare the bottom-half entry in nsos.h
#   - wire `.getsockname` into nsos_socket_fd_op_vtable
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
NSOS_SOCK="$WORKSPACE/zephyr/drivers/net/nsos_sockets.c"

for f in "$NSOS_H" "$NSOS_ADAPT" "$NSOS_SOCK"; do
    if [ ! -f "$f" ]; then
        echo "ERROR: $f missing" >&2
        exit 1
    fi
done

if grep -q "nano-ros: nsos getsockname" "$NSOS_ADAPT"; then
    echo "[nsos-getsockname-patch] already applied"
    exit 0
fi

# 1. nsos.h — declare the bottom-half entry after nsos_adapt_recvfrom.
python3 - "$NSOS_H" <<'PYEOF'
import sys
from pathlib import Path
path = Path(sys.argv[1])
src = path.read_text()
anchor = '''int nsos_adapt_recvfrom(int fd, void *buf, size_t len, int flags,
			struct nsos_mid_sockaddr *addr, size_t *addrlen);'''
add = anchor + '''
/* nano-ros: nsos getsockname — Phase 11W.8 */
int nsos_adapt_getsockname(int fd, struct nsos_mid_sockaddr *addr, size_t *addrlen);'''
if anchor not in src:
    sys.stderr.write("ERROR: nsos.h recvfrom anchor not found\n"); sys.exit(1)
src = src.replace(anchor, add, 1)
path.write_text(src)
PYEOF

# 2. nsos_adapt.c — host-side impl, placed after nsos_adapt_recvfrom.
python3 - "$NSOS_ADAPT" <<'PYEOF'
import sys
from pathlib import Path
path = Path(sys.argv[1])
src = path.read_text()
# Anchor on the end of nsos_adapt_recvfrom: find its closing then insert.
marker = "int nsos_adapt_recvfrom(int fd, void *buf, size_t len, int flags,"
idx = src.find(marker)
if idx < 0:
    sys.stderr.write("ERROR: nsos_adapt_recvfrom not found\n"); sys.exit(1)
# Find the function's closing brace at column 0.
brace = src.find("\n{\n", idx)
depth = 0; i = brace + 1
while i < len(src):
    if src[i] == '{': depth += 1
    elif src[i] == '}':
        depth -= 1
        if depth == 0:
            end = i + 1; break
    i += 1
impl = '''

/* nano-ros: nsos getsockname — Phase 11W.8. Host-side trampoline:
 * call glibc getsockname(), translate the returned host sockaddr to
 * the NSOS middleground form. */
int nsos_adapt_getsockname(int fd, struct nsos_mid_sockaddr *addr_mid, size_t *addrlen_mid)
{
	struct sockaddr_storage addr_storage;
	struct sockaddr *addr = (struct sockaddr *)&addr_storage;
	socklen_t addrlen = sizeof(addr_storage);
	int ret;

	ret = getsockname(fd, addr, &addrlen);
	if (ret < 0) {
		return -errno_to_nsos_mid(errno);
	}

	ret = sockaddr_to_nsos_mid(addr, addrlen, addr_mid, addrlen_mid);
	if (ret < 0) {
		return ret;
	}

	return 0;
}'''
src = src[:end] + impl + src[end:]
path.write_text(src)
PYEOF

# 3. nsos_sockets.c — top-half static fn + vtable wiring.
python3 - "$NSOS_SOCK" <<'PYEOF'
import sys
from pathlib import Path
path = Path(sys.argv[1])
src = path.read_text()

# Insert nsos_getsockname() just before the vtable definition.
vtable_anchor = "static const struct socket_op_vtable nsos_socket_fd_op_vtable = {"
if vtable_anchor not in src:
    sys.stderr.write("ERROR: nsos vtable not found\n"); sys.exit(1)
impl = '''/* nano-ros: nsos getsockname — Phase 11W.8. Top-half: ask the host
 * for the bound name via nsos_adapt_getsockname, then translate the
 * NSOS middleground sockaddr back to the Zephyr form. */
static int nsos_getsockname(void *obj, struct sockaddr *addr, socklen_t *addrlen)
{
	struct nsos_socket *sock = obj;
	struct nsos_mid_sockaddr_storage addr_storage_mid;
	struct nsos_mid_sockaddr *addr_mid = (struct nsos_mid_sockaddr *)&addr_storage_mid;
	size_t addrlen_mid = sizeof(addr_storage_mid);
	int ret;

	ret = nsos_adapt_getsockname(sock->poll.mid.fd, addr_mid, &addrlen_mid);
	if (ret < 0) {
		errno = errno_from_nsos_mid(-ret);
		return -1;
	}

	sockaddr_from_nsos_mid(addr, addrlen, addr_mid, addrlen_mid);

	return 0;
}

'''
src = src.replace(vtable_anchor, impl + vtable_anchor, 1)

# Wire .getsockname into the vtable (after .setsockopt line).
setsockopt_line = "\t.setsockopt = nsos_setsockopt,\n"
if setsockopt_line not in src:
    sys.stderr.write("ERROR: vtable .setsockopt line not found\n"); sys.exit(1)
src = src.replace(setsockopt_line,
                  setsockopt_line + "\t.getsockname = nsos_getsockname,\n", 1)

path.write_text(src)
PYEOF

echo "[nsos-getsockname-patch] patched NSOS getsockname into $WORKSPACE"
