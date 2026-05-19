#!/usr/bin/env bash
# scripts/zephyr/cyclonedds-zephyr-sockwaitset-patch.sh
#
# Phase 11W.8 — give Cyclone DDS' socket waitset a Zephyr-compatible
# self-pipe. The generic POSIX `make_pipe` uses `pipe(2)`, but the
# resulting fds are not NSOS socket fds and cannot be watched by the
# NSOS socket-poll waitset — `os_sockWaitsetTrigger` then fails to
# write the trigger byte and publisher creation fails.
#
# Replace it (under __ZEPHYR__) with a loopback TCP socket pair, the
# same technique the upstream _WIN32 path already uses. Both ends are
# real NSOS socket fds, so poll() watches them and read()/write()
# route through the host kernel. Relies on the NSOS getsockname
# support added by nsos-getsockname-patch.sh and needs CONFIG_NET_TCP.
#
# Idempotent.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
NANO_ROS_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
TARGET="$NANO_ROS_ROOT/third-party/dds/cyclonedds/src/core/ddsi/src/q_sockwaitset.c"

if [ ! -f "$TARGET" ]; then
    echo "ERROR: $TARGET missing" >&2
    exit 1
fi
if grep -q "nano-ros: zephyr self-pipe" "$TARGET"; then
    echo "[cyclonedds-zephyr-sockwaitset-patch] already applied"
    exit 0
fi

python3 - "$TARGET" <<'PYEOF'
import sys
from pathlib import Path
path = Path(sys.argv[1])
src = path.read_text()
old = '''#elif !defined(LWIP_SOCKET)
static int make_pipe (int pfd[2])
{
  return pipe (pfd);
}
#endif'''
new = '''#elif defined(__ZEPHYR__)
/* nano-ros: zephyr self-pipe — Phase 11W.8. Zephyr's pipe() yields
 * fds that the NSOS socket-poll waitset cannot watch. Build a
 * loopback TCP socket pair instead (same approach as the _WIN32
 * path), so both ends are real NSOS socket fds that poll() handles
 * and read()/write() route through the host kernel. Relies on the
 * NSOS getsockname support added in this phase. */
static int make_pipe (int pfd[2])
{
  struct sockaddr_in addr;
  socklen_t asize = sizeof (addr);
  int listener = socket (AF_INET, SOCK_STREAM, 0);
  int s1 = socket (AF_INET, SOCK_STREAM, 0);
  int s2 = -1;

  if (listener < 0 || s1 < 0)
    goto fail;
  memset (&addr, 0, sizeof (addr));
  addr.sin_family = AF_INET;
  addr.sin_addr.s_addr = htonl (INADDR_LOOPBACK);
  addr.sin_port = 0;
  if (bind (listener, (struct sockaddr *)&addr, sizeof (addr)) == -1)
    goto fail;
  if (getsockname (listener, (struct sockaddr *)&addr, &asize) == -1)
    goto fail;
  if (listen (listener, 1) == -1)
    goto fail;
  if (connect (s1, (struct sockaddr *)&addr, sizeof (addr)) == -1)
    goto fail;
  if ((s2 = accept (listener, 0, 0)) < 0)
    goto fail;
  close (listener);
  pfd[0] = s2;  /* read end  (waitset reads triggers here) */
  pfd[1] = s1;  /* write end (os_sockWaitsetTrigger writes here) */
  return 0;

fail:
  if (listener >= 0) close (listener);
  if (s1 >= 0) close (s1);
  if (s2 >= 0) close (s2);
  return -1;
}
#elif !defined(LWIP_SOCKET)
static int make_pipe (int pfd[2])
{
  return pipe (pfd);
}
#endif'''
if old not in src:
    sys.stderr.write("ERROR: make_pipe generic anchor not found\n")
    sys.exit(1)
src = src.replace(old, new, 1)
path.write_text(src)
PYEOF

echo "[cyclonedds-zephyr-sockwaitset-patch] patched $TARGET"
