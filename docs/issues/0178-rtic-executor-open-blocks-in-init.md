---
id: 178
title: "RTIC mps2-an385 images never deliver — blocking zenoh connect vs RTIC (init-mask [fixed] + wfi-yield / no monotonic [open])"
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

## Root cause — three layers (layer 1 fixed; 2–3 open)

The delivery failure is a stack of RTIC ↔ blocking-network-I/O mismatches, each
uncovered only after fixing the one above it.

### Layer 1 — `Executor::open` in `#[init]` (interrupts masked) — FIXED

The scaffold opened the executor **inside `#[init]`**: `nros::main!` →
`RticBoardEntry::init_hardware_with_deploy` → `Executor::open`. That is a
**blocking** zenoh-pico session open (a TCP connect driven by the platform poll
loop), and RTIC runs `#[init]` **with interrupts masked**, before the scheduler
starts — so it deadlocks there.

**Fixed** by deferring the open: `RticBoardEntry` now returns a `Boot` carrier
from `init_hardware` (hardware up, no network I/O) and exposes
`open_executor(boot)`, which the generated `__nros_run` task calls on its first
poll — after `init` returns and interrupts unmask. Verified: a probe print at
`open_executor` fires and the task reaches the open in task context. Spans the
trait (`nros-platform/src/board/rtic_entry.rs`), the macro
(`nros-macros/src/main_macro.rs`), and both board impls
(`nros-board-rtic-{mps2-an385,stm32f4}`).

### Layer 2 — the connect needs `wfi` to yield to QEMU (slirp) — OPEN

Even opened from the task, the connect still hangs (talker stalls right after
`Ethernet ready`; zenohd logs **no** incoming session). The zenoh-pico connect
busy-waits with a poll + a **`wfi` idle hook** whose whole job (per
`nros-board-mps2-an385/src/lib.rs:83`) is to *"release the CPU to QEMU's main
loop between iterations"* so **host-timed slirp** can deliver the handshake
packets. Under `-icount shift=auto`, a pure spin races virtual time ahead of
wall-clock, so slirp never delivers → hang.

That hook (`nros_board_mps2_an385::enable_wfi_idle`) is **never called** anywhere
in the tree — the direct-exec path evidently yields some other way, but the RTIC
scaffold does not install it.

### Layer 3 — `wfi` needs an armed periodic IRQ, via RTIC's monotonic — OPEN

`enable_wfi_idle`'s own docs: *"Must be called AFTER an IRQ source is armed; for
RTIC examples that means immediately after `Mono::start` … wfi with no pending
interrupt deadlocks."* RTIC declares **no monotonic**, so SysTick is unarmed →
no periodic IRQ to wake `wfi`. Confirmed: calling `enable_wfi_idle` without an
armed IRQ hangs *earlier* (first `wfi` never wakes).

Attempted arming SysTick directly with a board-crate `#[cortex_m_rt::exception]
fn SysTick` + `enable_wfi_idle` — it builds but does **not** fix delivery: the
board-crate exception handler is not wired into the `#[rtic::app]`-generated
vector table (RTIC owns it), so the tick never fires. The periodic IRQ must come
through RTIC's own mechanism.

## Fix direction (remaining — layers 2 + 3)

1. Give the generated `#[rtic::app]` a **monotonic** (`rtic-monotonics` Systick):
   add the dep to the RTIC examples, and have the macro emit
   `Mono::start(cx.core.SYST, <freq>)` in `#[init]`. This arms the periodic IRQ
   through RTIC (correct vector wiring + priority).
2. After `Mono::start`, call the board's `enable_wfi_idle()` — add a
   `RticBoardEntry` hook (e.g. `fn on_scheduler_ready()`) the macro invokes, or
   fold it into `open_executor`'s prologue once the monotonic is guaranteed live.
3. Verify SysTick fires *during* the priority-1 `__nros_run` task (its exception
   priority must outrank the task) so `wfi` in the connect busy-wait wakes.

## Notes

- Independent of #176 (heap OOM, fixed) and of the rtic-2.3.0 `Send` build fix
  (a zero-cost `__NrosLocalCell` newtype cannot change runtime behavior); those
  two only let the image build + boot far enough to expose this.
- Latent for a long time — the RTIC images could not build (rtic 2.3.0) then
  OOM'd (#176), so this hang was never reached until both were fixed. It is
  plausible these `test_qemu_rtic_*_e2e` tests have not passed since the
  `Executor<'s>` per-entry rework; treat green here as unproven history.
- Layer 1 (deferral) is landed; `just check` stays green. Layers 2–3 are the
  remaining runtime-only work on the `test-all` e2e lane.
- 2026-07-14 (phase-287 W7 sweep): confirmed on freshly rebuilt images
  (`build/fixtures-cargo/qemu-arm-baremetal` rebuilt in the same sweep) — all
  four `test_qemu_rtic_*_e2e` still 0 messages, 3/3 retries at reduced load.
  Matches layers 2–3 being the live cause.
