#!/usr/bin/env bash
# scripts/zephyr/nsos-recvmsg-patch.sh
#
# Phase 11W.10 — implement recvmsg() in Zephyr's Native Simulator
# Offloaded Sockets (NSOS). Upstream leaves `nsos_recvmsg` an
# `errno = ENOTSUP; return -1;` stub.
#
# Cyclone DDS' UDP receive path (`ddsi_udp_conn_read` →
# `ddsrt_recvmsg` → `recvmsg`) therefore fails on every call with
# ENOTSUP → DDS_RETCODE_ERROR. The Cyclone recv thread treats that
# as a hard error, never blocks, and busy-spins logging
# `UDP recvmsg sock N: ret 0 retcode -1` — on single-core native_sim
# this starves the publish thread.
#
# Implement the single-iovec form (which Cyclone uses: one iovec
# holding the whole datagram + msg_name carrying the source address)
# by delegating to the existing `nsos_recvfrom` path. That reuses the
# blocking/poll + NSOS-middleground sockaddr translation already in
# place, so recvmsg now blocks for data and returns the source
# address like a real BSD recvmsg.
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

TARGET="$WORKSPACE/zephyr/drivers/net/nsos_sockets.c"
if [ ! -f "$TARGET" ]; then
    echo "ERROR: $TARGET missing" >&2
    exit 1
fi
if grep -q "nano-ros: nsos recvmsg" "$TARGET"; then
    echo "[nsos-recvmsg-patch] already applied"
    exit 0
fi

python3 - "$TARGET" <<'PYEOF'
import sys
from pathlib import Path
path = Path(sys.argv[1])
src = path.read_text()
old = '''static ssize_t nsos_recvmsg(void *obj, struct msghdr *msg, int flags)
{
	errno = ENOTSUP;
	return -1;
}'''
new = '''static ssize_t nsos_recvmsg(void *obj, struct msghdr *msg, int flags)
{
	/* nano-ros: nsos recvmsg — Phase 11W.10. Upstream left this an
	 * ENOTSUP stub; Cyclone DDS' UDP read uses recvmsg, so every
	 * receive failed and busy-spun the recv thread. Delegate the
	 * single-iovec form (Cyclone uses one iovec + msg_name) to
	 * nsos_recvfrom, reusing its poll/block + sockaddr translation. */
	void *buf = NULL;
	size_t len = 0;
	socklen_t namelen;
	ssize_t ret;

	if (msg == NULL) {
		errno = EINVAL;
		return -1;
	}
	for (size_t i = 0; i < msg->msg_iovlen; i++) {
		if (msg->msg_iov && msg->msg_iov[i].iov_len > 0) {
			buf = msg->msg_iov[i].iov_base;
			len = msg->msg_iov[i].iov_len;
			break;
		}
	}

	namelen = msg->msg_namelen;
	ret = nsos_recvfrom(obj, buf, len, flags,
			    (struct sockaddr *)msg->msg_name, &namelen);
	if (ret >= 0) {
		msg->msg_namelen = namelen;
		msg->msg_flags = 0;
	}
	return ret;
}'''
if old not in src:
    sys.stderr.write("ERROR: nsos_recvmsg stub anchor not found\n")
    sys.exit(1)
src = src.replace(old, new, 1)
path.write_text(src)
PYEOF

echo "[nsos-recvmsg-patch] patched $TARGET"
