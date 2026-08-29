---
id: 836
title: "A FreeRTOS/lwIP image loses SEDP announcements at discovery and never
  matches the topics announced after the gap"
status: resolved
type: bug
area: platform
related: [phase-385, issue-0749, issue-0830, issue-0889]
---

## Symptom, as originally reported

An `mps3-an536-freertos` image running the Autoware Safety Island controller,
on a `tap` with a real ROS 2 / Autoware stack, appeared to receive **every
small topic and never the large one**: the controller logged `Waiting for
trajectory data` indefinitely while a host CycloneDDS subscriber on the same
domain and interface read that topic at a clean 10 Hz. Because the one topic
that never arrived was also the only one bigger than a datagram (~13 KiB
against the peer's 1400 B `MaxMessageSize`), this was filed as a payload-SIZE
problem: RTPS fragmentation, defragmentation buffers, reassembly sizing.

**That framing was wrong, and two things made it look right.**

## What it actually is

`MEMP_NUM_TCPIP_MSG_INPKT` was left at the lwIP default of **8** while this
board raised everything around it — `PBUF_POOL_SIZE=128`,
`DEFAULT_UDP_RECVMBOX_SIZE=64` and, crucially, `TCPIP_MBOX_SIZE=64`.

Those two are not independent. The tcpip mailbox holds *pointers* to
`tcpip_msg` structures allocated from `MEMP_TCPIP_MSG_INPKT`, so the pool, not
the mailbox, is the real driver→stack queue depth. Sizing the mailbox to 64 and
the pool to 8 does not give a queue of 64; it gives a queue of 8 that looks
like 64. Past that, `tcpip_input()` returns `ERR_MEM` and the frame is dropped
before it ever reaches IP.

A discovery burst from a real ROS 2 peer overruns 8 easily. Measured with
`LWIP_STATS` on the failing image, it is the **only** pool that ever fails an
allocation:

```
TCPIP_MSG_INPKT   used=0  max=8   avail=8    err=7     <-- exhausted
PBUF_POOL         used=0  max=9   avail=128  err=0
UDP_PCB           used=4  max=5   avail=8    err=0
```

`max=8` is the pool pinned at its ceiling; `err=7` is seven frames discarded.
With the pool at 64 the same burst peaks at **14** — above the old ceiling, so
the overrun was not marginal — and `err=0`.

The frames it drops carry SEDP. That turns a handful of lost packets into a
permanent failure, because a reliable builtin reader that loses a sample stops
delivering everything after it:

```
reorder_sample(0x2170a888 R, 6  @ ...) expecting 6:  return [6,7)
reorder_sample(0x2170a888 R, 10 @ ...) expecting 7:  adding to empty store
reorder_sample(0x2170a888 R, 11 @ ...) expecting 7:  max = [10,11)
reorder_sample(0x2170a888 R, 12 @ ...) expecting 7:  max = [10,12)
...
reorder_gap   (0x2170a888 R, [1,1)   ) expecting 7:  too old      <- 30 s later
```

SEDP samples 7-9 are gone; 10-12 sit in the reorder store behind them; the
reader is still "expecting 7" half a minute later and never recovers. Every
writer announced from sample 7 onward is never matched, so those topics deliver
nothing, forever. On the wire the island simply stops sending ACKNACKs from its
application readers while its timed-event SPDP keeps ticking — which reads as
"the receive path died", though lwIP is healthy throughout (ICMP replies,
`udp.recv` still climbing at the publisher's rate).

**The topics announced last are the ones lost.** That is the whole "size"
effect: the Autoware trajectory publisher happened to be announced after the
gap. Nothing about the 13 KiB payload was ever involved — with the pool fixed,
a 17.6 KiB / 13-fragment sample arrives as reliably as a 116-byte one.

## Why it read as a size problem

1. **The topic that loses is the topic announced last**, and in the Autoware
   graph that was the trajectory. Size and announcement order were confounded.

2. **The consumer's own logging hid the rest.** The ASI controller reported all
   five missing inputs through a single `log_info_throttle` call site, and that
   throttle keys on `(__FILE__, __LINE__)` — so the five shared one slot and
   only the first missing input in the if-chain was ever printed. An image
   receiving *nothing* printed `Waiting for acceleration data` alone. The
   report "every small topic arrives, the large one does not" was partly read
   off that output. Fixed on the ASI side; the counterpart worth stating here
   is that a consumer's readiness log is not evidence about the wire.

## Fix

`MEMP_NUM_TCPIP_MSG_INPKT=64` in the board's compile definitions, next to the
`TCPIP_MBOX_SIZE=64` it has to track.

## Verification

Reproducer: boot the island, drive it from one host publisher on its own
topics, ask the island whether it still reports them missing. No Autoware, no
bridge, no planner — 60 s per run. It fails on a **boot-time coin flip**, which
is why the original lane, with its single long runs, read as "sometimes".

| build | runs | island received its inputs |
| --- | --- | --- |
| before (`MEMP_NUM_TCPIP_MSG_INPKT`=8, the lwIP default) | 12 | 7 |
| after (=64) | 34 | 34 |

Same harness, same host publisher, same image otherwise.

## Ruled out along the way

Each of these was tested by measuring the pass rate over repeated runs, not by
argument, and none of them moved it:

* **`AllowMulticast` on FreeRTOS** (issue 0888) — 3/8, against 7/12 baseline.
* **Cyclone receive-buffer sizing**, 64 KiB/16 KiB → 1 MiB/128 KiB — 13/18.
* **SEDP datagram coalescing.** A failing run showed three DATA submessages
  packed into one datagram where a passing run showed two, which looked
  causal; staggering the peer's `create_publisher` calls so they never
  coalesce gave 6/10.
* **lwIP's per-thread select semaphore** (`LWIP_NETCONN_SEM_PER_THREAD` with
  one TLS slot) — Cyclone's `recv` thread does get one; read out of its TCB.

## What this does NOT fix

The trajectory now *arrives* at every size, but on the ASI lane it is often
stale by the consumer's freshness check — equally at 2 fragments and at 13, so
it is a rate problem, not a size one. That is issue 0889 (the Cyclone RMW
installed no wake callback, so the executor polled and mostly missed), fixed
separately.

## Why it generalises

Any nano-ros FreeRTOS/lwIP board that raises `TCPIP_MBOX_SIZE` without raising
`MEMP_NUM_TCPIP_MSG_INPKT` has this bug, and it stays invisible until a peer
sends a burst: the small-message examples in this repo announce a handful of
entities and never overrun 8. A real ROS 2 node announces dozens at once.
Boards that raise one should raise the other, and `LWIP_STATS` is what makes
the difference between guessing and reading `err=7` off the pool.
