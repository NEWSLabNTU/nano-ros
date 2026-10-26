---
id: 2094
title: "A quiet NuttX zenoh link loses its session after 60 s: the executor thread stops waking, so no keep-alive goes out"
status: open
type: bug
area: [nuttx, zenoh]
severity: high
found: 2026-10-26
related: [rfc-0110, phase-508, 0906]
---

## Measured

A NuttX Rust listener (`nuttx::build_nuttx_listener`, QEMU `virt`, SLIRP)
connected ALONE to `rmw_zenohd`, with no talker, for 150 s. The router ran
with `zenoh_transport=trace`. This was nano-ros on branch
`feat/508-lease-knobs`, on top of PR #2697 (zenoh-pico 1.10, RFC-0110 D2:
the lease and keep-alive run as futures on the executor thread, and no lease
task exists any more).

Router log, relative to the first open:

| t (s) | router saw |
| --- | --- |
| 0 | `New transport opened` |
| +35 | client `KeepAlive` |
| +50.7 | client `KeepAlive` |
| +110.7 | `RX task failed ... expired after 60000 milliseconds` |
| +110.7 (8 ms later) | `New transport opened`, same zid |

The client negotiates a 60 s lease (`Z_TRANSPORT_LEASE` = 60000, issue
0906), with `Z_TRANSPORT_LEASE_EXPIRE_FACTOR` = 3. So it owes a keep-alive
every 20 s, and three in a row went missing. A second run without trace
logging measured the same shape: an expiry at about 60 s, then four more
reopens within 30 s. A third run, on a clean rebuild, sent NO keep-alive
before the router expired the session at +60.5 s, then reopened four times
in 83 s. So it reproduced in 3 of 3 runs.

**The control is healthy.** A FreeRTOS listener (mps2-an385, lwIP, with a
100 ms socket timeout) ran the same soak and kept ONE session for 150 s.
The pub/sub cells pass on NuttX because traffic sets `_transmitted`, so an
idle keep-alive is never needed there.

## What the evidence says

- **The executor thread is not waking every `Z_CONFIG_SOCKET_TIMEOUT`
  (5000 ms on NuttX).** The RFC-0110 stack sampler is a future on that
  thread. It needs about 11 wakes, spaced at least 1 s apart, to print
  `nros: zenoh executor stack peak`. With a 5 s timeout it would print
  within about 60 s. It printed nothing in 150 s.
- **The client reconnects 8 ms after the router closes,** so the read
  future does return when bytes or EOF arrive. A recv that blocks until
  data arrives, rather than timing out, fits every observation.
- **NuttX's own `SO_RCVTIMEO` path reads correctly.** `setsockopt` stores
  `net_timeval2dsec(tv, TV2DS_CEIL)`, which is 50 dsec, and `tcp_recvfrom`
  waits `_SO_TIMEOUT` = 5000 ms, then maps `-ETIMEDOUT` to `-EAGAIN`.
  zenoh-pico treats `EAGAIN` as "nothing to read". So the defect is not
  visible from the source.
- **The background executor holds its mutex across the blocking recv,** and
  parks on a `pthread_cond_wait` whenever an app thread is mid-spawn
  (`_waiters > 0`). CLAUDE.md already records that a NuttX pthread condvar
  hangs (which is why the NuttX spin uses `sem_timedwait`). That is a
  candidate, not a finding: on a quiet listener nothing spawns after open,
  except the one-shot stack reporter.

## Other platforms, same soak

- **Zephyr native_sim** (`build-c-listener-zenoh`, alone, 150 s): ONE
  session. The client's keep-alives reached the router every ~21 s. So a
  sleeping future wakes normally under the same executor.
- **Zephyr mps2_an385 (Cortex-M, a talker, so traffic flows):** a SECOND
  anomaly that may share a cause. The RFC-0110 stack sampler spawned,
  took one sample at +4 s (992 of 131072 bytes), returned
  `wake_up_after(1000)`, and never ran again in 86 s. The image used
  temporary `printk` instrumentation, since reverted. The talker's traffic
  sets `_transmitted`, so a keep-alive future that never woke would be
  masked here. Whether the lease and keep-alive futures stall the same way
  on this image is not measured.

## Shortening the timeout makes it worse, and why NuttX keeps 5000

With `NUTTX_SOCKET_TIMEOUT_MS` = 100 (a temporary experiment, reverted) the
same soak opened **99** sessions in 150 s: a reconnect storm. So
`platform-implementation-notes.md`'s "NuttX intentionally keeps 5000" stands.
Its reason was never written down, and this is it.

## Attribution: likely RFC-0110 D2, not measured on 1.8

Under zenoh-pico 1.8 the lease and keep-alive ran on their own lease task,
which a blocked read thread could not starve. D2 moved them onto the
executor thread, which is exactly the thread that is not waking here. The
same soak has NOT been run against a pre-#2697 NuttX image, so "regression"
is inferred.

## Next

1. Run the soak on a pre-#2697 build, to settle the attribution.
2. Instrument the executor loop on NuttX (and on the Cortex-M Zephyr
   image above): count the spins, the recv
   returns and the condvar waits. This decides between the recv not timing
   out and the executor parking on a condvar.
3. Whichever it is, the fix must keep 5000 ms (or explain the storm) and
   must hold on the quiet soak. That soak belongs in the matrix as a cell,
   since no current cell is quiet for longer than the lease.
