---
id: 1508
title: "A bake allocates derived priorities from Zephyr's Kconfig DEFAULTS
  projection, not from the image's own plan - so an image that moves
  CONFIG_NUM_PREEMPT_PRIORITIES or lowers a transport band gets a table above
  its own transport, and no lane judges a derived tier"
status: resolved
type: bug
area: [codegen, zephyr, scheduling]
severity: medium
found: 2026-09-27
related: [issue-1427, issue-0623, issue-0506, issue-0852, issue-1537, rfc-0079, phase-459]
resolved_in: "issue 1508 fix (C/C++ roads); Rust macro road -> issue 1537"
---

## Resolution

Fixed on the two CMake roads - the C/C++ entry TU, whose table the image runs,
and `codegen-system`'s `nros-plan.json` record - by allocating out of the
IMAGE's plan; the Rust `nros::main!` road is split out as
**issue 1537**, with the reasons it could not be done here. Verified on real
`native_sim/native/64` images (Zephyr 3.7, the `nano-ros-workspace` west tree,
this checkout named as the module via `-DZEPHYR_MODULES`), not only in unit
tests.

### The premise that did not hold

"Nothing on a bake road has a `.config`, because `nros sync` runs before
Zephyr's cmake." True of `nros sync` - but `nros sync` resolves the MODEL and
derives nothing. The two derivations that reach a C/C++ image both run at cmake
CONFIGURE time, inside the Zephyr build, AFTER Kconfig:

* `nano_ros_add_executable` -> `nros codegen entry` writes the entry TU whose
  `NativeTierSpec.priority` literals are what `k_thread_create` gets (phase-459
  W2 made the entry derive its own table);
* the Zephyr module's `nros_system_generate` -> `nros codegen-system` writes
  `nros-plan.json`, the record.

Both run after `find_package(Zephyr)`, where Zephyr's `DOTCONFIG` names the
image's `.config`. So candidate fix 3 was not a sequencing impossibility; it was
available at exactly the two places that mattered. Traced in a real configure
(`cmake --trace-expand`): both `execute_process` calls carry
`--dotconfig <build>/zephyr/.config`.

### Re-measured first

At the realizer boundary (`derive_tiers_from_contracts`, the contract model in
`derive.rs`'s tests), and again on real images with the real emitter:

| image `.config` | resolved transport | resolved `pool.app` | table before (projection) | table now |
| --- | --- | --- | --- | --- |
| defaults (15, bands 200/200) | `[4, 4]` | `[5, 14]` | 5 / 6 | **5 / 6** (unchanged) |
| `CONFIG_NUM_PREEMPT_PRIORITIES=32` | `[7, 7]` | `[8, 31]` | 5 / 6 - above the transport | **8 / 9** |
| `CONFIG_NROS_ZENOH_READ_PRIORITY=100` | `[4, 9]` | `[10, 14]` | 5 / 6 - inside the transport | **10 / 11** |

"5 / 6" is the 30 Hz / 10 Hz pairs of the phase-459 W0 fixture
(`examples/workspaces/derived-tiers-cpp`). The resolved bands above were read by
BOTH implementations off the built images' `.config` - the Rust `zephyr_plan`
and the Python checker `resolve_zephyr_plan` - and agree. The "before" column
on the 32-priority image is the same CLI run by hand without `--dotconfig`
against that image: `.priority = 5, 5, 6, 6` while its transport sits at 7.

### What changed

* `nros_orchestration_ir::derive::derive_tiers_in_plan` - the derivation with a
  caller-supplied `PriorityPlan`. `derive_tiers_from_contracts` is unchanged in
  signature and delegates with `PriorityPlan::for_target`, so the macro road and
  every existing caller are byte-identical.
* `nros_cli_core::orchestration::image_priority_plan` - `.config` -> plan. It
  names the two transport-band symbols (the core crate may not, RFC-0071 D2) and
  a test holds them, plus the four kernel symbols, EQUAL to the Zephyr board
  descriptor's `[board.priority_plan] inputs`, so a symbol added there cannot go
  missing here. An absent band symbol is the Kconfig default, as in the Python
  checker.
* `nros codegen entry --dotconfig` and `nros codegen-system --dotconfig`;
  `cmake/NanoRosEntry.cmake` and `zephyr/cmake/nros_system_generate.cmake` pass
  `${DOTCONFIG}` when it exists (only a Zephyr build defines it). A `.config`
  handed in for a non-Zephyr board is REFUSED, not ignored; a file with no
  `CONFIG_NUM_PREEMPT_PRIORITIES` is refused by name.
* An image whose Kconfig applies no transport priority at all
  (`CONFIG_POSIX_PRIORITY_SCHEDULING` or `CONFIG_PREEMPT_ENABLED` off - RFC-0079
  section 4.1 rule 2) has no band to allocate below. It keeps the projection,
  which is what every such image has always had, and the CLI prints a note
  saying the table is NOT judged against that image. This is common, not
  exotic: 107 of 142 built images in the local west workspace are in that state
  (every non-zenoh one; `NROS_ZENOH_MULTI_THREAD` is what selects the gate), so
  refusing would have broken builds.
* `nano_ros_entry` now prints the CLI's stderr on SUCCESS as a `STATUS`
  message. It was captured and dropped, so the derived-schedule degradations
  and refusals issue 1427 called "fail-loud" (and this change's plan line) never
  reached the operator on the cmake road. Measured in the configure:
  `-- nano_ros_entry(zephyr_entry): codegen entry: derived tier priorities
  allocate out of this image's plan - transport [4, 9], pool.app [10, 14]`.

### Why this fix, and why the other two alone were not enough

* **Allocate from the image's plan (chosen).** It removes the cause wherever
  the plan is knowable, and on the cmake roads it is always knowable.
* **Build-time check of derived tiers (option 1) - not needed on these roads.**
  Where the `.config` is in hand, checking the projection's numbers against it
  and refusing would be strictly worse than allocating correctly from it. It
  would also cover only C/C++, since a Rust table is in no parseable artifact.
* **Boot-time report (option 2) - not taken, recorded in issue 1537.** It is the
  one detector that sees any image whatever road baked it, but it fires only
  after flashing, and with the cmake roads fixed it would only catch the Rust
  road and authored pins (which `check-tier-priority-plan-image.py` already
  judges). It needs its own verification on an image that exercises it.

### Verified on real images, and what is NOT verified

Measured (native_sim/native/64, Zephyr 3.7, host toolchain):

* Configure + full build + link of the W0 fixture's Zephyr entry, default and
  `NUM_PREEMPT_PRIORITIES=32` (read-100 reconfigured and relinked through
  ninja). Generated `.priority` literals as in the table.
* **Incremental:** editing the Kconfig fragment 32 -> 20 and running `ninja`
  re-ran the configure and moved the table 8 / 9 -> 6 / 7 (pool `[6, 19]`).
  No wipe.
* **At run time**, under gdb with a `rmw_zenohd` up: the boot thread adopts, and
  the first spawned tier is created at, **6** (default), **9** (32), **11**
  (read-100) - `nros_zephyr_set_current_priority` /
  `nros_zephyr_tier_task_create` arguments. Those are the 10 Hz tiers; the
  image does not get as far as creating the 30 Hz ones, because every variant
  - the default included, so not this change - exhausts its 66,048-byte
  `CONFIG_NROS_ZEPHYR_HEAP_SIZE` arena declaring entities and then SEGVs in
  `z_impl_k_mutex_lock`. This W0 fixture has no `fixtures.toml` row, so no lane
  had ever booted it. Not investigated here.

NOT verified on an image:

* `codegen-system`'s derivation under `--dotconfig`. The flag reaches it in a
  real configure (traced), but on this fixture codegen-system derives NO tiers:
  the cmake callback groups do not reach it, which is issue 1426 / PR #1356. Its
  allocation is covered by the shared core's tests only.
* The unapplied-priority note on a real image (unit test only).
* The Rust road and a Zephyr boot report: issue 1537.

### Tests

| test | what it pins |
| --- | --- |
| `derive::a_derived_table_is_allocated_out_of_the_images_own_plan` | both shapes: projection 5/6 reproduces the inversion, image plan gives 8/9 and 10/11, all in `pool.app` and below the transport. Mutation: `derive_tiers_in_plan` ignoring its plan fails it |
| `derive::the_default_image_derives_what_the_projection_does` | the negative direction: default `.config` derives the identical table |
| `image_priority_plan::the_image_plan_is_read_from_its_own_kconfig` | the two shapes' bands from `.config` text; absent band symbols == the projection |
| `image_priority_plan::an_image_that_applies_no_priority_keeps_the_projection_and_says_so` | the unapplied note |
| `image_priority_plan::a_file_that_is_not_a_zephyr_config_is_refused_by_name` | a non-`.config` is refused naming the key |
| `image_priority_plan::a_dotconfig_for_a_non_zephyr_board_is_refused` | the wrong-board refusal |
| `image_priority_plan::the_band_symbols_are_the_descriptors_inputs` | the symbol list is the descriptor's |


## What issue 1427 left

phase-459 W4 made `realize_rtos` allocate out of a `PriorityPlan`
(RFC-0079), which fixed the allocation issue 1427 measured. The plan a BAKE
passes is `PriorityPlan::for_target(target_rtos)`
(`packages/core/nros-orchestration-ir/src/derive.rs:91`), and for Zephyr that
is the **Kconfig DEFAULTS projection**, documented as such: Zephyr's own
`CONFIG_NUM_PREEMPT_PRIORITIES=15` / `CONFIG_NUM_COOP_PRIORITIES=16` with ONE
transport task at the platform default band 200.

The image's real plan is a function of four per-image Kconfig values, which is
the whole reason RFC-0079 §4.1 calls Zephyr's band DERIVED.
`PriorityPlan::from_zephyr_dotconfig` resolves it and is called by **tests
only** - nothing on a bake road has a `.config` in hand, because `nros sync`
runs before Zephyr's cmake generates one.

## Measured, 2026-09-27, at 4d439a115 (island `.config`, `input_two` ranking)

| plan | `reserved.transport` | `pool.app` | rank 0 |
| --- | --- | --- | --- |
| defaults projection (what a bake uses) | `[4, 4]` | `[5, 14]` | 5 |
| island image, bands 200/255 | `[0, 4]` | `[5, 14]` | 5 |
| Kconfig defaults, bands 200/200 | `[4, 4]` | `[5, 14]` | 5 |
| **image with `CONFIG_NUM_PREEMPT_PRIORITIES=32`** | `[7, 7]` | **`[8, 31]`** | (bake still says 5) |
| **image with `CONFIG_NROS_ZENOH_READ_PRIORITY=100`** | `[4, 9]` | **`[10, 14]`** | (bake still says 5) |

The first three agree, which is why 1427's acceptance passes and why every
in-tree Zephyr image is correct today: nothing in the tree overrides
`CONFIG_NUM_PREEMPT_PRIORITIES` (Zephyr's default 15 everywhere) and no image
lowers a transport band below 200.

The last two are the defect. A board whose SoC defconfig sets a different
preemptive count, or an image that lowers a transport priority, gets a derived
table allocated from `[5, 14]` while its transport sits at k_thread 7 or in
`[4, 9]` - **the derived tier is more urgent than the transport that feeds
it**, which is issue 0623's inversion and exactly what 1427 was about, arrived
at through a different door.

## And nothing would catch it

`scripts/check-tier-priority-plan-image.py` DOES resolve the real plan from a
built image's `.config`, and the Zephyr fixture lane runs it
(`just/zephyr-ci.just:456`). It judges `scan_pins()` - **AUTHORED**
`[tiers.<name>.zephyr] priority` rows in `system.toml` files. A derived tier is
not authored anywhere: it is written into the resolved SystemModel, which is a
build artifact that `check-no-tracked-models` forbids committing. So the one
checker that can see an image's real band never sees the numbers this issue is
about.

## Candidate fixes, none of them free

1. **Judge derived tiers in the image checker.** The C/C++ generated entry TU
   carries the table as `nros_native_tier_spec_t` `.priority` literals, in the
   same build tree as the `.config` - so the pair the checker needs is on disk
   together. A RUST image's table is baked by the `nros::main!` proc-macro and
   appears in no parseable artifact, so this covers part of the matrix and must
   REPORT the part it cannot see rather than passing (issue 0196's rule).
2. **Give Zephyr a boot-time report.** FreeRTOS has
   `report_tiers_above_transport` (issue 0623); Zephyr has no equivalent, and it
   is the one place where both numbers exist in one vocabulary for ANY image,
   whatever its Kconfig. `nros_board_zephyr_run_tiers_ns` holds the tier array
   (RAW k_thread) and the platform can map a band through
   `nros_zephyr_native_priority` + `NUM_PREEMPT - posix - 1`. Needs a Zephyr
   build to verify, which is why it was not done under 1427.
3. **Get a `.config` to the derivation.** Not possible where the derivation runs
   today; it would mean re-deriving at cmake configure time, after Kconfig.

(1) and (2) are detectors and are complementary: (1) fails a build, (2) tells
an operator about an image nobody re-baked.

## Acceptance

An image whose Kconfig implies a pool other than `[5, 14]` is REPORTED - by a
lane, by its own boot output, or both - rather than silently carrying a derived
table from the defaults projection. Whichever detector lands, it must say what
it could not examine.
