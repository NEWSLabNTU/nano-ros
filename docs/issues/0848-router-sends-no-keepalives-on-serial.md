---
id: 848
title: "The router's serial keepalive is a 1-byte write that never lands as a
  parseable frame — board never resets its lease and expires at 2 x lease"
status: open
type: bug
area: rmw
related: [issue-0839, issue-0821]
---

## ROOT CAUSE (2026-08-28) — both sides on one timeline

Built `zenohd` v1.9.0 from source with `--features zenoh/transport_serial`
(serial is feature-gated; the first build refused with "Unicast not supported
for serial protocol"), instrumented the keepalive arm, the `TimeoutTracker`,
and the serial link's `write()`. Ran it against the board with the board's own
receive probes in the same run.

```
ROUTER writes -- all 12 returned OK, keepalive arm fired 4x
  18:07:48.050   74 bytes  [e1 09 f0 c7 1f ...]   handshake
  18:07:48.080    6 bytes  [62 3c b4 ea d3 03]
  18:07:48.269   10 bytes  [25 b4 ea d3 03 ...]
  18:07:48.300   10 bytes  [25 b5 ea d3 03 ...]
  18:07:58.307    1 byte   [04]      <- KEEPALIVE
  18:08:08.309    1 byte   [04]      <- KEEPALIVE

BOARD receives
  frame rb=9  ok hdr=0x03      the early, larger writes
  frame rb=83 ok hdr=0x00      arrive and parse correctly
  frame rb=15 ok hdr=0x00
  timeout, rb=4                <- 4 bytes, no terminator, dropped
  timeout, rb=0  x many
  expired x1
```

**The keepalive is a one-byte write and it never lands as a parseable frame.**
zenoh-pico's serial reader scans byte-by-byte for the `0x00` COBS terminator
(`_z_read_serial_internal`); it saw 4 bytes that never terminated and timed out
after 1000 ms. Every larger write in the same session framed and parsed fine.

So the full chain is: timer fires -> arm fires -> `write()` returns OK -> the
small keepalive does not arrive as a complete frame -> the board never resets
`_received` -> lease expires at `2 x Z_TRANSPORT_LEASE` -> close, reconnect,
repeat.

### What this retires

Every earlier hypothesis in this issue is now superseded, and each was wrong for
a different reason worth keeping:

- *"the router never sends keepalives"* — it does; the arm fires 4x. The
  evidence I had (absence of `universal::tx: Scheduled` lines) could never have
  shown it, because the arm bypasses the pipeline.
- *"the keepalive arm never fires"* — instrumented: `arm_enabled=true`,
  `keep_alive=10s`, fired.
- *"the board's read path is broken"* — it decodes every frame it is handed,
  including post-handshake data frames.
- *"the tx task is blocked"* — profiled: `tx-0` parked in `ep_poll`, nothing
  held.

### Not yet established

WHY a 1-byte payload fails to produce a complete frame while 6-, 10- and
74-byte payloads succeed. Candidates: framing/flush behaviour in `ZSerial`
for very small buffers, or a terminator that is written but not flushed. That
is one more probe inside `ZSerial::write` and is the obvious next step.

## Earlier diagnosis (superseded, kept for the record)

The original claim (router goes silent) was right; the *evidence* for it was
not, and it was retracted below for that reason. It has now been re-established
from the board side, which does not depend on router logging at all.

`_z_read_serial_internal` on Zephyr polls per byte with a **1000 ms** deadline
(`_Z_ZEPHYR_SERIAL_TIMEOUT_MS`), so consecutive calls give ~100% listening duty.
Instrumented to report bytes-seen on every path:

```
DIAG-RX: got frame rb=9  -> deserialize=ok hdr=0x03    <- INIT|ACK
DIAG-RX: got frame rb=83 -> deserialize=ok hdr=0x00    <- data
DIAG-RX: got frame rb=15 -> deserialize=ok hdr=0x00    <- data
DIAG-RX: timeout after 0 byte(s)   x19                 <- genuine silence
DIAG-RX: timeout after 4 byte(s)   x1                  <- truncated frame
```

**Three frames arrive around the handshake, then nineteen one-second windows of
absolute silence.** The board is listening the whole time and there is nothing
on the wire. The receive path is healthy — it decodes every frame it is given,
including two post-handshake data frames.

That also explains the earlier "board decodes only init+open" reading: zenoh's
`*_decode` DEBUG lines cover the handshake messages specifically, so the two
data frames were received and processed without producing one.

### Second, smaller finding: a truncated frame

One timeout fired **after 4 bytes had already arrived** — a frame started and
never completed within 1000 ms. Whether that is a dropped tail on the wire or a
sender that stalled mid-frame is not established, but it is a distinct failure
from the 19 zero-byte windows and should not be folded into them.

### Router-side: source read, logs raised, threads profiled (2026-08-28)

**Source** (`eclipse-zenoh/zenoh` at tag 1.9.0, `io/zenoh-links/zenoh-link-serial/src/unicast.rs`):

- `write()` and `read()` take **separate** `write_lock` / `read_lock`, so there
  is no read-vs-write deadlock at that layer.
- Both obtain `&mut ZSerial` from a shared `UnsafeCell<Option<ZSerial>>` via
  `get_port_mut(&self)`. The safety note claims "no concurrent reads or
  concurrent writes will ever happen", which is true of each *pair* — but it
  does **not** exclude a read concurrent with a write, and both hand out `&mut`
  to the same port. Noted as a latent hazard; not shown to fire here.
- `write()` logs `tracing::error!` on failure under the **`zenoh_link_serial`**
  target. Every earlier run used `zenoh_transport=debug` only, so a write error
  would have been invisible.

**Logs raised** to `zenoh_link_serial=trace`: **zero errors**, no "Unable to
write". Writes are not failing. (That module logs almost nothing on the success
path, so this bounds the failure rather than locating it.)

**Threads profiled** via `/proc/<pid>/task/*/{comm,wchan}` — sampled three
times across the silent window (ptrace is restricted here, so no backtraces):

```
t+6s / t+11s / t+16s
  tx-0    ep_poll          <- parked in the reactor, all three samples
  tx-1    futex_wait_queue <- idle worker
  rx-0/1  ep_poll / futex
  acc-0   ep_poll
```

**No thread is blocked on a serial write or on a mutex.** The tx worker is
simply parked with nothing to do. That is what an unfired keepalive timer looks
like, and it rules out the stalled-write and lock-contention explanations.

### What remains unknown

Why the router stops. The board-side measurement proves nothing arrives; it
cannot say whether the router's tx task stopped, its keepalive arm never fires,
or bytes are lost between the two. That still wants a working wire tap, and the
socat approach failed twice here.

## Earlier correction (kept — the reasoning still applies to router-log evidence)

This issue was filed claiming **"router keepalives sent: 0"**, counted by
grepping the router log for `KeepAlive` and finding only `rx:` lines. **That
count was meaningless.** `zenoh_transport::unicast::universal::tx: Scheduled`
logs a *pipeline push*; the keepalive arm calls `link.send()` **directly**,
bypassing the pipeline, and emits no log line at all. Absence of those lines
says nothing about whether keepalives were sent.

Re-running with the keepalive interval driven down to 2 s (`lease: 20000 /
keep_alive: 10`) over a 20 s session changed nothing in the log, for the same
reason — the experiment could not discriminate either.

**Whether the router transmits keepalives on serial is therefore still
UNMEASURED.** It needs the wire; the socat tap was attempted twice and captured
zero bytes both times.

What the title now claims is board-side and measured. The original framing is
kept below so the mistake is legible rather than erased.

## What is actually measured, board-side

With the router keepaliving every 2 s, over the whole session the board decoded:

```
1 x _z_init_decode      <- handshake
1 x _z_open_decode      <- handshake
(nothing else, ever)
```

- keepalive decodes: **0** — the 5 `KEEP_ALIVE` matches in its log are all
  `_z_keep_alive_encode`, its own sends
- Declare decodes: **0** — it never decoded the two Declares the router
  demonstrably queued at t+0.2 s

So the board's receive path stops the moment `_z_unicast_handshake_open`
returns. Note the two decodes it *does* perform happen on the OPENING thread
inside the handshake, not in `_zp_unicast_read_task` — so this is consistent
with the read task never processing a single frame.

`_zp_unicast_start_read_task` does `_Z_ERROR_RETURN(_Z_ERR_SYSTEM_TASK_FAILED)`
if `_z_task_init` fails, and no such error appears, so the task was created.
What it does after creation is the open question.

## And the board stack-overflows at expiry

```
[20.095] _zp_unicast_lease_task: Closing session because it has expired
[20.290] _z_close_encode: Encoding _Z_MID_T_CLOSE
[20.291] ***** USAGE FAULT *****  Illegal load of EXC_RETURN into PC
         pc 0x004236dd  _z_network_message_elem_copy
         s[3] _z_wireexpr_copy   s[7] _z_slist_new
[20.293] ZEPHYR FATAL ERROR 34   Current thread: idle
```

"Illegal load of EXC_RETURN into PC" is a corrupted return address — a stack
overflow, in the message-copy path during teardown.

**CONFOUND, stated plainly:** that image was built with
`CONFIG_NROS_ZEPHYR_TASK_STACK_SIZE=6144`, which is BELOW the 8192 default and
was chosen by hand to fit RAM while raising `TASK_SLOTS` to 8. So this overflow
may well be self-inflicted, and the earlier "action discovery works at slots=8"
result was measured on those same undersized stacks. A retest at 8192 was
started and its harness failed (one line of RTT captured); it is **not**
evidence either way and no conclusion is drawn from it.

## Confound eliminated (2026-08-28): 8192 changes nothing

Rebuilt RAM-neutral at the DEFAULT task stack size — `TASK_SLOTS=6`,
`TASK_STACK_SIZE=8192`, `MAIN_STACK_SIZE=16384` — and re-measured. Both
symptoms persist unchanged:

| | at 6144 | at 8192 |
| --- | --- | --- |
| board decodes, whole session | init + open only | **init + open only** |
| keepalives decoded | 0 (5 sent) | **0 (5 sent)** |
| expiry | 20.09 s | **20.07 s** |
| fault | `FATAL 34` @ 20.29 s | **`FATAL 34` @ 20.28 s** |
| faulting pc | `0x004236dd` | **`0x004236dd`** |

Same fault, same address, same timing at both stack sizes. **The undersized
stack was not the cause of either symptom**, and the earlier
"discovery works at slots=8" result is not invalidated by it either.

The fault address being IDENTICAL across builds is itself informative: this is
a deterministic path, not random stack corruption. `0x004236dd` is
`_z_network_message_elem_copy`
(`include/zenoh-pico/protocol/definitions/network.h:356`), reached via
`_z_wireexpr_copy` and `_z_slist_new`, and it fires immediately after
`_z_close_encode` on the expiry path:

```
[20.075] _zp_unicast_lease_task: Closing session because it has expired
[20.282] _z_transport_tx_send_t_msg: Send session message
[20.282] _z_close_encode: Encoding _Z_MID_T_CLOSE
[20.283] ***** USAGE FAULT *****  Illegal load of EXC_RETURN into PC
```

So there are two independent defects on this image, neither explained by
configuration:

1. **The read path never delivers a frame** after the handshake — no
   keepalives, not even the Declares the router queued.
2. **The expiry teardown faults deterministically** at
   `_z_network_message_elem_copy`.

(1) is what causes the expiry; (2) is what happens when the expiry runs. Fixing
(1) would avoid (2) in practice but leave it latent.

## Original framing (superseded, kept for the record)

## Problem

Against the router config this repo ships
(`experiments/serial-interop/router-serial.json5`, `lease: 60000` /
`keep_alive: 6`, i.e. one keepalive every 10 s), the router transmits **twice**
to the board and then goes silent for the rest of the session. Captured at
`RUST_LOG=zenoh_transport=trace`, action-server image on
mr_canhubk3/s32k344 over serial:

```
15:51:12.408  New transport opened                          <- serial link up
15:51:12.585  tx: Scheduled NetworkMessageRef { Declare .. } <- 1st transmit
15:51:12.617  tx: Scheduled NetworkMessageRef { Declare .. } <- 2nd, and last
15:51:19.095  rx: TransportMessage { KeepAlive }             <- the BOARD's
15:51:22.439  rx: TransportMessage { KeepAlive }
15:51:25.766  rx: TransportMessage { KeepAlive }
15:51:29.110  rx: TransportMessage { KeepAlive }
15:51:32.437  rx: TransportMessage { KeepAlive }
15:51:32.629  rx: TransportMessage { Close }                 <- board gives up
```

Counted over the whole 20.2 s session:

| | count |
| --- | --- |
| router transmits (`universal::tx`) | **2**, both in the first 210 ms |
| router keepalives **sent** | **0** |
| board keepalives **received** by the router | 5, every 3.33 s, on schedule |

The board is behaving correctly. It keepalives on cadence, hears nothing for
two lease periods, and closes with `_Z_CLOSE_EXPIRED`. There is nothing
arriving for it to be starved of.

## This corrects an earlier claim

[Issue 0839](0839-action-image-session-expires-every-20s.md) records the router
keepalive cadence as "verified on the wire at 10.0 s" and therefore ruled out.
**That measurement was the TALKER image**, not this one, and it does not
generalise. For the action image the router emits no keepalives at all. 0839
has been corrected.

## Not a config difference

Same router, same config file, same board, same transport, same baud. A
**talker** holds its session through a five-minute soak with `closed=0` and
`ros2 topic hz` steady at 1.99 Hz. Only the guest image differs.

## Where the mechanism lives

zenoh's keepalive is emitted by the transport's TX task when the link has been
idle for the keepalive period
(`io/zenoh-transport/src/unicast/universal/link.rs`, read while chasing
[issue 0821](archived/0821-zenoh-pico-faults-at-lease-expiry-on-zephyr.md)):

```rust
_ = keep_alive_tracker.wait_if(write_priority.unwrap_or(Priority::Control) == Priority::Control) => {
    let message: TransportMessage = KeepAlive.into();
    link.send(&message, Some(Priority::Control)).await?;
}
```

`keep_alive_tracker.reset()` is called on every batch actually written, and the
tracker's timeout is driven by a background task spawned in
`TimeoutTracker::new`. Serial does not override `supports_priorities()`, so it
takes the single-`write_loop(None, ..)` path where the keepalive arm IS
enabled — which is why this is a defect rather than a documented limitation.

**Not established:** why the arm never fires here when it fires for the talker
on the identical link. The two Declares at t+0.2 s are the last thing the tx
task ever schedules.

## Suggested next step

Raise `RUST_LOG` on `zenoh_transport::unicast::universal::link` specifically
and compare a talker session against an action session on the same router
process — the question is narrow: does `keep_alive_tracker` fire and the send
fail, or does it never fire at all. That distinguishes a stalled tx task from a
tracker that is being reset by something.

Worth checking too whether the queryables the action image declares (three,
versus the talker's zero) leave a pipeline in a state the tx task treats as
non-idle.

## Impact

Blocks [issue 0839](0839-action-image-session-expires-every-20s.md), and with
it any action over serial: the session cannot outlive `2 x
Z_TRANSPORT_LEASE`. `CONFIG_NROS_ZENOH_LEASE_MS` mitigates by moving the
deadline, but the link is still one-way after the handshake.
