---
rfc: 0110
title: "Driving zenoh-pico 1.10's executor: one thread where the platform can block, the caller's spin where it cannot"
status: Stable
since: 2026-10
last-reviewed: 2026-10
implements-tracked-by: [phase-508]
amends: [rfc-0016]
supersedes: []
superseded-by: null
---

# RFC-0110 — Driving zenoh-pico 1.10's executor

**Status:** Draft (2026-10-24). Written before any code. It decides how
`zpico.c` drives zenoh-pico once the pin moves to 1.10.1
([phase-508](../roadmap/phase-508-zenoh-pico-1-10-migration.md),
[issue 0910](../issues/0910-zenoh-pico-1-10-migration.md)).

## Summary

zenoh-pico 1.9 replaced the read task and the lease task with an
asynchronous executor (upstream `65f80db8`, #1190). Read, lease, keep-alive,
query timeout and reconnect are now futures in one run queue. How that queue
gets driven depends on the build:

- In a multi-thread build, `zp_start_read_task` starts ONE thread that runs
  the whole queue, and `zp_start_lease_task` does nothing.
- In a single-thread build, nothing runs unless the caller calls
  `zp_spin_once`, and each call runs one task.
- `zp_read` and `zp_send_keep_alive` are compiled out of multi-thread
  builds.

`zpico.c` today drives the session three ways. The background read and lease
tasks serve POSIX, Zephyr, NuttX and FreeRTOS. ThreadX runs no background
task and polls `zp_read` from its tier threads under a try-lock. Bare-metal
smoltcp and serial poll `zp_read`, process query timeouts by hand and send
their own keep-alives. Under 1.10.1 the ThreadX arm no longer compiles, and
the single-thread arms compile but would never run the lease, reconnect or
query-timeout futures.

This RFC keeps two driving modes and names them once:

- **THREAD:** the executor runs on its own thread. This is the background
  executor, used where a blocked receive is a scheduler-visible wait.
- **CALLER:** the executor runs inside `zpico_spin_once`, in the thread that
  calls it. This mode is used where no second thread exists (bare metal), or
  where a blocking receive would stall the RTOS scheduler (ThreadX on its
  Linux host port).

## Motivation

Three facts about 1.10.1, all read in its source:

1. **One thread runs everything.** The background executor holds its mutex
   while a task runs (`src/runtime/background_executor.c:56-83`).
   - A read future blocked in `recv` therefore delays the lease and
     keep-alive futures by up to `Z_CONFIG_SOCKET_TIMEOUT`.
   - Any app thread that spawns or cancels a future waits for the current
     poll to finish.
   - In 1.8.0 the read and lease threads could each block without stopping
     the other.
2. **Single-thread builds need a different call.** `zp_spin_once` runs ONE
   task per call (`src/runtime/executor.c:104-148`). It returns `false` only
   when the read future reports idle, which happens only when
   `Z_RUNTIME_IDLE_READ_TASK_SLEEP > 0` (`src/transport/unicast/read.c:388-393`).
   With the default of 0, a loop "until idle" never ends.
3. **Neither knob has a fallback.** `Z_RUNTIME_MAX_TASKS` and
   `Z_RUNTIME_IDLE_READ_TASK_SLEEP` exist only as `config.h.in`
   substitutions. Our config generator must emit them, or nothing compiles.

The ThreadX arm exists because of a measured hazard (phase-297 W5). On
ThreadX's Linux host port, a host `select` or `recv` blocks the pthread while
the ThreadX scheduler still counts that thread as running, so every other
tier starves. 1.10.1's background executor would put exactly that blocking
receive on a thread of its own.

## Design

### D1 — Two driving modes, chosen in one place

`zpico.c` derives `ZPICO_DRIVE_THREAD` or `ZPICO_DRIVE_CALLER` once, near
its platform block. The conditions that today spell the choice inline,
`Z_FEATURE_MULTI_THREAD == 1 && !defined(ZENOH_THREADX) && !defined(ZPICO_SMOLTCP)`
and its variants, read the macro instead.

| build | `Z_FEATURE_MULTI_THREAD` | mode | why |
| --- | --- | --- | --- |
| POSIX (Linux), NuttX, Zephyr, FreeRTOS (lwIP and SPE) | 1 | THREAD | a blocking receive is an RTOS-visible wait |
| ThreadX (NetX Duo, and the Linux host port) | 1 | CALLER | the host port's receive is invisible to the ThreadX scheduler (phase-297 W5) |
| bare-metal smoltcp, bare-metal serial, the `select` single-thread arm | 0 | CALLER | there is no second thread |

### D2 — THREAD: the read task becomes the executor thread

- **Start:** `zpico_open` calls `zp_start_read_task` once, with the read
  task's attributes. It no longer calls `zp_start_lease_task`.
  `zpico_close` stops the executor with `zp_stop_read_task`; `z_close`
  would stop it anyway (`src/session/utils.c:188`).
- **The lease task's knobs retire.** That covers `zenoh_lease_priority` and
  `zenoh_lease_stack_bytes` in the FreeRTOS scheduling config,
  `ZPICO_LEASE_TASK_PRIORITY` / `NROS_ZENOH_LEASE_PRIORITY`, and the lease
  half of `zpico_set_task_config`.
  - **A setting that names one is REFUSED**, with a message naming this RFC,
    and is never silently ignored. A knob that keeps its name and loses its
    effect is the kind of carrier that caused issues 0460 and 0491.
  - **Issue 0623's transport band** becomes `{executor, poll}`, and
    `report_tiers_above_transport` uses `min(executor, poll)`.
- **The executor's stack is measured, not summed.** One thread now holds the
  read path, the lease and keep-alive sends, the reconnect (a blocking
  connect plus handshake and declaration replay) and every user callback.
  Its default comes from a measured high-water mark on each THREAD platform,
  the way issue 1146 sized `app_stack_bytes`. Until that measurement exists,
  the default is the larger of today's read and lease stacks, and the image
  prints its peak at boot.
- **Unchanged:** the `zpico_spin_once` wait on the session wake primitive
  (`_zpico_notify_spin`), RFC-0106's park, and the tx-flush thread
  (`ZPICO_TX_BATCH_THREAD`).
- **`Z_RUNTIME_IDLE_READ_TASK_SLEEP = 0`.** The receive blocks, so an idle
  read waits in `recv` and needs no sleep.
- **That receive MUST time out** (`Z_CONFIG_SOCKET_TIMEOUT`).
  - The executor checks its timers (lease, keep-alive, query timeout) only
    between receives, so a receive that blocks until data arrives starves
    every timer. Under 1.8 a separate lease thread did not care.
  - Zephyr honours `SO_RCVTIMEO` only with `CONFIG_NET_CONTEXT_RCVTIMEO`,
    which `NROS_RMW_ZENOH` selects; `zpico.c` `#error`s without it (issue
    2105).
  - The timeout bounds how late a timer runs: up to two receives, because a
    ready read runs before a due timer. Measured on NuttX's 5000 ms that
    gives keep-alives every 25–30 s against a 60 s lease (issue 2094).

### D3 — CALLER, single-thread: a bounded `zp_spin_once` loop

`zpico_spin_once(s, timeout_ms)` becomes:

```c
/* sketch, not final code */
uint32_t steps = 0;
while (steps < ZPICO_SPIN_BUDGET && zp_spin_once(z_session_loan(&s->session))) {
    steps++;
}
/* steps == 0: the first step found nothing ready. With timeout_ms > 0, wait
 * (smoltcp poll / z_sleep_ms) and spin again until the deadline. */
```

- **Gone:** `zp_read`, `_z_pending_query_process_timeout` and
  `zp_send_keep_alive` leave these arms. The executor's query-timeout future
  runs every 1 s (`src/session/query.c:64-68`), and the keep-alive future
  sends only when nothing else was sent (`src/transport/unicast/lease.c:183-191`).
  Calling the polled `zp_read` beside the executor would also reset the read
  future's buffered remainder (`read.c:133`). The two paths never mix.
- **`Z_RUNTIME_IDLE_READ_TASK_SLEEP = 1`**, so an idle read returns `false`
  and the loop ends. `ZPICO_SPIN_BUDGET` bounds a flood. It is the
  caller-side twin of the frame-budget redesign of fork commit `00f8e2f0`.
- **The issue 0387 / 1512 rule stays:** an idle spin returns 0, never an
  error code.

### D4 — CALLER, multi-thread (ThreadX): the background executor, run by the caller

ThreadX needs the multi-thread runtime, because tier threads publish
concurrently and need its locks. It must not have the executor thread. Two
pieces make that work:

1. **A fork patch** (zenoh-pico `nano-ros` branch, shaped to go upstream).
   `zp_spin_once` becomes available in multi-thread builds while the
   background executor is NOT started. It runs one `_z_executor_spin` step
   under the executor's mutex, taken with `try_lock`. A caller that loses
   the race returns `false` without blocking ("another tier is draining").
   - This replaces `threadx_read_mutex` and `_zpico_threadx_locked_read`.
     The executor's own mutex is now the single-reader rule from issue #247.
   - Calling it while the thread IS started returns `false` and does
     nothing, so the two modes cannot run at once.
2. **A receive that does not block.** ThreadX's links reach the socket
   through the nano-ros platform net ABI: `zpico-sys/c/zpico/platform_aliases.c`
   forwards each `_z_tcp_*` / `_z_udp_*` link call to `nros_platform_*`,
   which `nros-platform-threadx/src/net.c` implements over NetX Duo BSD (or
   the host sockets, on the Linux port). `zpico-sys/c/platform/threadx/network.c`
   is NOT on that path; nothing has compiled it since phase-80, and an
   earlier draft of this RFC named it by mistake. `nros_platform_tcp_read`
   puts a zero-timeout readiness check in front of `recv` (`select` on the
   Linux port, `nx_bsd_select` on NetX Duo) and returns "no data" when
   nothing is there; `read_exact` keeps a blocking receive, because it is
   reached only mid-frame. Today `zpico_spin_once` makes that same check
   before calling `zp_read`.
   - The socket itself stays blocking, so sends keep their current
     write-all behaviour.
   - Upstream's read future treats a failed or empty receive as idle
     (`read.c:82-115`).
   - The UDP read is NOT changed: no in-tree ThreadX image names a `udp/`
     locator. A UDP unicast session on ThreadX would still block the
     caller in its receive, and needs the same check before it is used.

`zpico_spin_once` then sleeps first (`z_sleep_ms(timeout_ms)`, a real ThreadX
yield; phase-297 W5) and runs the bounded loop from D3.

### D5 — Pinging the link

`ping_session` (`nros-rmw-zenoh/src/shim/session.rs`) calls
`zpico_send_keep_alive`, which calls `zp_send_keep_alive`. That function no
longer exists in multi-thread builds. Both modes call the internal
`_zp_send_keep_alive(_z_session_t *)` (`include/zenoh-pico/net/session.h:298`),
which sends one keep-alive through the transport's TX path.

### D6 — Knobs

`nros-zpico-build`'s `config_header()`:

- **Emits** `Z_RUNTIME_MAX_TASKS` and `Z_RUNTIME_IDLE_READ_TASK_SLEEP`,
  which is 0 under THREAD and 1 under CALLER (D2, D3).
- **`Z_RUNTIME_MAX_TASKS` defaults to 8.** A client session spawns read,
  lease, keep-alive, query timeout and reconnect, which is five. Upstream's
  default of 64 sizes three static containers that each embedded image
  would carry (`include/zenoh-pico/runtime/executor.h:156-185`).
  - A failed spawn is loud only at OPEN, which fails with
    `_Z_ERR_FAILED_TO_SPAWN_TASK`. A reconnect that cannot spawn is
    SILENT: measured at `Z_RUNTIME_MAX_TASKS = 4`, the session never
    resumed and nothing reported why. (This said "already loud" until
    2026-10-27.)
  - The number is a floor to measure, not a guess to keep. It belongs in
    the sizing descriptor (RFC-0100) once peer mode or advanced pub/sub
    adds tasks.
- **Stops emitting** `Z_TRANSPORT_LEASE_TASK_SLEEP_CHUNK_MS` (fork commit
  `23a117d2`, which is obsolete under the executor) and
  `Z_FEATURE_PERIODIC_TASKS`.
- **Re-defines `Z_RX_TASK_BUDGET_*`** under the frame-budget redesign
  (`00f8e2f0`). A budget means "consecutive read polls before the read
  future yields with `wake_up_after`". It has to be at least 2 ms, because
  the executor only waits when the next wake is more than 1 ms away
  (`background_executor.c:75`).

## What must be measured before this leaves Draft

| question | why it matters | how |
| --- | --- | --- |
| executor stack high-water mark, per THREAD platform | one stack now holds read + lease + reconnect + callbacks (D2) | boot-time peak print, the issue 1146 method |
| keep-alive delay under a blocked receive | NuttX sets `Z_CONFIG_SOCKET_TIMEOUT = 5000`; a 5 s blocked receive delays the keep-alive against `rmw_zenohd`'s 10 s lease | quiet-link soak on NuttX against `rmw_zenohd`; if it is marginal, lower the timeout to Zephyr's 100 ms (issue 0129's reasoning) |
| `Z_RUNTIME_MAX_TASKS` floor | the default of 8 is an estimate | count live tasks at open, reconnect and close in a test image |
| ThreadX multi-tier contention | the try-lock replaces `threadx_read_mutex`; a spawned tier's interest reply must still arrive (issue #247) | the multi-tier ThreadX e2e that caught #247 |
| app-thread latency on spawn and cancel | spawning from an app thread waits for the current executor poll (D2, fact 1) | only advanced pub/sub and session open spawn from app threads; confirm nano-ros reaches neither in steady state |

### Results (2026-10-27)

All five are answered, and the RFC is Stable. The keep-alive row found
two defects, both fixed. Both were the same failure seen from two platforms:
the executor's timers never ran, so the lease and keep-alive never ran.

- On NuttX, the cause was a clock-ABI split (issue 2094).
- On a Zephyr image with the native IP stack, the socket receive never timed
  out (issue 2105).

- **Executor stack high-water mark.** `zpico_open` spawns a one-shot
  sampler future on the executor. It samples once a second from +4 s and
  prints `nros: zenoh executor stack peak <used> of <size> bytes` when the
  value has held for 10 samples (or after 45 samples). It is compiled only
  where the platform can answer.

  | platform (image) | peak | stack |
  | --- | --- | --- |
  | FreeRTOS mps2-an385, Rust and C talker | 984 B | 5120 B |
  | FreeRTOS mps2-an385, Rust and C listener | 728 B | 5120 B |
  | NuttX virt, Rust listener | 2296 B | 65256 B |
  | Zephyr mps2_an385, C talker (`CONFIG_INIT_STACKS` overlay) | 992 B (stable; one sample before issue 2105's fix) | 131072 B |
  | POSIX host | not measurable (8 MiB host thread) | — |

  So no THREAD platform comes near its stack. NuttX and Zephyr carry 28x
  and 130x their peak. Lowering those defaults is a sizing change of its
  own, not made here. The Zephyr figure was a single early sample until
  issue 2105 was fixed: the sampler is a timed future, and on that image
  timed futures never ran.
- **Keep-alive under a blocked receive: 25–30 s on NuttX, against a 60 s
  expiry.**
  - The first measurement FAILED. A quiet NuttX listener lost its session
    at the lease in 3 of 3 runs.
  - The cause was not the blocked receive (issue 2094, fixed). NuttX linked
    the alias TU's `uint64_t` clock against `unix.h`'s `struct timespec`,
    so every executor wake-up was stack garbage. The correct clock it then
    linked wrapped every 4.29 s.
  - Fixed, the residual delay is the one this row asked about. Timers are
    checked only when the 5 s blocking read returns, and a due timer runs
    after a ready read. That makes up to ~10 s of lateness: keep-alives
    every 25–30 s, a lease check ~9 s late.
  - FreeRTOS (100 ms), ThreadX-Linux and Zephyr native_sim hold a quiet
    session too. `test_rtos_quiet_link_keeps_its_session` is the
    regression cell.
  - The 100 ms run that "made it worse" was on the broken clock, so it is
    no evidence either way. Whether NuttX can now take 100 ms is
    unmeasured.
  - The row's "10 s lease" is stale: the negotiated lease is 60 s since
    issue 0906.
- **`Z_RUNTIME_MAX_TASKS` floor: 5.** Measured on the POSIX integration
  suite (`nros-rmw-zenoh --features platform-posix --test
  zenoh_integration`): at 4, 21 of 22 pass, and
  `declares_keep_running_across_a_reconnect` fails because the reconnect
  cannot spawn its transport futures. At 5 and at 8, 22 of 22 pass. The
  default stays 8. That leaves headroom of 3, and the stack sampler holds
  one slot for its first minute.
- **ThreadX multi-tier contention: passes.** The `realtime_tiers` row
  `threadx-linux/rust` (the cell that caught #247) passed in 3 of 3 runs,
  on fixtures rebuilt from this tree.
- **App-thread spawn latency: not reached in steady state.** The futures
  that spawn from an app thread are advanced pub/sub
  (`Z_FEATURE_ADVANCED_* 0`) and peer mode (`Z_FEATURE_UNICAST_PEER 0`),
  and both are compiled out. That leaves session open, plus the stack
  sampler's one spawn right after it. A reconnect spawns from the executor
  thread itself.

## Alternatives considered

- **ThreadX runs the background executor thread like the others.** Rejected
  for the Linux host port: the executor thread would block in a host `recv`
  that the ThreadX scheduler cannot see, which is the phase-297 W5 starvation
  with one fewer thread to blame. It stays a candidate for ThreadX on NetX
  Duo hardware, where the receive is a ThreadX wait. D1 can split that row
  if a measurement favours it.
- **ThreadX goes single-thread** (`Z_FEATURE_MULTI_THREAD = 0`). Rejected:
  tier threads call `z_put` concurrently, and single-thread builds compile
  out the transport locks.
- **Keep `zp_read` in CALLER builds and add the executor beside it.**
  Rejected: the polled read resets the buffer the read future compacts
  (`read.c:133` against `read.c:82-115`), so mixing the two loses frames,
  the class of issue #247.
- **Keep the lease knobs and ignore them.** Rejected: an accepted knob with
  no effect is a silent wrong answer (D2).

## Phased plan

Tracked by phase-508:

1. The D4 fork patch, in the 1.10.1 patch-line rebase.
2. The D6 knobs.
3. D1 and D2: THREAD mode on POSIX first, measured, then Zephyr, NuttX and
   FreeRTOS.
4. D3: CALLER single-thread on bare-metal smoltcp and serial.
5. D4 and D5: ThreadX and the ping.
6. Retire and refuse the lease knobs (D2). Done 2026-10-26: the registry row
   carries `retired`, so every env and `system.toml` spelling is refused with
   the reason, the Kconfig symbol is gone, `[node.rt]` lease keys panic at
   parse, `nros_app_config_t` lost its `keepalive_*` fields,
   `zpico_set_task_config` takes two arguments, and the transport band is
   `{executor, poll}` (Zephyr's band symbols are the read priority alone).
7. Take each measurement in the table above; this RFC goes to Stable when
   all of them are taken.

## Changelog

- 2026-10-24: first draft, from issue 0910's second-pass survey.
- 2026-10-26: step 6 done — the lease knobs retired and refused.
- 2026-10-27: step 7 measurements taken (see "Results"). The
  `MAX_TASKS` floor is 5, ThreadX tiers pass, and app-thread spawns are
  not reached in steady state. The stack peaks are under 2.3 KiB
  everywhere. The keep-alive soak FAILS on NuttX (issue 2094), so the RFC
  stays Draft. D6's "a failed spawn is already loud" is corrected: it holds
  at open only.
- 2026-10-27: Stable. The cause of issue 2105 was that Zephyr honours
  `SO_RCVTIMEO` only under `CONFIG_NET_CONTEXT_RCVTIMEO`, so on a native-IP
  image the executor's receive blocked until the router spoke, and the
  lease and keep-alive ran only then. `NROS_RMW_ZENOH` now selects the
  option, and `zpico.c` refuses a THREAD build that lacks it. D2 gains the
  rule: a THREAD platform's receive must time out.
- 2026-10-27: the keep-alive row answered. Issue 2094 was a clock ABI split
  on NuttX, now fixed: a quiet link holds one session, with keep-alives
  every 25–30 s. Draft remains for issue 2105, a timed future that never
  re-woke on a Zephyr Cortex-M image.
- 2026-10-26: D4 corrected — ThreadX's sockets come from the platform net ABI
  (`nros-platform-threadx/src/net.c`), not the dead `c/platform/threadx/network.c`.
