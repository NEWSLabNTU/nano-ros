---
id: 2738
title: "The Zephyr log sink formats every record into a 1280-byte buffer on the CALLER's stack"
status: open
type: bug
severity: medium
area: [zephyr, log]
found: 2026-11-19
related: [2165, 2000, phase-478]
---

**Home:** [phase-478](../roadmap/phase-478-island-developer-experience-findings.md)
(an island finding: it was the first suspect for issue 2165). Owner: nros 139.

## What the code does

`nros_platform_log_write`, the Zephyr port's `nros_log` sink
(`packages/platform/nros-platform-zephyr/src/platform.c:1610`, main
93368ca929), declares `char buf[NROS_PLATFORM_LOG_BUFSZ]` (1280 bytes, line
1597) on the stack of whichever thread raised the record, `snprintf`s
`"<name>: <msg>"` into it, then calls `LOG_ERR/WRN/INF/DBG("%s", buf)`. In
`CONFIG_LOG_MODE_IMMEDIATE` the log core and its backend also run on that
stack.

So every `nros_log` call costs a thread at least 1280 bytes plus `snprintf`
plus the immediate-mode log path. A Zephyr thread sized for its own work
(1-2 KiB is common for a sensor or watchdog thread) overflows the first time
it logs. The record that overflows is raised inside `nros_log`'s dispatch, so
the thread dies holding `nros_log::DISPATCH_GUARD` (issue 2000). After that,
every record from every thread goes to the early ring, fills it, and is
counted lost, and the loss report never runs because it needs the same guard.

## Why this is filed although 2165 closed

Issue 2165 (four records, then silence, on ASI's SMP image) was this
mechanism's first suspect. It does not reproduce at main: on ASI's own image
with its workaround compiled out, `DISPATCH_GUARD`, `early::PENDING` and
`early::LOST` read 0 at 10, 31, 61 and 121 s and every record reached the
console. That run had no small-stack thread logging. The hazard in the code
is unchanged, and nothing reports it when it fires.

## Fix direction

1. **No caller-stack buffer.** Pass name and message to the log macro
   directly (`"%.*s: %.*s"`), or format into a buffer the sink owns. Under the
   deferred log mode Zephyr copies `%s` arguments into the message package by
   reading up to a NUL, and `nros_log`'s message is a length-delimited slice,
   not NUL-terminated. Check what Zephyr 3.7 and 4.4 copy for `%.*s` before
   choosing the first option. A sink-owned buffer must be safe across CPUs on
   SMP and across preemption.
2. **A dead guard holder is loud.** If a thread dies while it holds the
   dispatch guard, say so once on the console (the fatal-error hook can check
   the guard's owner), instead of losing every later record silently.

## Probe: does deferred logging copy a `%.*s` argument by its precision?

Fix direction 1 is safe only if Zephyr copies a RW `%.*s` argument by the
PRECISION. If it copies by `strlen`, it reads past nros_log's slice, which has
no NUL. Zephyr's sources leave this open, so measure it once on each line.
The probe needs no nano-ros. It runs on a host with Zephyr headroom
(manager ruling 2026-11-20: host 139 runs light steps only).

**The image.** Write these three files into an empty scratch directory `probe/`:

`probe/CMakeLists.txt`
```cmake
cmake_minimum_required(VERSION 3.20.0)
find_package(Zephyr REQUIRED HINTS $ENV{ZEPHYR_BASE})
project(log_precision_probe)
target_sources(app PRIVATE src/main.c)
```

`probe/prj.conf`
```
CONFIG_LOG=y
CONFIG_LOG_MODE_DEFERRED=y
CONFIG_LOG_PROCESS_THREAD=y
# Smaller than a strlen copy of the argument below (2006 bytes), larger
# than a precision copy (6). The difference is what the probe sees.
CONFIG_LOG_BUFFER_SIZE=1024
```

`probe/src/main.c`
```c
#include <string.h>
#include <zephyr/kernel.h>
#include <zephyr/logging/log.h>
LOG_MODULE_REGISTER(probe, LOG_LEVEL_INF);

/* A 5-byte slice, then 2000 more non-NUL bytes, then the only NUL: the shape
 * of an nros_log message, which is length-delimited and not NUL-terminated. */
static char rw[2100];

int main(void)
{
    memset(rw, 'X', sizeof(rw));
    memcpy(rw, "HELLO", 5);
    rw[2005] = '\0';

    LOG_INF("probe: deferred=%d", IS_ENABLED(CONFIG_LOG_MODE_DEFERRED));
    k_msleep(100);

    /* A: the slice through %.*s, from RW memory. */
    LOG_INF("A[%.*s]", 5, rw);
    /* Overwritten before the log thread runs: a COPIED argument still prints
     * HELLO, an argument kept by pointer prints WORLD. */
    memcpy(rw, "WORLD", 5);
    k_msleep(100);

    /* B: the control, a NUL-terminated RW string. */
    char ok[3] = "OK";
    LOG_INF("B[%s]", ok);
    k_msleep(100);

    LOG_INF("probe: done");
    k_msleep(200);
    return 0;
}
```

**The two builds.** `native_sim/native/64`, once per line. The question is
about cbprintf packaging, which is the same on every architecture.

```bash
source ./activate.sh
. scripts/lib/zephyr-workspace.sh
for line in 3.7 4.4; do
    ws="$(nros_zephyr_ws_resolve_abs "$line")"
    ( cd "$ws" && ZEPHYR_BASE="$ws/zephyr" ZEPHYR_TOOLCHAIN_VARIANT=host \
        west build -p always -b native_sim/native/64 -d "/tmp/probe-$line" /path/to/probe )
    timeout 5 "/tmp/probe-$line/zephyr/zephyr.exe" > "/tmp/probe-$line.log" 2>&1 || true
    echo "== $line"; cat "/tmp/probe-$line.log"
done
```

**What to look for.** First, both logs must show `probe: deferred=1`, `B[OK]`
and `probe: done`; without all three, the run is not a verdict. Then the `A`
line:

| Console | Meaning | Fix direction 1 |
| --- | --- | --- |
| `A[HELLO]` | copied, by the precision | safe on this line |
| `--- 1 messages dropped ---`, no `A` line | copied by `strlen` (2006 B does not fit the 1024 B buffer) | over-reads the slice: use direction 2 |
| `A[WORLD]` | not copied, kept by pointer | the caller's buffer must outlive the log thread: use direction 2 |

Send both logs, labelled by line, to nros 139.

## Done when

A Zephyr SMP image (`qemu_cortex_a53/smp`) with a thread on a small stack
that logs runs as a gate cell. It shows the records from that thread reaching
the console, and it fails on main's sink (the negative control).
