#!/usr/bin/env bash
# scripts/zephyr/cyclonedds-zephyr-mcjoin-patch.sh
#
# Phase 11W.8 — make Cyclone DDS' SPDP multicast group-join
# best-effort on Zephyr. NSOS cannot complete the ASM
# IP_ADD_MEMBERSHIP join (synthetic loopback interface + NSOS
# multicast struct-shape gaps), so `joinleave_spdp_defmcip` returns
# an error and aborts participant init. On Zephyr, treat the join
# failure as non-fatal and continue unicast-only.
#
# Idempotent.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
NANO_ROS_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
TARGET="$NANO_ROS_ROOT/third-party/dds/cyclonedds/src/core/ddsi/src/q_init.c"

if [ ! -f "$TARGET" ]; then
    echo "ERROR: $TARGET missing" >&2
    exit 1
fi
if grep -q "nano-ros: zephyr best-effort multicast" "$TARGET"; then
    echo "[cyclonedds-zephyr-mcjoin-patch] already applied"
    exit 0
fi

python3 - "$TARGET" <<'PYEOF'
import sys
from pathlib import Path
path = Path(sys.argv[1])
src = path.read_text()
old = '''  if (arg.errcount)
  {
    GVERROR ("rtps_init: failed to join multicast groups for domain %"PRIu32" participant %d\\n", gv->config.domainId, gv->config.participantIndex);
    return -1;
  }
  return 0;'''
new = '''  if (arg.errcount)
  {
#ifdef __ZEPHYR__
    /* nano-ros: zephyr best-effort multicast — Phase 11W.8. Zephyr
     * NSOS cannot complete the ASM IP_ADD_MEMBERSHIP join (synthetic
     * loopback interface + NSOS multicast struct-shape gaps). Treat
     * the join failure as non-fatal so participant init completes;
     * discovery then relies on the unicast path. LAN multicast
     * discovery remains a follow-up. */
    GVWARNING ("rtps_init: multicast join failed for domain %"PRIu32" participant %d; continuing unicast-only (Zephyr NSOS)\\n", gv->config.domainId, gv->config.participantIndex);
    return 0;
#else
    GVERROR ("rtps_init: failed to join multicast groups for domain %"PRIu32" participant %d\\n", gv->config.domainId, gv->config.participantIndex);
    return -1;
#endif
  }
  return 0;'''
if old not in src:
    sys.stderr.write("ERROR: joinleave_spdp_defmcip errcount anchor not found\n")
    sys.exit(1)
src = src.replace(old, new, 1)
path.write_text(src)
PYEOF

echo "[cyclonedds-zephyr-mcjoin-patch] patched $TARGET"
