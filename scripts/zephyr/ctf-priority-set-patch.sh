#!/usr/bin/env bash
# scripts/zephyr/ctf-priority-set-patch.sh <workspace-dir>
#
# Issue 2766 -- 3.7 line only. Makes a priority change reach the CTF stream.
#
# Stock v3.7.0 emits the event and drops it. `z_thread_prio_set()`
# (kernel/sched.c) does call
# `SYS_PORT_TRACING_OBJ_FUNC(k_thread, sched_priority_set, thread, prio)`, and
# `ctf_top.c` does implement `sys_trace_k_thread_priority_set()`, but
# `subsys/tracing/ctf/tracing_ctf.h` maps the first to an EMPTY macro, so a
# capture holds no `thread_priority_set` event whatever the application does,
# and the absence reads as "no priority change happened". The safety island
# carried this as `patches/zephyr/0001-ctf-trace-k_thread_priority_set.patch`
# (measured on its FVP boot: 0 events before, 53 after).
#
# One line inside the tracing subsystem; the kernel is untouched. Upstream
# fixed it the same way (passing `prio` through as well), and the 4.4 line
# already carries that, so 4.4's set does not run this.
#
# Idempotent. Inert unless an image enables CONFIG_TRACING_CTF.
set -euo pipefail

# shellcheck source=scripts/lib/zephyr-patchset.sh
source "$(cd "$(dirname "${BASH_SOURCE[0]}")/../lib" && pwd)/zephyr-patchset.sh"

WORKSPACE="${1:?usage: ctf-priority-set-patch.sh <workspace-dir>}"
TARGET="$WORKSPACE/zephyr/subsys/tracing/ctf/tracing_ctf.h"
if [ ! -f "$TARGET" ]; then
    echo "ERROR: $TARGET missing" >&2
    exit 1
fi

# Check mode (issue 2831): the guard is in the Python below, so it exits 10
# where it would write, and the shared helper reports the missing edit.
_py_rc=0
python3 - "$TARGET" <<'PYEOF' || _py_rc=$?
import os
import sys

path = sys.argv[1]
src = open(path).read()
MARK = "nano-ros: CTF priority-set event"
if MARK in src:
    print("[ctf-priority-set-patch] already applied")
    sys.exit(0)

old = "#define sys_port_trace_k_thread_sched_priority_set(thread, prio)\n"
new = (
    f"/* {MARK} (issue 2766): stock 3.7 maps this to nothing. */\n"
    "#define sys_port_trace_k_thread_sched_priority_set(thread, prio) \\\n"
    "\tsys_trace_k_thread_priority_set(thread)\n"
)
if src.count(old) != 1:
    sys.stderr.write(f"ERROR: anchor not found once in {path}: {old.strip()}\n")
    sys.exit(1)
if os.environ.get("NROS_PATCH_CHECK") == "1":
    sys.exit(10)
open(path, "w").write(src.replace(old, new, 1))
print(f"[ctf-priority-set-patch] patched {path}")
PYEOF
if [ "$_py_rc" -eq 10 ]; then
    nros_patch_would_apply ctf-priority-set-patch "$TARGET: priority-set event mapping"
fi
exit "$_py_rc"
