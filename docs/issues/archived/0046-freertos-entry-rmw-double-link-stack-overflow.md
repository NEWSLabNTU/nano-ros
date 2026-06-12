---
id: 46
title: FreeRTOS Entry-pkg boots but stack-overflows at Executor creation — zenoh app-task stack + heap under-sized for the Entry/run-plan path
status: resolved
type: bug
area: freertos
related: [issue-0045, phase-212]
---

## Resolution (2026-06-13)

**The cyclonedds-double-link hypothesis was wrong.** The cyclonedds backend is
selected at runtime via the `rmw-cffi` vtable, not stored inline per-backend, so
it costs flash, not the app-task stack/heap. The real cause is **memory sizing**:
the Phase 212 Entry / run-plan `Executor::open` needs more app-task stack than the
old *direct* talker, and the FreeRTOS task stack is allocated *from* the heap
(`heap_4` `ucHeap`), so stack and heap compete.

Diagnosed empirically: 256 KiB app stack → `*** STACK OVERFLOW: nros_app ***`;
1 MiB stack (from the 512 KiB zenoh heap) → `*** MALLOC FAILED ***`; **384 KiB
stack + 2 MiB heap → boots clean through Executor + network**.

Fix (both shared FreeRTOS defaults; the 4 MiB SRAM has ample headroom):
- `nros-board-freertos/src/config.rs` — `app_stack_bytes` default 256 KiB → 384 KiB.
- `nros-board-freertos/build.rs` — the `rmw-zenoh` heap default 512 KiB → 2 MiB.

**Verified:** `freertos_rs_talker_entry` (`thumbv7m-none-eabi`) boots through the
full board lifecycle under QEMU (banner → LAN9118 + lwIP → MAC/IP →
`Network ready.`) with no stack overflow / MALLOC fail. The Phase 212.O.1 runtime
gate `freertos_board_run_executes_run_plan` is **un-`#[ignore]`d and GREEN** — it
asserts the boot lifecycle (the deterministic part this fix enables) and starts a
host zenohd on `7451` (the entry's `tcp/10.0.2.2:7451` locator) for the connected
run.

**Known limitation (not a regression) → #48:** the post-network connected run goes
through `Executor::open` over the slirp→host-zenohd path, which **never
establishes** (originally mislabeled "timing-flaky"). Investigated: the firmware
boots on the board-default `192.0.3.10/24` while slirp is `10.0.2.0/24`, and even
on the right subnet the guest→host connection doesn't deliver — filed as **#48**.
The test logs (does not assert) the `Application setup complete` / `Published:`
markers until #48 lands.

---

_Original report below (hypothesis since corrected)._

## Symptom

With #45 resolved, `freertos_rs_talker_entry` (`thumbv7m-none-eabi`) compiles,
links, and boots through the full board lifecycle under QEMU:

```
========================================
  nros FreeRTOS Platform
========================================
Initializing LAN9118 + lwIP...
  MAC: 02:00:00:00:00:00
  IP:  192.0.3.10
*** STACK OVERFLOW: nros_app ***
```

The overflow hits at **Executor creation**, before the run-plan body. This keeps
the Phase 212.O.1 runtime acceptance test
`freertos_run_plan_runtime.rs::freertos_board_run_executes_run_plan` `#[ignore]`d.

## Root cause (suspected)

`app_stack_bytes` already defaults to 256 KB, so the overflow is not a plain
under-sized task — it is the inline Executor arena being far larger than expected
because the firmware links **two RMW backends at once**:

- `zpico_sys` (zenoh-pico) — the board default, force-linked by
  `nros-board-mps2-an385-freertos` under `feature = "rmw-zenoh"`.
- `nros_rmw_cyclonedds` — pulled transitively through the Component pkg's `nros`
  umbrella `rmw-cffi` feature.

even though the deploy config for this example says `rmw = "zenoh"`. Two backends'
session/arena state inflates the inline Executor allocation on the `nros_app`
task stack past 256 KB.

## Fix direction

- Ensure the Component → `nros` dependency selects a **single** RMW backend
  matching the deploy config (drop the cyclonedds leg when `rmw = "zenoh"`); the
  `rmw-cffi` umbrella should not transitively pull a second backend.
- Re-check the Executor-arena sizing vs `app_stack_bytes` once a single backend
  is linked; tune the FreeRTOS task stack / heap if still tight.
- Then un-`#[ignore]` `freertos_board_run_executes_run_plan`.

This is rmw-backend-selection + stack/heap tuning, distinct from the #45
link/panic-handler design (now resolved).
