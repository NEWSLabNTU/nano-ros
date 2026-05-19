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

# Phase 11W.8 — actsize == 0 tolerance. NSOS getsockopt(SO_*BUF)
# succeeds but reports 0; treat as "stack can't size, continue".
old3 = '''    else
    {
      /* If the configuration states it must be >= X, then error out if the
         kernel doesn't give us at least X */
      GVLOG (DDS_LC_CONFIG | DDS_LC_ERROR,
             "failed to increase socket %s buffer size to at least %"PRIu32" bytes, current is %"PRIu32" bytes\\n",
             name, socket_min_buf_size, actsize);
      rc = DDS_RETCODE_NOT_ENOUGH_SPACE;
    }'''
new3 = '''    else if (actsize == 0)
    {
      /* nano-ros: zephyr unsupported-sockopt — Phase 11W.8. Zephyr
       * NSOS getsockopt(SO_*BUF) succeeds but reports 0 because the
       * host-offloaded socket doesn't surface a buffer size; treat
       * 0 as "stack can't size, continue" (the host socket still has
       * the kernel default buffer). */
      GVLOG (DDS_LC_CONFIG, "socket %s buffer size unreported by stack, continuing\\n", name);
    }
    else
    {
      /* If the configuration states it must be >= X, then error out if the
         kernel doesn't give us at least X */
      GVLOG (DDS_LC_CONFIG | DDS_LC_ERROR,
             "failed to increase socket %s buffer size to at least %"PRIu32" bytes, current is %"PRIu32" bytes\\n",
             name, socket_min_buf_size, actsize);
      rc = DDS_RETCODE_NOT_ENOUGH_SPACE;
    }'''
if old3 not in src:
    sys.stderr.write("ERROR: actsize==0 anchor not found in ddsi_udp.c\\n")
    sys.exit(1)
src = src.replace(old3, new3, 1)

# Phase 11W.8 — multicast TX options best-effort. The IP_MULTICAST_*
# setsockopt family is unsupported / shape-mismatched on Zephyr NSOS;
# evaluate each call but never fail the connection on its result.
old4 = '''  const unsigned char ttl = (unsigned char) gv->config.multicast_ttl;
  const unsigned char loop = (unsigned char) !!gv->config.enableMulticastLoopback;
  dds_return_t rc;
  if ((rc = set_mc_options_transmit_ipv4_if (gv, intf, sock)) != DDS_RETCODE_OK) {
    GVERROR ("ddsi_udp_create_conn: set IP_MULTICAST_IF failed: %s\\n", dds_strretcode (rc));
    return rc;
  }
  if ((rc = ddsrt_setsockopt (sock, IPPROTO_IP, IP_MULTICAST_TTL, &ttl, sizeof (ttl))) != DDS_RETCODE_OK) {
    GVERROR ("ddsi_udp_create_conn: set IP_MULTICAST_TTL failed: %s\\n", dds_strretcode (rc));
    return rc;
  }
  if ((rc = ddsrt_setsockopt (sock, IPPROTO_IP, IP_MULTICAST_LOOP, &loop, sizeof (loop))) != DDS_RETCODE_OK) {
    GVERROR ("ddsi_udp_create_conn: set IP_MULTICAST_LOOP failed: %s\\n", dds_strretcode (rc));
    return rc;
  }
  return DDS_RETCODE_OK;'''
new4 = '''  const unsigned char ttl = (unsigned char) gv->config.multicast_ttl;
  const unsigned char loop = (unsigned char) !!gv->config.enableMulticastLoopback;
  dds_return_t rc;
  /* nano-ros: zephyr unsupported-sockopt — Phase 11W.8. The
   * IP_MULTICAST_* setsockopt family is best-effort on Zephyr NSOS
   * (struct-shape / size differences from upstream POSIX cause
   * BAD_PARAMETER / generic ERROR). Evaluate each call but never
   * fail the connection on its result; embedded multicast group
   * membership is driven by nros-platform-zephyr's IGMP path, not
   * Cyclone's setsockopt. */
#define NROS_MC_OPT_BEST_EFFORT(expr) ((void)(expr))
  NROS_MC_OPT_BEST_EFFORT(rc = set_mc_options_transmit_ipv4_if (gv, intf, sock));
  NROS_MC_OPT_BEST_EFFORT(rc = ddsrt_setsockopt (sock, IPPROTO_IP, IP_MULTICAST_TTL, &ttl, sizeof (ttl)));
  NROS_MC_OPT_BEST_EFFORT(rc = ddsrt_setsockopt (sock, IPPROTO_IP, IP_MULTICAST_LOOP, &loop, sizeof (loop)));
#undef NROS_MC_OPT_BEST_EFFORT
  (void) rc;
  return DDS_RETCODE_OK;'''
if old4 not in src:
    sys.stderr.write("ERROR: multicast-tx anchor not found in ddsi_udp.c\\n")
    sys.exit(1)
src = src.replace(old4, new4, 1)

path.write_text(src)
PYEOF

echo "[cyclonedds-zephyr-udp-rcvbuf-patch] patched $TARGET"
