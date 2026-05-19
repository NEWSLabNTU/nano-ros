#!/usr/bin/env bash
# scripts/zephyr/cyclonedds-zephyr-log-flush-patch.sh
#
# Phase 11W.7 — patch Cyclone DDS log.c on Zephyr so `vlog1` flushes
# every message to the registered sink, not only those ending with
# `\n`. Upstream Cyclone buffers fragmented log messages into a
# thread-local buffer; the sink only fires when a message terminates
# with newline.
#
# Why: on Zephyr we install a Zephyr LOG_INF sink so cyclonedds
# diagnostics surface in `west run` output. When init aborts on a
# fragmented log message (no `\n` yet), no diagnostic ever reaches
# the console — the abort is opaque. Flushing every call gives us
# visibility at the cost of a slightly chattier trace.
#
# Idempotent.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
NANO_ROS_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

TARGET="$NANO_ROS_ROOT/third-party/dds/cyclonedds/src/ddsrt/src/log.c"
if [ ! -f "$TARGET" ]; then
    echo "ERROR: $TARGET missing — submodule not checked out?" >&2
    exit 1
fi

if grep -q "nano-ros: zephyr eager-flush" "$TARGET"; then
    echo "[cyclonedds-zephyr-log-flush-patch] already applied"
    exit 0
fi

python3 - "$TARGET" <<'PYEOF'
import sys
from pathlib import Path

path = Path(sys.argv[1])
src = path.read_text()

# Locate the flush gate inside vlog1.
old = "  if (fmt[strlen (fmt) - 1] == '\\n' && lb->pos > BUF_OFFSET + 1) {"
new = (
    "  /* nano-ros: zephyr eager-flush — Phase 11W.7. Flush every vlog\n"
    "   * call so the registered sink sees every fragment, not only\n"
    "   * messages ending in '\\n'. Required on Zephyr so init-time\n"
    "   * aborts surface a diagnostic before the panic. */\n"
    "#ifdef __ZEPHYR__\n"
    "  if (lb->pos > BUF_OFFSET) {\n"
    "#else\n"
    "  if (fmt[strlen (fmt) - 1] == '\\n' && lb->pos > BUF_OFFSET + 1) {\n"
    "#endif"
)
if old not in src:
    sys.stderr.write("ERROR: vlog1 flush-gate anchor not found in log.c\n")
    sys.exit(1)
src = src.replace(old, new, 1)
path.write_text(src)
PYEOF

echo "[cyclonedds-zephyr-log-flush-patch] patched $TARGET"
