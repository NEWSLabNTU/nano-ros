---
id: 2379
title: "The Zephyr QEMU boards state no [board.run.qemu], so the harness restates their QEMU line and nothing holds it to Zephyr's board.cmake"
status: open
type: tech-debt
severity: low
area: [testing, qemu, zephyr, boards]
related: [issue-2172, phase-524, phase-528]
found: 2026-11-09
---

**Home: [phase-524](../roadmap/phase-524-emulator-twins.md)** (its "Issues
homed here" table). Phase-544, where issue 2172's one-source rule landed, is
closing; phase-524 owns the emulator boards and RFC-0115's "every board fact
is typed once".

## What

Issue 2172 (phase-544 W1) made the test harness read a QEMU board's line
from its descriptor's `[board.run.qemu]` (`nros_tests::qemu::DescriptorBoard`,
`BoardRun::qemu_plan_args`), and gate `check-harness-qemu-literals` rule 2
refuses a descriptor board's machine or NIC model typed in the harness. Its
"Not changed, by design" paragraph left the Zephyr boards out: no Zephyr
board stated a `[board.run.qemu]`, because `nros run` boots a Zephyr image
through the `west` runner, which reads Zephyr's own `board.cmake`.

So two Zephyr boots were still typed by hand:

* **`mps2-an385-zephyr`.** The Zephyr Cortex-M cells
  (`tests/zephyr_cortex_m_qemu.rs`) boot through
  `QemuProcess::start_mps2_an385_*`, which read the BARE-METAL board's
  descriptor (`nros-board-mps2-an385`). The Zephyr descriptor stated only
  `emulator = "qemu:mps2-an385"`. Right by coincidence: the two boards share
  a machine, and nothing said the Zephyr image boots the bare-metal line.
* **`qemu-cortex-a53`** (`qemu_cortex_a53/qemu_cortex_a53/smp`).
  `nros_tests::zephyr::zephyr_qemu_boot` typed `aarch64`,
  `virt,gic-version=3`, `cortex-a53`, `model=e1000` and `-smp cpus=2`.
  The descriptor's comment restated the machine and cpu in prose.
* `qemu_cortex_m3` (`ZephyrPlatform::QemuArm`, `lm3s6965evb`) had a typed
  line too, and no test booted it.

And no check held any of these to Zephyr's `board.cmake`, which is what
`west build -t run` boots.

## Ruling: what `[board.run.qemu]` on a Zephyr board changes

Measured in `cmd::run_plan::resolve_run`: on a west build, a `qemu` emulator
goes to the `west` runner (`WEST_EMULATORS`) before `[board.run.qemu]` is
read, and the QEMU args block runs only for the `qemu` runner. So stating the
table changes nothing `nros run` does on the west road. `nros run` keeps the
west road: `west build -t run` is Zephyr's own launch, and there is no
measured reason to switch. The table is read by the harness, and by
`nros run` only when an image names the `qemu` runner itself
(`[image.<id>.run] method = "qemu"`). The cargo `runner` it would render is
for cargo-road boards; a Zephyr board is on the west road and never reaches
it.

The descriptor states the board facts once (`system`, `machine`, `cpu`,
`smp`, `nic_model`, plus the harness's measured `icount`). Zephyr's
`board.cmake` is upstream and is not edited, so the two are held together by
a COMPARISON, not a derivation: every Zephyr QEMU cell, before it boots,
reads the `run_qemu` command its own west build generated (`build.ninja`,
the line `west build -t run` runs) and refuses to boot when the descriptor's
`system`, `machine`, `cpu`, `smp` or NIC model disagrees with it. The facts
nano-ros adds and Zephyr does not state (`icount`, `semihosting`) and the
network attachment (the caller's) are not compared.

## Fix

* **Descriptors.** `nros-board-mps2-an385-zephyr` states `[board.run.qemu]`
  `system = "arm"`, `cpu = "cortex-m3"`, `icount = "shift=auto"`,
  `nic_model = "lan9118"` (no `semihosting`: a Zephyr image prints on its
  UART, and Zephyr's line has none). `nros-board-qemu-cortex-a53` states
  `system = "aarch64"`, `machine = "virt,gic-version=3"`,
  `cpu = "cortex-a53"`, `smp = "cpus=2"`, `nic_model = "e1000"`.
* **`smp`.** A new `[board.run.qemu]` key
  (`nros_orchestration_ir::target_exec::BoardRunQemu::smp`), mapped by
  `qemu_plan_args` and rendered by `nros_qemu_ladder::plan_line` as
  `-smp <value>` after the NIC.
* **Harness.** `nros_tests::zephyr::west_qemu_boot(board, image)` reads the
  image's `build.ninja`, takes the `run_qemu` command
  (`west_run_qemu_argv`), refuses to boot on any disagreement
  (`west_run_disagreements`), and returns the descriptor's boot on QEMU's user
  network. New `DescriptorBoard`s `MPS2_AN385_ZEPHYR` and `QEMU_CORTEX_A53`;
  `QemuProcess::start_zephyr_networked`, `start_zephyr_mps2_an385_networked_measured`
  and `start_zephyr_mps2_an385_networked_with_monitor` replace the MPS2
  launchers the Zephyr cells used (the old `_measured` and `_with_monitor`
  had no other caller and are gone). `zephyr_qemu_boot` is gone, and so is
  `ZephyrPlatform::QemuArm` (`qemu_cortex_m3`), which no test booted.
* **Tests.** `zephyr_qemu_descriptors_agree_with_the_west_run_line` (both
  descriptors against Zephyr 3.7's generated lines; an edit to `system`,
  `cpu` or `nic_model` is reported; `-smp` and the machine options are
  compared); `boards_on_one_qemu_machine_state_one_boot` now holds every
  descriptor the harness does not read to one it does;
  `a_zephyr_qemu_table_leaves_the_west_road_alone` (CLI: the ruling above).
* **Gate.** `check-harness-qemu-literals` rule 2 now derives the a53's
  machine and NIC model too, and its self-tests carry the line
  `zephyr_qemu_boot` typed. A quoting test in `qemu.rs` that used
  `model=e1000` as sample text now uses `virtio-net-pci`.

## Verified (2026-11-09)

In the `ros2` box, Zephyr 3.7, fixture `build-cortex-m-c-talker-zenoh` built
fresh in the same tree after `just setup-cli`.

* `zephyr_cortex_m_c_zenoh_pubsub_e2e` PASS (14.4 s, Zephyr's in-kernel
  stack over the LAN9118, publishes delivered), booted from the Zephyr
  descriptor with no semihosting.
* Negative control: `cpu = "cortex-m4"` in the descriptor made the same cell
  refuse to boot with `cpu: descriptor Some("cortex-m4"), west
  Some("cortex-m3")` and Zephyr's full line; reverted.
* Unit: `nros-orchestration-ir --lib` 345, `nros-qemu-ladder` 5,
  `nros-tests --lib` 296, `nros-cli-core --lib` 2289, all passed. Clippy
  `-D warnings --all-targets` clean on all four crates.
* Not run: the `qemu_cortex_a53` smp cell (`sched_dims_applied_e2e`); its
  descriptor is checked against the generated line in the unit test only.
