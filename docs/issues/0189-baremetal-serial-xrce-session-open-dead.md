---
id: 189
title: "qemu-arm-baremetal serial/XRCE lanes: session open dead AFTER the #184 heap fix — zenoh-serial hangs at Executor::open, XRCE fails ConnectionFailed fast"
status: open
type: bug
area: baremetal
related: [issue-0184, issue-0178, phase-282]
---

## Summary

With #184's heap fix in (the three images now boot past the 74888-byte
executor-backing allocation), the two lanes fail one layer deeper —
serialized, fresh images 2026-07-13:

- `emulator::test_qemu_serial_pubsub_e2e` (zenoh-pico serial, socat pty ↔
  zenohd serial plugin): BOTH talker and listener print through
  `Serial ready.` then hang silently — no `Executor::open failed`, no
  publish, no panic; 97 s to test timeout. The session open never completes
  and never errors.
- `emulator::test_qemu_xrce_pubsub_e2e` (MicroXRCEAgent on a socat pty):
  boots, then `Executor::open failed: Transport(ConnectionFailed)` within
  ~2 s of boot — the uxr session create against the agent fails fast.

## Suspects (untriaged)

1. **#178 layers 2–3 family** — the zenoh-pico connect busy-wait needs a
   `wfi`-yield for QEMU (`-icount shift=auto`) to let host-timed I/O
   deliver; #178 proved the ethernet direct-exec path yields "some other
   way" while RTIC doesn't. The SERIAL direct-exec poll loop may lack
   whatever yield the ethernet path has.
2. **phase-282 tx rework** (zenoh-pico fork: batching + flush thread +
   split tx locking, `798328d78`/`25c3a6d3c`) — if the serial link's
   handshake writes now sit in a batch that only a flush *task* drains,
   a single-threaded bare-metal image may never emit InitSyn. The
   ethernet baremetal lanes' current state should discriminate (same
   threading model, different link).
3. The XRCE fast-fail is likely a different mechanism than the zenoh hang
   (it errors instead of hanging) — possibly agent-side pacing vs the
   1 s startup delay, or the serial framing on the pty.

## History caveat

These lanes were part of the museum-binary population (#182 class): the
last *proven* pass predates the phase-271 executor rework (their 24 KB
heap could not even boot a post-271 image, and the published phase-204/207
footprint figures were measured on pre-271 images). Treat "green history"
as unproven, per the #178 note.
