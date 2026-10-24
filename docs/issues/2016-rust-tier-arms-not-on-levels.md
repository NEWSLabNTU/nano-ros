---
id: 2016
title: "The Rust Linux, FreeRTOS and NuttX tier arms still open one executor per tier"
status: open
type: tech-debt
area: [executor, linux, freertos, nuttx]
severity: low
found: 2026-10-24
related: [phase-509, rfc-0108, 2020]
---

## Measured

phase-509 P10 moved every C-ABI tier runner onto levels: one executor per
core and a worker per level (RFC-0108 "As built in P10"). It also moved two
Rust runners: Zephyr (`ZephyrBoard::run_tiers_with_task_memory`) and ThreadX
(`run_tiers_entry`). Both use `nros::levels::LevelCore`.

Three Rust tier arms were not moved:

| arm | file | shape today |
| --- | --- | --- |
| Linux | `nros-board-linux/src/lib.rs` `LinuxBoard::run_tiers` | `std::thread::scope`, `Executor::open_with_session_slot` per spawned tier |
| FreeRTOS | `nros-board-freertos/src/entry.rs` `run_tiers_entry` | `Executor::open_with_session_handle_slot` per spawned tier |
| NuttX | `nros-board-nuttx/src/lib.rs` `run_tiers` | an executor per tier over the session |

A Rust image on these boards therefore pays a context and a backing per tier.
Its tiers order work inside each executor, not across one ready set, which is
the shape P9 had before P9b.

The Linux arm is the hardest of the three. It closes its session at the end
(`close_session`), honours the termination guard, and uses scoped threads.
`LevelCore` leaks its runtime for the image's life and has no teardown.

## Closes when

All three arms run on `LevelCore`, or on a shared Rust level runner beside it.
Each must keep its `realtime_tiers_e2e` row green: native/rust,
native/rust-derived, freertos/rust, nuttx-arm/rust and nuttx-riscv/rust.
`LevelCore` must also gain a teardown that the hosted arm can call.
