#!/usr/bin/env bash
# scripts/zephyr/cyclonedds-zephyr-udp-rcvbuf-patch.sh
#
# Phase 11W.7 — patch Cyclone DDS `ddsi_udp.c` so `set_socket_buffer`
# treats DDS_RETCODE_UNSUPPORTED the same as DDS_RETCODE_BAD_PARAMETER
# (skip-with-info, not error+abort).
#
# Why: Zephyr NSOS implements only the BSD socket subset its drivers
# need; `getsockopt(SO_RCVBUF)` and `getsockopt(SO_SNDBUF)` are not in
# that subset and return DDS_RETCODE_UNSUPPORTED. Upstream Cyclone
# only special-cases BAD_PARAMETER, so the UNSUPPORTED return surfaces
# as a `GVERROR` and the participant init aborts. Adding UNSUPPORTED
# to the skip list lets Cyclone proceed with default socket buffer
# sizes when the underlying stack can't report the chosen size.
#
# Idempotent.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
NANO_ROS_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

TARGET="$NANO_ROS_ROOT/third-party/dds/cyclonedds/src/core/ddsi/src/ddsi_udp.c"
if [ ! -f "$TARGET" ]; then
    echo "ERROR: $TARGET missing — submodule not checked out?" >&2
    exit 1
fi

if grep -q "nano-ros: zephyr unsupported-sockopt" "$TARGET"; then
    echo "[cyclonedds-zephyr-udp-rcvbuf-patch] already applied"
    exit 0
fi

python3 - "$TARGET" <<'PYEOF'
import sys
from pathlib import Path

path = Path(sys.argv[1])
src = path.read_text()

old1 = '''  rc = ddsrt_getsockopt (sock, SOL_SOCKET, socket_option, &actsize, &optlen);
  if (rc == DDS_RETCODE_BAD_PARAMETER)
  {
    /* not all stacks support getting/setting RCVBUF */
    GVLOG (DDS_LC_CONFIG, "cannot retrieve socket %s buffer size\\n", name);
    return DDS_RETCODE_OK;
  }
  else if (rc != DDS_RETCODE_OK)'''
new1 = '''  rc = ddsrt_getsockopt (sock, SOL_SOCKET, socket_option, &actsize, &optlen);
  /* nano-ros: zephyr unsupported-sockopt — Phase 11W.7. NSOS returns
   * DDS_RETCODE_UNSUPPORTED for getsockopt(SO_RCVBUF/SO_SNDBUF);
   * treat as BAD_PARAMETER (skip-with-info, no abort). */
  if (rc == DDS_RETCODE_BAD_PARAMETER || rc == DDS_RETCODE_UNSUPPORTED)
  {
    /* not all stacks support getting/setting RCVBUF */
    GVLOG (DDS_LC_CONFIG, "cannot retrieve socket %s buffer size\\n", name);
    return DDS_RETCODE_OK;
  }
  else if (rc != DDS_RETCODE_OK)'''

if old1 not in src:
    sys.stderr.write("ERROR: first sockopt anchor not found in ddsi_udp.c\\n")
    sys.exit(1)
src = src.replace(old1, new1, 1)

old2 = '''    if ((rc = ddsrt_getsockopt (sock, SOL_SOCKET, socket_option, &actsize, &optlen)) != DDS_RETCODE_OK)
    {
      GVERROR ("ddsi_udp_create_conn: get %s failed: %s\\n", socket_option_name, dds_strretcode (rc));
      return rc;
    }'''
new2 = '''    if ((rc = ddsrt_getsockopt (sock, SOL_SOCKET, socket_option, &actsize, &optlen)) != DDS_RETCODE_OK)
    {
      if (rc == DDS_RETCODE_BAD_PARAMETER || rc == DDS_RETCODE_UNSUPPORTED) {
        GVLOG (DDS_LC_CONFIG, "cannot verify socket %s buffer size\\n", name);
        return DDS_RETCODE_OK;
      }
      GVERROR ("ddsi_udp_create_conn: get %s failed: %s\\n", socket_option_name, dds_strretcode (rc));
      return rc;
    }'''
if old2 not in src:
    sys.stderr.write("ERROR: second sockopt anchor not found in ddsi_udp.c\\n")
    sys.exit(1)
src = src.replace(old2, new2, 1)

path.write_text(src)
PYEOF

echo "[cyclonedds-zephyr-udp-rcvbuf-patch] patched $TARGET"
