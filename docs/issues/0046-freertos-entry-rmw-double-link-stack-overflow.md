---
id: 46
title: FreeRTOS Entry-pkg boots but stack-overflows at Executor creation — Component links both zenoh + cyclonedds rmw backends
status: open
type: bug
area: freertos
related: [issue-0045, phase-212]
---

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
