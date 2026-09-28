---
id: 1537
title: "nros::main! still derives a Zephyr image's tier priorities from the
  Kconfig DEFAULTS projection - the RUST half of issue 1508, which the C/C++
  entry no longer has"
status: resolved
type: bug
area: [codegen, zephyr, scheduling]
severity: medium
found: 2026-09-28
related: [issue-1508, issue-1427, issue-0623, issue-0460, issue-1551, rfc-0079, rfc-0071]
resolved_in: "issue 1537 fix: nros::main! reads $DOTCONFIG; Zephyr boot report"
---

## Resolution

Fixed both ways the acceptance allows: a Rust Zephyr image now DERIVES inside
its own `pool.app`, and every Zephyr tier image REPORTS at boot a tier that
meets or outranks its transport. Measured on real `native_sim/native/64` images
(Zephyr 3.7, the `nano-ros-workspace` west tree, this checkout named as the
module via `-DZEPHYR_EXTRA_MODULES`).

### The crux, measured: `.config` reaches expansion, and an edit re-expands

* **Reachable.** zephyr-lang-rust's `rust_cargo_application` runs cargo as
  `cmake -E env ... DOTCONFIG=<build>/zephyr/.config ... cargo build` (read off
  the failing ninja command line of the first attempt), and cargo hands its
  environment to rustc, so the proc-macro reads it with `std::env::var_os`. The
  expansion's own stderr line names the file:
  `nros::main!: derived tier priorities allocate out of this image's plan -
  transport [4, 4], pool.app [5, 14] (<build>/zephyr/.config)`. `DOTCONFIG` is
  the one Kconfig input that does reach the Rust lane; issue 0460 is about the
  knobs cmake exports with `set(ENV)`, which this does not use.
* **Fresh.** The macro puts the `.config` on its `tracked` list (emitted as
  `include_bytes!`, so rustc lists it in the crate's dep-info) and names
  `option_env!("DOTCONFIG")` (an `env-dep`, so a changed or removed VARIABLE
  rebuilds too). Measured: editing the image's Kconfig fragment and running
  `ninja` alone - no wipe, no reconfigure by hand - moved the baked table each
  time: default 5 / 6 -> `CONFIG_NUM_PREEMPT_PRIORITIES=32` 8 / 9 ->
  `CONFIG_NROS_ZENOH_READ_PRIORITY=100` 10 / 11. (The entry's own build script
  also `rerun-if-changed`s `$DOTCONFIG` via `nros_zephyr_build::bake_nros_config`,
  so on this leaf the two edges overlap; the macro's does not depend on a leaf
  having that build script.)

So the premise "the macro would need a rebuild edge on `.config`" had an answer
on stable: the same `include_bytes!` tracking the macro already uses for every
other input.

### What changed

* **One spelling, three roads.** The transport-band symbols and their reader
  moved from `nros-cli-core` to `nros_entry_lower::zephyr_image` - a CLI-tree
  leaf the macro can afford (issue 0083) that gains no dependency for it, so no
  lockfile moved. The core crate still receives the bands as NUMBERS (RFC-0071
  D2). The resolved / unapplied / not-a-`.config` classification moved into
  `nros_orchestration_ir::priority_plan::zephyr_image_plan` (+ `ImagePlan`,
  `PriorityPlan::describe_allocation`). `nros-cli-core`'s
  `image_priority_plan` is now a thin eyre wrapper over the same two calls and
  keeps `the_band_symbols_are_the_descriptors_inputs`, which now holds the
  moved list to the board descriptor.
* **`nros::main!`** (`zephyr_image_plan_from_env`): on a Zephyr expansion that
  derived tiers, re-derive with `derive_tiers_in_plan` out of the image's plan.
  No `$DOTCONFIG` (a host `cargo check`) or an image that applies no priority
  keeps the projection and prints a note saying it is NOT judged; an unreadable
  file or a file that is not a `.config` is a compile error naming it. An image
  that derives nothing never reads the file, so no existing image can start
  failing on it.
* **Zephyr boot report** (`nros_zephyr_report_tier_vs_transport`,
  `zephyr/nros_platform_zephyr_shims.c`), called per tier by BOTH tier arms
  (`entry_tiers.rs`, `zephyr_run_tiers.c`) after the session opens and before
  any tier runs. Mirrors FreeRTOS's `report_tiers_above_transport`: a report,
  not an error; header + transport line + guidance, then one line per offending
  tier. The transport side is READ BACK, not assumed: `nros_zephyr_task_create_prio`
  records, for every task it creates at an explicit priority, what
  `pthread_getschedparam` says the thread got, so a refused policy or a
  mapping bug is reported as what runs. An image with no such task prints, once,
  that its tiers were NOT checked.

### Verified on real images

Throwaway workspace: `examples/workspaces/realtime-rust` copied under `tmp/`
with its `[tiers.*]` removed and a contract giving ctrl 100 Hz / telem 10 Hz, so
`nros::main!` DERIVES (no in-tree Rust Zephyr image derives - the three that
could all author or declare no contract). Priorities read under gdb at
`nros_zephyr_tier_task_create` / `nros_zephyr_set_current_priority`, with a
private `rmw_zenohd` (stopped after each run):

| image `.config` | macro's plan line | spawned / boot tier | boot report |
| --- | --- | --- | --- |
| defaults | transport `[4, 4]`, pool `[5, 14]` | 5 / 6 | silent |
| `NUM_PREEMPT_PRIORITIES=32` | `[7, 7]`, `[8, 31]` | 8 / 9 | silent |
| `ZENOH_READ_PRIORITY=100` | `[4, 9]`, `[10, 14]` | 10 / 11 | silent |
| `ZENOH_READ_PRIORITY=100`, macro reverted to the projection | - | **5 / 6** | **fires**: `transport: zpico_read 9, zpico_lease 4 (floor 9)`, both tiers named |

The last row is the defect reproduced on an image and caught by the report; the
transport numbers the kernel reported (9 and 4) are exactly the plan's `[4, 9]`.
The C/C++ arm: `examples/workspaces/realtime-cpp`'s Zephyr entry with
`ZENOH_READ_PRIORITY=100` - its AUTHORED `[tiers.high.zephyr] priority = 9`
ties the read task and the report names it (`tier `high` at 9 <= 9`), while
`low` at 10 is not named. That image needed `CONFIG_NROS_ZEPHYR_HEAP_SIZE`
raised to boot at all (`HEAP EXHAUSTED: request 90520 bytes, arena 66048`,
issue 1551's arena) - raised in the throwaway fragment only, not in the tree.

**Negative direction.** A default-band image derives exactly what it did: 5 / 6
on the real image, `a_default_band_image_resolves_exactly_the_projection`
(macro) and 1508's `derive::the_default_image_derives_what_the_projection_does`
(same `PriorityPlan` in, byte-identical `tiers` out).

### Not verified

* The unapplied note and the boot report's "NOT checked" line on an image:
  `CONFIG_POSIX_PRIORITY_SCHEDULING=n` does not stick on the zenoh images
  (Kconfig reselects it), so no buildable image reaches either. Unit tests only
  (`an_unapplied_image_keeps_the_projection_and_a_bad_file_is_refused`).
* A real board: native_sim only.
* No in-tree fixture derives on the Rust Zephyr road, so no lane re-proves this;
  the measurement above used a throwaway workspace.

### Tests

| test | what it pins |
| --- | --- |
| `nros-macros` `image_plan_tests::a_zephyr_expansion_allocates_out_of_the_images_own_plan` | both shapes resolve the image's plan; the `.config` is TRACKED and `$DOTCONFIG` is an env-dep |
| `image_plan_tests::a_default_band_image_resolves_exactly_the_projection` | negative direction |
| `image_plan_tests::no_dotconfig_or_not_zephyr_keeps_the_projection` | host check / other RTOS untouched; unset variable still tracked |
| `image_plan_tests::an_unapplied_image_keeps_the_projection_and_a_bad_file_is_refused` | unapplied note, not-a-`.config`, unreadable file |
| `nros-entry-lower` `zephyr_image::an_absent_band_is_the_default_and_the_last_assignment_wins` | the moved reader |

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
