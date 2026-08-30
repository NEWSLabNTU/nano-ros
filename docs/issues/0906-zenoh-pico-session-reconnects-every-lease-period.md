---
id: 906
title: "Every zenoh-pico session drops and rebuilds every ~20 s — the ROS router sends it no KeepAlive"
status: open
type: bug
area: rmw-zenoh, interop
related: [issue-0899, rfc-0075]
---

## What happens

A nano-ros zenoh-pico client connected to `rmw_zenohd` closes its session and
reconnects roughly every twenty seconds, forever, with no error reported to the
application. It is not load-dependent, not platform-dependent, and not
provoked by anything the app does.

`tshark` on the loopback side of the link, one 45 s run of the FreeRTOS
`c_talker` under QEMU (slirp forwards to the host router), payload frames only:

    router -> guest, tcp.len>0:   0.004 s  76 B
                                  0.006 s   8 B
                                  0.011 s  12 B
                                 19.522 s  76 B   <- a NEW handshake
                                 19.522 s   8 B
                                 19.523 s  12 B
                                 40.086 s  76 B   <- and another
    guest -> router, tcp.len>0:  40 frames (the 1 Hz publishes)

Three frames in the first twelve milliseconds, then silence until the session
has already been declared dead and rebuilt.

## Why

`_zp_unicast_lease_task` (`src/transport/unicast/lease.c`) closes a CLIENT
session when `curr_peer->common._received` was false for one whole lease
period. Probed on the expiry branch, that is exactly what fires:

    !!! LEASE-EXPIRED nothing received in 10000ms

`_received` is set by the read task on each RX batch. A probe there shows it
running **once** per session — at open — and then blocking for the full ten
seconds, which the capture above confirms is honest: there is nothing to read.

The talker never sends a keepalive either, and cannot: the lease task only
sends one `if (!ztu->_common._transmitted)`, and a 1 Hz publisher has
transmitted every time the check comes round.

## It is NOT a FreeRTOS or an embedded problem

The same capture against the NATIVE Linux `examples/native/c/talker`, same
router, same port:

    0.000 s 76 B / 0.000 s 8 B / 0.000 s 12 B / 20.075 s 76 B ...

Identical. Every zenoh-pico client we ship behaves this way against the ROS
router. Native survives it silently — the reconnect succeeds and publishing
continues — which is why it has gone unnoticed.

## Why it matters even though "it reconnects"

* Each cycle drops and re-declares every publisher, subscriber and queryable.
  Anything a peer had matched is unmatched and rematched on a 20 s beat.
* It is the trigger for [[issue-0899]]: on FreeRTOS the teardown races the
  publishing task and the image asserts. 0899's own defect is the unsynchronised
  teardown, but this issue is what fires it, over and over.
* A session that dies on a timer is a strong candidate for interop flakiness
  reported elsewhere and attributed to discovery or to QEMU load.

## A reconnect does not restore delivery

Found while verifying [[issue-0899]]'s fix, which stopped the FreeRTOS image
crashing at the first lapse and so made the next symptom visible for the first
time.

Talker and listener, both on mps2-an385, 80 s window, five runs: the talker
publishes 76–77 messages and the listener hears **19** of them (40 in one run) —
exactly the count up to the first lapse. Publishing continues, reconnects
succeed, and the subscriber never receives another sample.

So the churn is not merely wasteful. Either the subscription is not re-declared
after `_z_reopen`, or it is re-declared and no longer matches. That is a
third question this issue has to answer, and it is the one that actually costs
messages.

## Where the messages actually go — measured, and it is NOT the write filter

The reconnect is not what costs the messages. The teardown that precedes it
never finishes.

**The publisher stops being called at all.** A probe inside
`z_publisher_put` counted 19 entries against 77 `Publishing:` lines, and the
example's own return check tells the rest: from message 20 onward every call
returns `-10` (`NROS_RET_PUBLISH_FAILED`), forever. So the samples are not
filtered, not queued and not dropped on the wire — the shim refuses them.

**Because the session reads as closed, permanently.**
`_z_session_is_closed()` is literally

    session->_tp._type == _Z_TRANSPORT_NONE

and `_type` is set to NONE at the start of a teardown (and by `_z_open` on
entry). It is restored only when the reopen lands. It never lands.

**The teardown parks in lwIP.** A stage counter through
`_zp_unicast_failed` → `_z_unicast_transport_clear` → `_z_common_transport_clear`,
read with gdb attached AFTER the guest went quiet (a plain `-s` stub, so the
guest ran at full speed until the sample), lands on the same step every time:

    clr_stage=50    /* inside _z_close_tcp, in shutdown(fd, SHUT_RDWR) */

sampled twice, 25 s apart, unchanged. Removing the `shutdown` call moves the
stall into `close()` — 37-58 failed publishes per run either way — so it is the
netconn teardown itself, not that one call.

**When the teardown does complete, delivery fully recovers.** Under a build
whose diagnostic prints happened to widen the window: 60 published, 60 heard.
So a reopen DOES restore the subscription and the matching; nothing is lost in
re-declaration. The entire message loss is the stalled teardown.

## The lwIP threading configuration was not a valid one

Two findings, both measured, both necessary and neither sufficient:

**1. `LWIP_NETCONN_SEM_PER_THREAD` without `LWIP_NETCONN_FULLDUPLEX`.** lwIP's
own header says what our usage requires:

> `LWIP_NETCONN_FULLDUPLEX==1`: Enable code that allows reading from one thread,
> writing from a 2nd thread and closing from a 3rd thread at the same time.
> `LWIP_NETCONN_SEM_PER_THREAD==1` is required to use one socket/netconn from
> multiple threads at once!

That is exactly the shape here — zenoh-pico's read task calls `recv`, the app
task publishes, the lease task sends keepalives and closes. The board set
`SEM_PER_THREAD` alone and left `FULLDUPLEX` at its default of 0.

**2. The per-thread semaphore was never allocated for two of the three tasks.**
The FreeRTOS port's `sys_arch_netconn_sem_get()` only READS the thread-local
slot; only `lwip_socket_thread_init()` allocates. Probed:

    !!! NETCONN-SEM-NULL task=zpico_read
    !!! NETCONN-SEM-NULL task=zpico_lease

Both are now fixed — `FULLDUPLEX` is on, and `z_task_wrapper` in the zenoh-pico
fork calls `lwip_socket_thread_init()`/`_cleanup()` — and the stall SURVIVES
both. They were undefined behaviour that had to go before anything downstream
could be reasoned about, not the answer.

## What is landed

All of it. The zenoh-pico patches are on the fork's `nano-ros` line, pushed as a
fast-forward (`e0832729..ce206ec0`), and the superproject pin moved with them:

* `567c0c52` — the [[issue-0899]] crash fix.
* `ce206ec0` — the per-task lwIP netconn semaphore, this issue's finding 2.
* the board's `LWIP_NETCONN_FULLDUPLEX` flip, this issue's finding 1, in the
  same superproject commit as the pin bump. It was deliberately held out of the
  earlier docs commit: shipping it without the fork side would have been the
  same half-a-requirement mistake this issue is about, in the other direction.

None of that CLOSES this issue. The lease teardown still parks in lwIP's netconn
shutdown/close, so a lapsed session still loses delivery. What landed removes
two pieces of undefined behaviour that stood between the symptom and any sound
reasoning about it.

## Still open, and this is the next thread to pull

Why lwIP's netconn teardown does not complete for a socket whose reader has
already been joined. Worth checking, in this order: whether the joined read task
was deleted while still owning netconn state (`_z_task_join` calls
`vTaskDelete` on it); whether the tcpip thread is servicing the API mailbox at
that moment; and whether `MEMP_NUM_TCPIP_MSG_API` / the netconn pools are
exhausted by then (issue 0836 was the INPKT half of exactly that question on a
sibling board). `LWIP_STATS` is 0 on this board — turn it on first.

## Method notes for whoever continues

* **A peer must be attached.** Without the listener none of this reproduces.
* **Diagnostic `printf`s hide it.** Probe-laden builds run clean where the same
  code without prints stalls. Use `volatile` stage counters and read them with
  gdb attached AFTER the hang, never a print in the path under test.
* Both halves of a talker/listener pair must be rebuilt; a stale peer produces
  numbers that look like a fix.

## What to establish first

Whose job the keepalive is, on this pairing:

* If the ROUTER is expected to send periodic KeepAlive to an idle client, then
  `rmw_zenohd` not doing so is the defect, and the pin/version matters
  (RFC-0075 — the router is whatever ROS ships, so this can move under us).
* If the CLIENT is expected to keep its own lease refreshed, then zenoh-pico's
  `if (!_transmitted)` suppression is wrong for a publisher: transmitting does
  not prove the peer is alive, which is the entire point of a lease.

Decide that before writing a fix; the two answers land in different repos.

## Acceptance

* A zenoh-pico client publishing at 1 Hz against `rmw_zenohd` holds ONE session
  for at least five lease periods, proven by a payload-only packet capture
  showing no second handshake — not by "messages still arrive".
* Whichever side owns the keepalive is named in the RMW notes, so the next
  person does not have to re-derive it from a capture.
