---
id: 924
title: "A FreeRTOS lease teardown parks forever in lwIP's netconn shutdown/close"
status: resolved
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

## FIXED — a graceful close waiting on a peer that will never answer

`_z_close_tcp` called `shutdown(fd, SHUT_RDWR)` and then `close(fd)`. Every
caller reaches it because the transport is already being torn down, and the
commonest reason is that the peer stopped answering. A graceful close is exactly
wrong there: `shutdown()` makes lwIP send a FIN and wait for the ACK, and a
wedged, frozen or dead peer never sends one.

The fix is `SO_LINGER` with a zero timeout before `close()` — the portable
"abort now". The stack sends RST instead of FIN and `close()` returns
immediately, waiting on nothing. Zenoh has already sent its own Close message at
the protocol level by then, so the graceful half of the handshake has served its
purpose; the TCP-level courtesy buys nothing but a hang.

## Why it looked like [[issue-0906]] had fixed it

0906 raised the client lease from 10 s to 60 s and the stall appeared to go
away. It had not. At 60 s the outages under test merely ended shortly after the
lease expired, so the peer thawed and ACKed the FIN that was still pending.
Lengthening the outage past the lease brings the wedge straight back. **The
lease value decides how OFTEN this is reached, never whether it is possible** —
and the same reasoning applies to a router restart in the field, which is not
scheduled around anyone's lease.

## Measured

The discriminating experiment is SIGSTOP, not SIGKILL. A frozen router keeps the
socket healthy and simply stops answering, which is the case that needs a real
FIN handshake; a killed router resets the socket, so `close()` returns
immediately and a kill-only test reports success on broken code.

    before:  247 publishes, 227 failures — delivery frozen from the first
             outage and never recovering across repeated cycles
    after:   3 alternating freeze/kill cycles, 150 s outages, shipped 60 s
             lease: delivery recovers every time (heard 27 -> 62 -> 96 -> 130)

Steady state re-checked, unchanged: 330 s, 327 published, 325 heard, 0 publish
errors, exactly 2 TCP handshakes.

## A guard that is NOT the fix, kept and labelled

`_z_session_t` gains `_reconnecting`, guarded by `_mutex_transport`, so only one
task can own a failover: `_z_reopen` starts a new lease task before it returns,
and that task begins counting immediately, so a reopen outlasting one lease
period could bring a second teardown onto the same transport.

I wrote this first, believing it was the bug. It is not. Instrumentation counted
exactly ONE failover claim in a failing run, and the guard changed nothing. It
is kept because the re-entrancy is real — `_zp_unicast_failed` is reachable from
both the lease-expiry and keep-alive-failure arms — but it has never been
observed to fire and is not load-bearing here.

## What was ruled out on the way

Two lwIP threading defects, both fixed under 0906 and NEITHER curing this:
`LWIP_NETCONN_SEM_PER_THREAD` set without `LWIP_NETCONN_FULLDUPLEX`, and a
per-thread netconn semaphore never allocated for the read and lease tasks. Also
ruled out: the emulator. The stall reproduces identically on the patched
`build/qemu` QEMU and on the system one.

## Method notes

* A peer must be attached, or the teardown path is not exercised the same way.
* **Diagnostic `printf`s hide this.** Probe-laden builds complete the teardown
  where the same code without prints stalls. Use `volatile` stage counters and
  read them from gdb after the hang; never put a print in the path under test.
* Both halves of a talker/listener pair must be rebuilt, or a stale peer gives
  numbers that look like a fix.
* **Kill stray processes BY EXECUTABLE NAME (`pgrep -x`) before every run.** A
  leftover router keeps serving the port so the "outage" never happens; a
  leftover talker keeps feeding the listener so a dead run still scores
  messages. Both faked a PASS here. And never `pgrep -f` for the cleanup: a
  full-command-line match also matches the shell running the script, which then
  kills its own caller.
* **Freeze the peer (SIGSTOP), do not kill it.** SIGKILL resets the socket and
  `close()` returns immediately, so a kill-only test passes on broken code.
* Check that the harness's own time budget covers the longest cycle. A freeze
  longer than the budget kills the images mid-test and scores it as a product
  failure.

## Acceptance

* ~~Kill the router mid-run: the image reconnects when it comes back, rather
  than publishing `-10` forever.~~ Met, and strengthened: three alternating
  FREEZE/kill cycles with 150 s outages, delivery resuming after each.
* ~~The teardown completes with no diagnostic output compiled in~~ — met: every
  number above comes from a build with no probes compiled in.
