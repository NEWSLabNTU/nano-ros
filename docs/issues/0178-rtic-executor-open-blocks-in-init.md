---
id: 178
title: "RTIC mps2-an385 images never deliver — Executor::open (blocking zenoh connect) runs in #[init] with interrupts masked"
status: open
type: bug
area: baremetal
related: [issue-0176, phase-216, phase-271]
---

## Summary

Every `deploy = "rtic-*"` qemu-arm-baremetal image boots and brings up the
network but **never opens its zenoh session and never publishes/receives**, so
all four RTIC e2e tests fail with zero delivery:

```
RTIC QEMU pubsub: published=0, received=0     # test_qemu_rtic_pubsub_e2e
RTIC QEMU service client did not complete all calls   # …_service_e2e
RTIC QEMU listener received 0 messages
```

Serial (talker, with a router listening on the baked port `tcp/10.0.2.2:7450`):

```
  nros QEMU Platform
Initializing LAN9118 Ethernet...
  IP: 10.0.2.10
Ethernet ready.
<hangs here — no zenoh connect, no publish, no panic>
```

zenohd logs **no** incoming session — the guest never completes the TCP
handshake. Non-RTIC mps2 talkers over the *same* slirp/smoltcp path deliver
fine, so it is RTIC-entry-specific.

## Root cause

The RTIC scaffold opens the executor **inside `#[init]`**:
`nros::main!` → `RticBoardEntry::init_hardware_with_deploy` →
`init_with_config` → `::nros::Executor::open(&exec_config)`
(`nros-board-rtic-mps2-an385/src/lib.rs:248`). `Executor::open` performs the
**blocking zenoh-pico session open** = a TCP connect driven by the smoltcp poll
loop, which needs the CMSDK timer tick and LAN9118 RX interrupt to make
progress.

But RTIC runs `#[init]` **with interrupts globally masked** and before the
scheduler starts — so no timer tick and no RX IRQ fire while `Executor::open`
blocks on the handshake. The connect can never complete → `init` never returns
→ the `__nros_run` task never spins → zero delivery. "Ethernet ready" prints
because the NIC bring-up is synchronous; the blocking connect is the next step.

This is the same class as the FreeRTOS "poll-task priority / executor-on-stack"
constraints — blocking network I/O must not run in a no-interrupt context.

## Fix direction (architectural — RTIC entry contract)

Move the zenoh session open **out of `#[init]`** into the `__nros_run` task
(runs after `init` returns and interrupts are unmasked):

1. `RticBoardEntry::init_hardware` returns hardware + an **unopened**
   executor config (or a deferred-open handle); the `__nros_run` task calls
   `Executor::open` on its first poll, once the scheduler + timer are live.
2. Spans the macro emit (`nros-macros/src/main_macro.rs` RTIC `#[init]`/task),
   the `RticBoardEntry` trait (`nros-platform/src/board/rtic_entry.rs`), and the
   board impl (`nros-board-rtic-mps2-an385`, `-stm32f4`). This is the deferred
   Phase 216 dispatch-wiring follow-up the scaffold comments reference.

Alternatively, if RTIC exposes an interrupts-enabled late-init hook, open there.

## Notes

- Independent of #176 (heap OOM, fixed) and of the rtic-2.3.0 `Send` build fix
  (a zero-cost `__NrosLocalCell` newtype cannot change runtime behavior); those
  two only let the image build + boot far enough to expose this.
- Latent for a long time — the RTIC images could not build (rtic 2.3.0) then
  OOM'd (#176), so this init-time hang was never reached until both were fixed.
- `just check` (build/clippy) is green; this is a runtime-only defect on the
  `test-all` e2e lane.
