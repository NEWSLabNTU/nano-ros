#!/usr/bin/env bash
# scripts/zephyr/cyclonedds-zephyr-threads-patch.sh
#
# Phase 11W.6 — patch Cyclone DDS `src/ddsrt/src/threads/posix/threads.c`
# so the 4 thread-cleanup functions use Zephyr's `k_thread_custom_data`
# slot under `__ZEPHYR__` instead of `pthread_setspecific`.
#
# Why: Cyclone DDS on the POSIX backend stores per-thread cleanup
# state in a `pthread_key_t` TLS slot. Zephyr's POSIX implementation
# rejects `pthread_setspecific` from any thread that is not a
# pthread (its `to_posix_thread(pthread_self())` returns NULL),
# returning EINVAL. Cyclone asserts on EINVAL inside
# `ddsrt_thread_cleanup_push` (file ifaddrs is not the issue — it's
# the cleanup-push hit during participant create from the Zephyr
# main thread). With `CONFIG_ASSERT=y +
# CONFIG_PICOLIBC_ASSERT_VERBOSE=y` the binary panics:
#
#   ASSERTION FAIL [0]
#     assertion "err != EINVAL" failed: file "threads.c", line 538,
#     function: ddsrt_thread_cleanup_push
#   >>> ZEPHYR FATAL ERROR 4: Kernel panic on CPU 0
#
# Fix: switch the TLS storage to `k_thread_custom_data_set/get` on
# Zephyr. Zephyr guarantees one `void *` per thread (including the
# native main thread), no pthread context needed. Single slot is
# sufficient — Cyclone uses exactly one key for this purpose.
#
# Trade-off: any other code in the linked image that already uses
# `k_thread_custom_data` on these threads will clobber Cyclone's
# slot. Documented constraint for the cyclonedds-on-Zephyr path.
#
# Idempotent — detects prior application via marker comment.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
NANO_ROS_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

TARGET="$NANO_ROS_ROOT/third-party/dds/cyclonedds/src/ddsrt/src/threads/posix/threads.c"
if [ ! -f "$TARGET" ]; then
    echo "ERROR: $TARGET missing — submodule not checked out?" >&2
    exit 1
fi

if grep -q "nano-ros: zephyr k_thread_custom_data" "$TARGET"; then
    echo "[cyclonedds-zephyr-threads-patch] already applied"
    exit 0
fi

python3 - "$TARGET" <<'PYEOF'
import sys
from pathlib import Path

path = Path(sys.argv[1])
src = path.read_text()

# Gate the four upstream POSIX function definitions behind
# `#ifndef _NROS_CYC_THREADS_OVERRIDE`, then APPEND the Zephyr
# override block at end of file. Wrapping the originals BEFORE
# inserting the override avoids the prefix-collision bug where
# `str.find()` would match the override's signature first.

# Upstream-specific signatures (note the space-before-paren style
# unique to upstream). The override below uses no-space form so
# str.find() never matches the override's body.
gates = [
    "dds_return_t ddsrt_thread_cleanup_push (void (*routine) (void *), void *arg)",
    "dds_return_t ddsrt_thread_cleanup_pop (int execute)",
    "void ddsrt_thread_init(uint32_t reason)",
    "void ddsrt_thread_fini(uint32_t reason)",
]

def wrap_function(src, sig_marker):
    idx = src.find(sig_marker)
    if idx < 0:
        sys.stderr.write(f"ERROR: signature {sig_marker!r} not found\n")
        sys.exit(1)
    open_idx = src.find("\n{\n", idx)
    if open_idx < 0:
        sys.stderr.write(f"ERROR: opening brace after {sig_marker!r} not found\n")
        sys.exit(1)
    depth = 0
    i = open_idx + 1
    while i < len(src):
        if src[i] == '{':
            depth += 1
        elif src[i] == '}':
            depth -= 1
            if depth == 0:
                end = i + 1
                break
        i += 1
    else:
        sys.stderr.write(f"ERROR: closing brace for {sig_marker!r} not found\n")
        sys.exit(1)
    block = src[idx:end]
    wrapped = "#ifndef _NROS_CYC_THREADS_OVERRIDE\n" + block + "\n#endif"
    return src[:idx] + wrapped + src[end:]

for marker in gates:
    src = wrap_function(src, marker)

override = """

/* nano-ros: zephyr k_thread_custom_data override — Phase 11W.6.
 * Replaces the pthread-key-based TLS used by the 4 functions above
 * (gated out via _NROS_CYC_THREADS_OVERRIDE) with Zephyr's per-thread
 * `void *` slot. Active only when the target builds under Zephyr
 * (__ZEPHYR__ defined by the kernel's autoconf). On any other host
 * (Linux / macOS / FreeBSD POSIX) the original pthread path stays
 * as-is. */
#ifdef __ZEPHYR__
#include <zephyr/kernel.h>
#include "dds/ddsrt/heap.h"

typedef struct _nros_cyc_cleanup {
    void (*routine)(void *);
    void *arg;
    struct _nros_cyc_cleanup *prev;
} _nros_cyc_cleanup_t;

static inline _nros_cyc_cleanup_t *_nros_cyc_head(void) {
    return (_nros_cyc_cleanup_t *)k_thread_custom_data_get();
}

static inline void _nros_cyc_head_set(_nros_cyc_cleanup_t *h) {
    k_thread_custom_data_set(h);
}

dds_return_t ddsrt_thread_cleanup_push(void (*routine)(void *), void *arg) {
    if (routine == NULL) {
        return DDS_RETCODE_BAD_PARAMETER;
    }
    _nros_cyc_cleanup_t *node = ddsrt_calloc(1, sizeof(*node));
    if (node == NULL) {
        return DDS_RETCODE_OUT_OF_RESOURCES;
    }
    node->routine = routine;
    node->arg = arg;
    node->prev = _nros_cyc_head();
    _nros_cyc_head_set(node);
    return DDS_RETCODE_OK;
}

dds_return_t ddsrt_thread_cleanup_pop(int execute) {
    _nros_cyc_cleanup_t *head = _nros_cyc_head();
    if (head == NULL) {
        return DDS_RETCODE_OK;
    }
    _nros_cyc_head_set(head->prev);
    if (execute) {
        head->routine(head->arg);
    }
    ddsrt_free(head);
    return DDS_RETCODE_OK;
}

void ddsrt_thread_init(uint32_t reason) {
    (void)reason;
}

void ddsrt_thread_fini(uint32_t reason) {
    (void)reason;
    _nros_cyc_cleanup_t *cur = _nros_cyc_head();
    while (cur != NULL) {
        _nros_cyc_cleanup_t *prev = cur->prev;
        cur->routine(cur->arg);
        ddsrt_free(cur);
        cur = prev;
    }
    _nros_cyc_head_set(NULL);
}

#endif /* __ZEPHYR__ */
"""

# The override block needs `_NROS_CYC_THREADS_OVERRIDE` to be defined
# BEFORE the gated upstream blocks are encountered by the preprocessor.
# Place a small `#define` near the top of the file (under __ZEPHYR__)
# AND keep the override functions at the bottom. The `#define` sits
# above any `#ifndef _NROS_CYC_THREADS_OVERRIDE` gate.
top_define = """\
/* nano-ros: zephyr k_thread_custom_data override gate — Phase 11W.6 */
#ifdef __ZEPHYR__
#define _NROS_CYC_THREADS_OVERRIDE 1
#endif
"""

# Insert top_define just after the `_GNU_SOURCE` define near the
# very top of the file — this sits BEFORE any conditional
# `#if defined(__APPLE__) / __QNXNTO__` block, so the `#define
# _NROS_CYC_THREADS_OVERRIDE` reaches preprocessor regardless of
# host platform branch.
gnu_anchor = "#define _GNU_SOURCE\n"
gnu_idx = src.find(gnu_anchor)
if gnu_idx < 0:
    sys.stderr.write("ERROR: _GNU_SOURCE anchor not found in threads.c\n")
    sys.exit(1)
insert_at = gnu_idx + len(gnu_anchor)
src = src[:insert_at] + "\n" + top_define + src[insert_at:]

# Append override block.
src = src + override

path.write_text(src)
PYEOF

echo "[cyclonedds-zephyr-threads-patch] patched $TARGET"
