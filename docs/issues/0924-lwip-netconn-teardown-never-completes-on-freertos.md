---
id: 924
title: "A FreeRTOS lease teardown parks forever in lwIP's netconn shutdown/close"
status: open
type: bug
area: boards, drivers, rmw-zenoh
related: [issue-0906, issue-0899, issue-0836]
---

## What happens

When zenoh-pico tears a unicast transport down, `_z_common_transport_clear` →
`_z_link_free` → `_z_close_tcp` calls `shutdown(fd, SHUT_RDWR)` and then
`close(fd)`. On mps2-an385 + FreeRTOS + lwIP the lease task enters that and never
comes out.

Measured with a `volatile` stage counter through the teardown, read with gdb
attached AFTER the guest went quiet (a plain `-s` gdbstub, so the guest ran at
full speed until the sample):

    clr_stage=50      /* inside _z_close_tcp, in shutdown(fd, SHUT_RDWR) */

sampled twice, 25 s apart, unchanged. Deleting the `shutdown` call moves the
stall into `close()` — 37-58 failed publishes per run either way — so it is the
netconn teardown itself, not that one call.

The consequence, while it lasts, is total: `_z_session_is_closed()` is literally
`_tp._type == _Z_TRANSPORT_NONE`, and `_type` is restored only when the reopen
lands. It never lands, so every subsequent publish returns
`NROS_RET_PUBLISH_FAILED` forever.

## Why this is filed separately from [[issue-0906]], and why it is not urgent

0906 was the trigger: the client's lease was shorter than the router's
keep-alive cadence, so a healthy session tore itself down every ~20 s and hit
this stall every time. That is fixed — the lease now matches what ROS announces,
and a 330 s run makes exactly two TCP handshakes, one per node.

So this path is no longer reached in the steady state. It IS still reached
whenever a peer genuinely goes away: a router restart, a cable pull, a crashed
node. Then the image stops publishing permanently and reports nothing but
`-10`s. A reconnect that cannot happen is a worse failure than the churn that
used to hide it.

## What was already ruled out

Two lwIP threading defects were found while chasing this, and both are FIXED
without curing it (see 0906 for the measurements):

* `LWIP_NETCONN_SEM_PER_THREAD` was 1 with `LWIP_NETCONN_FULLDUPLEX` at its
  default 0. lwIP requires both to use one netconn from several threads, which
  is our shape exactly — read task recv, app task send, lease task close.
* The per-thread semaphore was never allocated for the read and lease tasks;
  `sys_arch_netconn_sem_get()` returned NULL for both. `z_task_wrapper` in the
  zenoh-pico fork now calls `lwip_socket_thread_init()`.

The stall survives both.

## Where to look next

* Whether the joined read task was `vTaskDelete`d while it still owned netconn
  state. `_zp_unicast_failed` joins it first, and `_z_task_join` deletes the task
  from ANOTHER task; if it was inside lwIP at the time, the netconn is wedged.
* Whether the tcpip thread is servicing the API mailbox at that moment.
  `TCPIP_THREAD_PRIO` is 4, the same band as the poll task.
* Whether the netconn / `MEMP_NUM_TCPIP_MSG_API` pools are exhausted. Issue 0836
  was the INPKT half of exactly that question on a sibling board. **`LWIP_STATS`
  is 0 on this board — turn it on first**; that is what made 0836 answerable.

## Method notes

* A peer must be attached, or the teardown path is not exercised the same way.
* **Diagnostic `printf`s hide this.** Probe-laden builds complete the teardown
  where the same code without prints stalls. Use `volatile` stage counters and
  read them from gdb after the hang; never put a print in the path under test.
* Both halves of a talker/listener pair must be rebuilt, or a stale peer gives
  numbers that look like a fix.

## Acceptance

* Kill the router mid-run: the image reconnects when it comes back, rather than
  publishing `-10` forever.
* The teardown completes with no diagnostic output compiled in — proven by a
  stage counter read after the fact, not by a build whose prints widen the
  window.
