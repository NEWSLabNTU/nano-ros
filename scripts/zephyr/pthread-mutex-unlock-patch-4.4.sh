#!/usr/bin/env bash
# scripts/zephyr/pthread-mutex-unlock-patch-4.4.sh
#
# Phase 180.A — relax Zephyr 4.x's pthread_mutex_unlock to the Linux/glibc
# behaviour for the POSIX PTHREAD_MUTEX_NORMAL/DEFAULT "undefined" cases.
#
# Zephyr maps pthread mutexes onto k_mutex, which enforces owner-only unlock
# (and rejects unlocking an unlocked mutex) with -EPERM. POSIX leaves both
# UNDEFINED for NORMAL/DEFAULT mutexes; Linux/glibc just perform the unlock.
# Cyclone DDS' ddsrt relies on that — its xevent thread unlocks `xevq->lock`
# in a pattern k_mutex rejects, so ddsrt_mutex_unlock() (which abort()s on any
# nonzero return) kills the process right after the first publish. We force a
# releasable state (owner = caller, lock_count >= 1) so the unlock always
# succeeds, matching the platform cyclonedds was written against.
#
# 4.x-only (3.7's pthread/k_mutex path tolerated cyclone's usage). Idempotent.
set -euo pipefail

WORKSPACE="${1:?usage: pthread-mutex-unlock-patch-4.4.sh <workspace-dir>}"
TARGET="$WORKSPACE/zephyr/lib/posix/options/mutex.c"
if [ ! -f "$TARGET" ]; then
    echo "ERROR: $TARGET missing" >&2
    exit 1
fi
if grep -q 'nano-ros (Phase 180.A) — POSIX PTHREAD_MUTEX' "$TARGET"; then
    echo "[pthread-mutex-unlock-patch-4.4] already applied"
    exit 0
fi

python3 - "$TARGET" <<'PYEOF'
import sys
path = sys.argv[1]
src = open(path).read()

old = '''\tm = get_posix_mutex(*mu);
\tif (m == NULL) {
\t\treturn EINVAL;
\t}

\tret = k_mutex_unlock(m);'''

new = '''\tm = get_posix_mutex(*mu);
\tif (m == NULL) {
\t\treturn EINVAL;
\t}

\t/* nano-ros (Phase 180.A) — POSIX PTHREAD_MUTEX_NORMAL/DEFAULT leaves
\t * unlock by a non-owning thread (or unlock of an unlocked mutex)
\t * UNDEFINED; Linux/glibc permit it, and Cyclone DDS' ddsrt relies on
\t * that — its xevent thread unlocks xevq->lock in a pattern Zephyr's
\t * k_mutex rejects (owner != current OR lock_count == 0 -> -EPERM),
\t * which makes ddsrt_mutex_unlock() abort(). Force a releasable state so
\t * the unlock always succeeds, matching the Linux behaviour cyclonedds
\t * was written against. */
\tif (m->owner != k_current_get() || m->lock_count == 0U) {
\t\tm->owner = k_current_get();
\t\tm->lock_count = 1U;
\t}

\tret = k_mutex_unlock(m);'''

if old not in src:
    sys.stderr.write("ERROR: pthread_mutex_unlock anchor not found\n")
    sys.exit(1)
open(path, "w").write(src.replace(old, new, 1))
PYEOF
echo "[pthread-mutex-unlock-patch-4.4] patched $TARGET"
