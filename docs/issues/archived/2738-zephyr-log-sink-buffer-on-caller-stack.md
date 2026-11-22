---
id: 2738
title: "The Zephyr log sink formats every record into a 1280-byte buffer on the CALLER's stack"
status: resolved
type: bug
severity: medium
area: [zephyr, log]
found: 2026-11-19
resolved: 2026-11-23
resolved_in: \"the change that archives this issue\"
related: [2165, 2000, phase-478]
---

**Home:** [phase-478](../../roadmap/phase-478-island-developer-experience-findings.md)
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

### Result (2026-11-20, host 241, `native_sim/native/64`): a `strlen` copy, on both lines

```
== 3.7 (v3.7.2)
*** Booting Zephyr OS build v3.7.2 ***
<inf> probe: probe: deferred=1
--- 1 messages dropped ---
<inf> probe: B[OK]
<inf> probe: probe: done

== 4.4 (v4.4.0)
*** Booting Zephyr OS build v4.4.0 ***
<inf> probe: probe: deferred=1
--- 1 messages dropped ---
<inf> probe: B[OK]
<inf> probe: probe: done
```

All three preconditions are present, and neither line prints an `A` line.
Deferred logging copies a RW `%.*s` argument by `strlen` and ignores the
precision. Fix direction 1 would read past nros_log's slice, so it is out.
Direction 2 it is.

## Design: direction 2, a sink-owned buffer pool

- **The pool.** `CONFIG_NROS_LOG_SINK_BUFFERS` buffers of
  `NROS_PLATFORM_LOG_BUFSZ` bytes each, in `.bss`, owned by the sink. A slot
  is claimed with a compare-and-swap on one atomic bitmap, with no lock, so
  the claim is safe from an ISR and across CPUs. The sink formats into the
  slot, calls `LOG_*("%s", slot)` and releases the slot when `LOG_*`
  returns. In deferred mode Zephyr has copied the NUL-terminated string by
  then (the probe shows it copies); in immediate mode the backend has
  already printed it. So a slot is held only for one format and one log call.
- **The fallback.** When every slot is claimed, the record is formatted into
  a small stack buffer (`NROS_PLATFORM_LOG_FALLBACK_SZ`, 160 bytes), truncated
  and marked as truncated. A counter records each fallback, and the sink says
  once that the pool ran dry, naming the knob. The fallback costs a stack 160
  bytes, not 1280, and it never drops a record.
- **Default count.** `2 * CONFIG_MP_MAX_NUM_CPUS`: one thread-context
  record and one ISR-context record in flight per CPU. That is 2 x 2 x 1280
  = 5 KiB of `.bss` on a two-CPU image, moved there from the stack of
  whichever thread logged.
- **Not changed.** In `CONFIG_LOG_MODE_IMMEDIATE` Zephyr's own log core and
  backend still run on the caller's stack. That cost belongs to Zephyr, not
  to this buffer, and the gate cell below runs deferred.

### Sizing plan (measured on a host with Zephyr headroom)

1. **Exhaustion.** On `qemu_cortex_a53/smp` (2 CPUs), K threads (K = 1, 2,
   4, 8) each log 200 records back to back, plus a timer ISR that logs every
   1 ms. Read the fallback counter with N = 2, 4 and 8. The default stands if
   N = 4 shows no fallback at K = 4.
2. **Stack.** For a logging thread, record `k_thread_stack_space_get` on
   main's sink and on the pool. The expected drop is about 1280 bytes.
3. **RAM.** `just mem-report` on the same image, before and after. The
   `.bss` delta should be N x 1280.
4. **The gate cell (done-when).** A thread with a 1536-byte stack logs 50
   records in deferred mode. On the pool every record reaches the console.
   On main's sink the thread overflows (the negative control).

## Fix: a dead dispatcher holder is loud (fix direction 2)

The pool above takes the 1280 bytes off the caller's stack. A thread can
still die inside a sink, from any other fault, and that half is this one.

- **The guard records its holder.** `nros_log` keeps, beside
  `DISPATCH_GUARD`, the address of a stack slot in the frame that claimed it
  (`DISPATCH_HOLDER`, set after the claim, cleared before the release). A
  stack address is an identity every `no_std` target has for free, where a
  thread id is not, and it is never dereferenced. An interrupt that claims the
  guard leaves an address on the interrupt stack, which no thread's stack
  contains. Still lock-free and ISR-safe: one more atomic store on each side.
  `nros_log::dispatch_holder_frame()` and
  `release_dead_dispatch_holder(frame)` are the platform-neutral half, with C
  spellings `nros_log_dispatch_holder_frame` /
  `nros_log_release_dead_dispatch_holder`.
- **The Zephyr port attributes it.**
  `nros_zephyr_log_dead_holder_check(thread)` (`nros-platform-zephyr`,
  `platform.c`) asks whether the holder's frame lies inside the dying
  thread's `stack_info`. If it does, it prints ONE line through `printk`
  (`nros: thread <name> (<ptr>) died holding the nros_log dispatcher;
  released it, ...`) and releases the guard. Weak references to the Rust
  symbols, so an image without `nros_log` links. Without
  `CONFIG_THREAD_STACK_INFO` it says nothing rather than guess.
- **Who calls it.** `CONFIG_NROS_FAULT_LOG`'s `k_sys_fatal_error_handler`
  does. That handler halts, so there the line is the whole value. Stock
  Zephyr's default handler halts too. A thread dies while the system runs on
  only under an application handler that RETURNS, and such a handler must call
  the check. The changelog says so.

**Release, not poison, and why.** A record that meets a held guard goes to
the hold queue for the HOLDER to deliver (issue 2000). A dead holder delivers
nothing, so keeping the guard (or poisoning it) loses every later record,
which is exactly the silence this is about. Releasing does not break the
guard's promise, one caller inside the sinks at a time, because a dead thread
can never re-enter one. The next record claims the guard, is delivered, and
drains what was held meanwhile (after itself, so held records arrive one place
late). What release loses:
- the record the dead thread was delivering;
- if it died while draining, the hold-queue slot it had taken stays
  `READING`, so the queue is one slot smaller until reboot;
- a sink it died inside sees the next call without the end of the last one. A
  sink that takes a lock the dead thread can own may then block where it
  would have been skipped. The platform sink's own pool slot is one such
  state: a death inside `nros_platform_log_write` leaks its buffer, so later
  records take the 160-byte fallback (truncated, still delivered). Freeing
  the pool slot as well is not done here.

Not covered: a thread killed by another thread's `k_thread_abort` while it
holds the guard. Zephyr's only per-abort hook (`CONFIG_THREAD_ABORT_HOOK`,
3.7 and 4.4) is one hidden global symbol that CMSIS-RTOS v2 already defines,
and it runs under the scheduler lock, where `printk` can reach the log core.

Tests:
- `packages/core/nros-log/tests/dead_dispatch_holder.rs` (host): a thread
  parks forever inside a sink. A record raised then is held and not
  delivered. A release naming the wrong frame, or `0`, does nothing. The
  holder's frame releases it once. The next record delivers both itself and
  the stranded one. Mutation: dropping the guard store from the release fails
  it (`["before"]` only).
- Cell `bins/log-dead-holder-zephyr-smp` + `tests/log_dead_holder.rs`
  (`qemu_cortex_a53/smp`, fixture `build-log-dead-holder-smp`). A C sink
  `k_oops`es on a `DIE` record, and the image's fatal handler returns. Phase 1:
  `victim1` dies, the handler runs the check, there is one report naming it,
  and main's next record reaches the console. Phase 2, the negative control in
  the same boot: `victim2` dies with the check skipped, there is no report,
  and main's next record never arrives.

**Run 2026-11-23 on host 243, at 2b9325d549 over main 1affea9051.** The
fixture built (rc=0), and
`a_dead_dispatcher_holder_is_reported_once_and_later_records_are_delivered`
passed (4.2 s), so both phases held in one boot: the report and the delivery
with the check, neither without it. The same run's full `just ci gate` was
green, which includes the host test above.

    source ./activate.sh
    NROS_ZEPHYR_FIXTURE_FILTER=log-dead-holder just zephyr build-fixtures
    cargo nextest run -p nros-tests --test log_dead_holder

## Done when

A Zephyr SMP image (`qemu_cortex_a53/smp`) with a thread on a small stack
that logs runs as a gate cell. It shows the records from that thread reaching
the console, and it fails on main's sink (the negative control).

Both halves are met: the small-stack cell (`log-sink-small-stack-zephyr-smp`,
with its negative control) landed with the sink's measured buffer, and the
dead-holder cell above ran green. Concurrent loggers overflowing the hold
queue are a separate issue, 2762.

Issue 2762 (2026-11-23) made the Zephyr console sink re-entrant: it gets every
record directly, in the raising context, without the guard. So a dead holder
no longer silences the default sink at all; it can strand only records owed to
a sink that is not re-entrant (an application's `rclc_log_add_sink` sink, as
in the cell above). The release above still matters for those, and the cell
now reads its verdict from that serialised sink.
With 2762's two-per-CPU default, the small-stack cell's pool row on
`qemu_cortex_a53/smp` should print `sink buffers 4, cpus 2`.
