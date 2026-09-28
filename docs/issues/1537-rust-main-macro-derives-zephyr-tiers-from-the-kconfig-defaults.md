---
id: 1537
title: "nros::main! still derives a Zephyr image's tier priorities from the
  Kconfig DEFAULTS projection - the RUST half of issue 1508, which the C/C++
  entry no longer has"
status: open
type: bug
area: [codegen, zephyr, scheduling]
severity: medium
found: 2026-09-28
related: [issue-1508, issue-1427, issue-0623, issue-0460, rfc-0079, rfc-0071]
---

## What is left

Issue 1508 made the two CMake roads allocate a derived tier table out of the
IMAGE's own priority plan: `nros codegen entry` and `nros codegen-system` take
`--dotconfig`, and `nano_ros_add_executable` / `nros_system_generate` pass
Zephyr's `${DOTCONFIG}`, which exists at configure time because Kconfig runs
inside `find_package(Zephyr)`. Measured on real native_sim images there.

The third road, the `nros::main!` proc-macro that bakes a RUST entry's table,
still calls `derive_tiers_from_contracts(&model, &rtos, ..)`
(`packages/core/nros-macros/src/main_macro.rs`), i.e.
`PriorityPlan::for_target("zephyr")` - 15 preemptive priorities, one transport
task at band 200, pool `[5, 14]`. On an image with
`CONFIG_NUM_PREEMPT_PRIORITIES=32` (transport `[7, 7]`) or
`CONFIG_NROS_ZENOH_READ_PRIORITY=100` (transport `[4, 9]`) a Rust image would
still run its most urgent derived tier at 5, above or inside its own transport:
issue 0623's inversion. Nothing reports it, because the table is baked by a
macro expansion and appears in no artifact a checker can read.

## Why it was not done under 1508

* **The kernel half is reachable, the transport half is not.** Cargo passes its
  environment to rustc, and the Zephyr Rust lane sets `DOTCONFIG` (build scripts
  read it via `nros_zephyr_build`), so the macro can see the `.config`. But
  which Kconfig symbols carry the transport bands is a BACKEND statement, and
  `nros-macros` is a core crate (RFC-0071 D2): the CLI can name
  `CONFIG_NROS_ZENOH_{READ,LEASE}_PRIORITY` (`image_priority_plan.rs`, held to
  the board descriptor's `[board.priority_plan] inputs` by a test); the macro
  may not. The numbers would have to arrive as numbers, and a Kconfig knob
  reaching the Rust lane is issue 0460's delivery problem.
* **No Rust Zephyr image in the tree derives a table that could verify it.**
  The Zephyr `nros::main!` entries are `workspaces/{safety,rust,realtime-rust}`;
  realtime-rust authors its tiers, and whether the other two derive was not
  measured. RFC-0079 section 4.1: no change justified by reasoning alone.
* **The macro would need a rebuild edge on `.config`.** A proc macro cannot
  declare a tracked file on stable, so the entry's build script would have to
  `rerun-if-changed` it, or a Kconfig edit would leave a stale table.

## Also open: no Zephyr boot report

FreeRTOS prints `report_tiers_above_transport` at boot (issue 0623); Zephyr
does not. It is the one detector that sees any image whatever road baked it.
Issue 1508 option 2, not taken there: with the allocation fixed on the CMake
roads it would only fire for a Rust derived image or an AUTHORED pin, and the
latter is already judged by `check-tier-priority-plan-image.py`.

## Acceptance

A Rust Zephyr image with one of the two shapes above either derives inside its
own `pool.app`, or refuses / reports at build or boot naming the band and the
knob - verified on a built image.
