---
id: 852
title: "the zenoh read task inherits the executor's priority on Zephyr — the
  declared priority is discarded by the port, so a 20 ms timeslice starves the
  polled serial RX and it overruns"
status: open
type: bug
area: rmw, platform
related: [issue-0848, issue-0839, issue-0736, issue-0626, rfc-0079]
---

## Problem

`_z_read_serial_internal` (`src/system/zephyr/network.c`) receives byte by byte:

```c
res = uart_poll_in(sock._serial, &raw_buf[i]);
if (res != 0) { if (past deadline) return SIZE_MAX; k_yield(); }
```

Polled RX is the surface. **The root cause is that the read task runs at the
same priority as the executor, because the priority it was given is thrown
away by this port** — so `k_yield()` in that loop hands the executor a full
scheduler timeslice while bytes keep arriving.

## Root cause: the priority is discarded twice

`CONFIG_NROS_ZENOH_READ_PRIORITY` (default 16 on the normalised 0-31 band) is
wired through Kconfig and CMake, reaches `zpico_set_task_config`, and is stored:

```
zpico.c:1431   g_default_read_nros_attr.priority = NROS_PLATFORM_PRIORITY_RAW(read_priority)
zpico.c:1443   g_default_read_task_opts.task_attributes = (z_task_attr_t *) &g_default_read_nros_attr
```

zenoh-pico then hands that attr to the port. Both Zephyr entry points drop it.

**1. the zenoh-pico system shim** — `zephyr/nros_zenoh_zephyr_system.c:29`

```c
z_result_t _z_task_init(_z_task_t *task, z_task_attr_t *attr,
                        void *(*fun)(void *), void *arg) {
    (void)attr;                                     /* <-- discarded */
    return nros_zephyr_task_create(task, fun, arg) == 0 ? 0 : -1;
}
```

**2. the platform task ABI itself** — `nros-platform-zephyr/src/platform.c:291`

```c
const nros_platform_task_attr_t *a = (const nros_platform_task_attr_t *) attr;
(void) a;                                           /* <-- discarded */
```

The comment there explains why `stack_bytes` cannot be honoured (Zephyr's
`k_thread_create` needs an MPU-aligned `K_THREAD_STACK_DEFINE` region). That
reasoning is sound and does **not** extend to `priority`:
`CONFIG_POSIX_PRIORITY_SCHEDULING=y` in these images and
`pthread_attr_setschedpolicy` / `setschedparam` work.

Neither call site sets `PTHREAD_EXPLICIT_SCHED`, so
`nros_zephyr_task_create`'s bare `pthread_attr_init`
(`nros_platform_zephyr_shims.c:438`) leaves the Zephyr default in place:

```
zephyr/lib/posix/options/pthread.c:950   attr->inheritsched = PTHREAD_INHERIT_SCHED
zephyr/lib/posix/options/pthread.c:653   -> new thread takes the CREATOR's priority
```

The read task is started from the executor thread, so it inherits
`CONFIG_MAIN_THREAD_PRIORITY=0` — identical to the executor.

### This is the failure issue 0626 warned about, one layer down

Issue 0626 fixed the Zephyr arm of `zpico_set_task_config`, and its own comment
says:

> `PTHREAD_EXPLICIT_SCHED` is load-bearing: the default is
> `PTHREAD_INHERIT_SCHED`, under which the policy and param set below are
> IGNORED and the new thread silently takes the creator's. A scheduling
> attribute that is quietly dropped is the failure this issue is about, so it
> must not be reintroduced one layer down.

It was reintroduced one layer down, in nano-ros's own port. Issue 0736 records
the same shape on NuttX ("zenoh read/lease threads inherit whichever thread
opened the session"), and [RFC-0079](../design/0079-priority-is-allocated-not-authored.md)
is the general statement of it.

## Why equal priority loses bytes — arithmetic, not a race

```
CONFIG_TIMESLICING=y
CONFIG_TIMESLICE_SIZE=20      ms
CONFIG_TIMESLICE_PRIORITY=0   every preemptible thread is timesliced
```

At equal priority `k_yield()` moves the reader to the tail of its ready queue
and the executor gets a full slice. 20 ms at 115200 baud is **~230 bytes**
against an LPUART RX FIFO a few entries deep.

The loop's own comment shows the trap was seen but mis-scoped:

> Yield rather than sleep between polls: at 115200 a byte is 87 us and the
> shortest `z_sleep_ms(1)` blocks a whole tick, overrunning the UART.

`k_yield()` at equal priority blocks for a whole **timeslice**, which is 20x
worse than the tick the comment was avoiding.

## Proof

Instrumented read loop reporting `uart_err_check`, against a locally built
zenoh router logging every serial write:

```
DIAG-RX: frame rb=9  ok hdr=0x03  uart_err=0x0     <- handshake, board idle
DIAG-RX: frame rb=83 ok hdr=0x00  uart_err=0x0
DIAG-RX: frame rb=15 ok hdr=0x00  uart_err=0x0
DIAG-RX: timeout, rb=4  uart_err=0x1 OVERRUN       <- keepalive frame, truncated
DIAG-RX: timeout, rb=0  uart_err=0x0  x many       <- nothing left to lose
```

The overrun flag is set on **exactly** the truncated frame and nowhere else.

## Why it looks load-dependent

The priority story explains the split completely:

- **handshake survives** — the executor is blocked in its wake wait, so the
  read task is the only runnable thread and `k_yield()` returns immediately
- **keepalives die** — three queryables, two publishers and their callbacks
  make the executor runnable, so each `k_yield()` costs a full 20 ms slice
- **the talker soaks for five minutes** — one publisher, executor blocked
  almost always, reader keeps up

That is the whole reason this presented as "actions are broken and pub/sub is
fine". No amount of stack or lease tuning could move it.

## The red herring worth recording

[Issue 0848](archived/0848-router-sends-no-keepalives-on-serial.md) chased this
as a router defect for a long time, ending on "the keepalive is a 1-byte write
that never frames". The 1-byte figure was the payload handed to the link;
z-serial frames it as `header(1) + len(2) + payload + crc32(4)` and
COBS-encodes it, so ~10 bytes reach the wire. The board caught 4 of them and
overran. **The small write was never the anomaly — the receiver was.**

The router is exonerated: its timer fires, the keepalive arm fires, `write_all`
+ `flush` both succeed, and the frames it emits are well formed.

## Fix

The board uses `uart_mcux_lpuart.c`, not the LinFlexD driver — worth stating
because it changes what is available:

```
CONFIG_UART_MCUX_LPUART=y
CONFIG_UART_NXP_LPUART_ASYNC_API_SUPPORT=y   eDMA path exists
CONFIG_SERIAL_SUPPORT_INTERRUPT=y
CONFIG_SERIAL_SUPPORT_ASYNC=y
```

Neither `CONFIG_UART_INTERRUPT_DRIVEN` nor `CONFIG_UART_ASYNC_API` is enabled.
Both are available, and `mcux_lpuart_fifo_read` drains the whole hardware FIFO
in a loop (unlike LinFlexD's one-byte shim), so the ISR path is efficient here.

Cheapest first:

1. **Honour the declared priority.** Pass `attr` through `_z_task_init`, give
   `nros_zephyr_task_create` a priority parameter, set `PTHREAD_EXPLICIT_SCHED`
   + `SCHED_FIFO` + the mapped band value. Fix the platform ABI's `task_init`
   in the same change — it accepts a priority it silently discards on every
   Zephyr image, which is a latent RT defect independent of this issue.
   Does not remove polling; removes the 20 ms starvation window.
2. **Call `uart_err_check` and surface overruns.** Diagnostics only. Keep it:
   invisibility is what let this hide behind six other hypotheses.
3. **`CONFIG_UART_INTERRUPT_DRIVEN=y`** — ISR does `uart_fifo_read` into a
   `ring_buf`, the reader blocks on a `k_sem`. RX stops depending on thread
   scheduling at all. This is the actual fix.
4. **`CONFIG_UART_ASYNC_API=y` + eDMA** — zero per-byte CPU, double-buffered
   via `UART_RX_BUF_REQUEST`. The real-time target.

1 and 2 are small and independently correct; land them first. 3 is the fix.

## Status — what has landed and what it measured

**Landed (steps 1 and 2).** The priority is honoured at all three sites that
dropped it, and overruns are reported:

- `_z_task_init` forwards `attr` to `nros_platform_task_init` instead of
  `(void)attr;`
- the Zephyr platform port maps the band and applies it with
  `PTHREAD_EXPLICIT_SCHED`, via a new `nros_zephyr_task_create_prio`
- **SCHED_RR, not SCHED_FIFO.** On Zephyr `SCHED_FIFO` selects the COOPERATIVE
  band (`lib/posix/options/pthread_sched.h`), and a cooperative busy-poller
  would never be preempted — trading a 20 ms starvation of the reader for an
  unbounded starvation of everything else. The posix port picks SCHED_FIFO
  because on Linux that is simply "the real-time policy"; the same constant
  means something different here, which is why the map is per-port.
- `CONFIG_MAIN_THREAD_PRIORITY=5` on the serial images. Necessary: SCHED_RR
  maps as `zephyr = NUM_PREEMPT - posix - 1`, so the top preemptible slot is
  Zephyr priority 0 — exactly where main sat by default. The reader cannot be
  placed above a main thread already holding the top slot, so honouring the
  priority alone changes nothing.

Measured on the action image, clean router, five goals per run:

| | goals completing |
| --- | --- |
| before | **0 of 5** — the graph populated, no goal ever finished |
| priority honoured + main lowered | **2 of 5**, and `SUCCEEDED` with the correct sequence |

Real but partial, which is what the polled path predicts: raising the reader's
priority shrinks the window in which it is descheduled and cannot remove it,
because a polling reader still has to be RUNNING to catch a byte.

**Written but NOT enabled (step 3).** Interrupt-driven RX is implemented on the
zenoh-pico fork branch `fix/zephyr-serial-irq-rx`: `uart_fifo_read` in the ISR
into a `ring_buf`, reader blocked on a `k_sem`, falling back to the poll loop
when `CONFIG_UART_INTERRUPT_DRIVEN` is off. Board-side behaviour under it is
clean — full bring-up in 0.27 s, **zero** `uart_err_check` overruns, **zero**
ring overflows.

It ships **disabled**. A later A/B with a `demo_nodes_cpp` talker on the same
router as a live control settled it: with the polled build the board's node
appears in `ros2 node list` alongside the host talker, and with the ISR build
only the host talker appears. The router is provably healthy in the same
instant, so the ISR path does break the board's declarations from reaching it —
the board completes its handshake, registers every token and reports no
overrun, and the declarations still do not land. Cause not yet identified;
suspect the TX side, since `CONFIG_UART_INTERRUPT_DRIVEN` changes driver
behaviour for a path that still uses `uart_poll_out`.

## The measurement is confounded — [issue 0864](0864-board-zid-is-identical-on-every-boot.md)

The board presents the **same zenoh id on every boot**
(`1322740661b45746fa29b1803f32f5eb`, verified across resets and reflashes;
`CONFIG_TEST_RANDOM_GENERATOR` with no hardware entropy). A router therefore
cannot tell a rebooted board from the peer it already has, so any run that
crosses a reconnect depends on **router history** as much as on the firmware
under test.

That is not a footnote here. During this work an A/B appeared to show that
interrupt-driven RX broke ROS graph discovery. It did not: the run was against a
router that had failed to start (`Address already in use`, a stale process still
holding 7447) while a previous router still owned the serial port. Repeating it
with a genuinely clean router restored discovery on the polled build, and the
ISR build's remaining difference cannot be separated from the shared-zid effect.

**Issue 0853 should be fixed before step 3 is judged.** Its acceptance test is
two resets and two different zids.

## Adjacent, found while tracing this

`_z_read_serial_internal` calls `z_malloc` **twice per frame**
(`_Z_SERIAL_MAX_COBS_BUF_SIZE` + `_Z_SERIAL_MFS_SIZE`) on the receive hot path.
Unbounded-latency allocator calls inside RX. Belongs to the heap-unification
campaign, not to this fix.

## Impact

Any zenoh-over-serial image whose executor is busy enough to hold a timeslice
loses frames silently. Observed as session expiry at `2 x Z_TRANSPORT_LEASE`
([issue 0839](0839-action-image-session-expires-every-20s.md)), because the
dropped frames are the router's keepalives.
