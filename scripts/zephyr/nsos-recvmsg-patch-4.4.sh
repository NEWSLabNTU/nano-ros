#!/usr/bin/env bash
# scripts/zephyr/nsos-recvmsg-patch-4.4.sh
#
# Phase 180.A (4.4 port of Phase 11W.10) — implement recvmsg() in Zephyr
# 4.x's Native Simulator Offloaded Sockets (NSOS). Upstream leaves
# `nsos_recvmsg` an ENOTSUP stub; Cyclone DDS' UDP read uses recvmsg, so
# every receive fails (`UDP recvmsg sock N: ret 0 retcode -1`) and busy-spins
# the recv thread. We delegate the single-iovec form (Cyclone uses one iovec
# + msg_name) to nsos_recvfrom, reusing its poll/block + sockaddr translation.
#
# 4.x differs from the 3.7 patch: the signature uses `struct net_msghdr`
# (renamed from `struct msghdr`) and `nsos_recvfrom` takes `struct
# net_sockaddr *` / `net_socklen_t *`. Idempotent.
set -euo pipefail

WORKSPACE="${1:?usage: nsos-recvmsg-patch-4.4.sh <workspace-dir>}"
TARGET="$WORKSPACE/zephyr/drivers/net/nsos_sockets.c"
if [ ! -f "$TARGET" ]; then
    echo "ERROR: $TARGET missing" >&2
    exit 1
fi
if grep -q "nano-ros: nsos recvmsg" "$TARGET"; then
    echo "[nsos-recvmsg-patch-4.4] already applied"
    exit 0
fi

python3 - "$TARGET" <<'PYEOF'
import sys
path = sys.argv[1]
src = open(path).read()

old = '''static ssize_t nsos_recvmsg(void *obj, struct net_msghdr *msg, int flags)
{
\terrno = ENOTSUP;
\treturn -1;
}'''

new = '''static ssize_t nsos_recvmsg(void *obj, struct net_msghdr *msg, int flags)
{
\t/* nano-ros: nsos recvmsg (Phase 180.A, 4.4 port of Phase 11W.10).
\t * Upstream leaves this an ENOTSUP stub; Cyclone DDS' UDP read uses
\t * recvmsg, so every receive fails and busy-spins the recv thread
\t * ("UDP recvmsg sock N: ret 0 retcode -1"). Delegate the single-iovec
\t * form (Cyclone uses one iovec + msg_name) to nsos_recvfrom, reusing
\t * its poll/block + sockaddr translation. */
\tvoid *buf = NULL;
\tsize_t len = 0;
\tnet_socklen_t namelen;
\tssize_t ret;

\tif (msg == NULL) {
\t\terrno = EINVAL;
\t\treturn -1;
\t}
\tfor (size_t i = 0; i < msg->msg_iovlen; i++) {
\t\tif (msg->msg_iov && msg->msg_iov[i].iov_len > 0) {
\t\t\tbuf = msg->msg_iov[i].iov_base;
\t\t\tlen = msg->msg_iov[i].iov_len;
\t\t\tbreak;
\t\t}
\t}

\tnamelen = msg->msg_namelen;
\tret = nsos_recvfrom(obj, buf, len, flags,
\t\t\t    (struct net_sockaddr *)msg->msg_name, &namelen);
\tif (ret >= 0) {
\t\tmsg->msg_namelen = namelen;
\t\tmsg->msg_flags = 0;
\t}
\treturn ret;
}'''

if old not in src:
    sys.stderr.write("ERROR: nsos_recvmsg 4.4 stub anchor not found\n")
    sys.exit(1)
open(path, "w").write(src.replace(old, new, 1))
PYEOF
echo "[nsos-recvmsg-patch-4.4] patched $TARGET"
