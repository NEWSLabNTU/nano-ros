---
id: 2172
title: "The MPS2/MPS3 QEMU line has three spellings: the board's cargo runner, the CLI's printed line, and the harness's launchers"
status: resolved
type: tech-debt
severity: medium
area: [testing, qemu, cli]
related: [phase-544, phase-528, 2169, 2170, 2379]
found: 2026-10-31
resolved: 2026-11-09
resolved_in: "phase-544 W1: the harness reads [board.run.qemu] through nros_orchestration_ir::target_exec::board_run_named and BoardRun::qemu_plan_args, the mapping nros run and the cargo runner use; the MPS3-AN536 descriptor states -icount and its NIC model"
---

**Home: [phase-544](../../roadmap/phase-544-harness-run-plan-followups.md) W1** — rehomed 2026-11-06 from phase-528 (archived), whose W5 put the harness on `nros run --plan` and its QEMU lines on the shared renderer; reading `[board.run.qemu]` is what is left.

## What

Issue 2170 put the NuttX and ThreadX two-guest launchers on
`nros-qemu-ladder`. The MPS boards are outside that crate. Their line
comes from each board descriptor's cargo `runner`, and three places spell
it:

* **The board's `runner`** (`packages/boards/nros-board-mps2-an385*/` and
  `nros-board-mps3-an536-freertos/nros-board.toml`):
  `qemu-system-arm -cpu cortex-m3 -machine mps2-an385 -nographic
  -semihosting-config enable=on,target=native -kernel`. It has no
  `-icount` and no NIC.
* **The CLI's printed run command** (`cmd::build_next::cargo_runner`) is that
  runner verbatim, so `nros build` for a networked MPS2 image prints a line
  with no network.
* **The harness's launchers** (`nros_tests::qemu::QemuProcess::
  start_mps2_an385*`, `start_mps3_an536_mcast`, `start_mps3_an536_slirp`)
  add `-icount shift=auto` and a LAN9118 NIC by hand.

The book's measured line for a networked MPS2 image is the harness's, with
`-icount shift=auto -nic user,model=lan9118` (phase-517 W9). So the printed
command and the measured one disagree. The `runner` is right only for
`cargo run` of an image with no network.

## Fix direction

* Give the descriptor (or `nros-qemu-ladder`, with `Board::Mps2An385` /
  `Board::Mps3An536`) the board's machine line ONCE, plus its NIC model
  (`lan9118`). Build the printed command, the `runner` and the harness's
  launchers from it, each adding only its network: `user`, `socket,mcast`,
  or none.
* Carry `-icount shift=auto` wherever the image uses SLIRP. The
  `book-internals/src/testing/qemu-icount.md` rule says why.
* Issue 2169's per-board check of the printed command would have caught the
  missing NIC; land that check against this board too.

## Progress (2026-10-31)

Issue 2169's check (`every_probed_chapter_boot_line_is_the_printed_run_command`)
caught the missing NIC and `-icount` on both MPS2-AN385 boards. Their
`runner` now carries the chapter's measured line, so the runner, the printed
command and the book agree there. Two things are left:

* the harness's MPS launchers still spell the line by hand;
* the MPS3-AN536 runner still has no `-icount`. It has no probe track to
  compare against, and its harness pairs use `socket,mcast` and SLIRP with
  `hostfwd` rather than plain SLIRP.

## Progress (2026-11-06, phase-528 W5 step 2)

The harness's MPS launchers no longer spell a line. Each states plan args
(`nros_tests::run::QemuBoot`: `system`, `cpu`, `machine`, `icount`,
`semihosting`, `nic`), and the qemu runner renders them with
`nros_qemu_ladder::plan_line`, the renderer the CLI's plan uses too. So the
harness and `nros run` share the renderer. What is left: the harness still
states the board's facts itself rather than reading them from the board
descriptor's `[board.run.qemu]`, so a descriptor edit does not reach the
harness. Reading the descriptor (or giving the ladder `Board::Mps2An385` /
`Board::Mps3An536`) closes this issue. The MPS3-AN536 `-icount` point is
unchanged.

## Resolution (2026-11-09, phase-544 W1)

One source: the board descriptor's `[board.run.qemu]`, with one mapping
onto RunPlan QEMU args.

* `BoardRun` (with `BoardRunQemu`, `BoardRunRenode`, `EMULATOR_KINDS`)
  moved from `nros-cli-core` to `nros_orchestration_ir::target_exec`, which
  the CLI and the harness both depend on. `BoardRun::qemu_plan_args` is the
  one mapping of `system`, `cpu`, `machine`, `icount` and `semihosting` onto
  plan args. `nros run`'s plan uses it, and so does the cargo `runner`
  (`BoardDescriptor::qemu_argv`), which `nros_qemu_ladder::plan_line` now
  renders as well. So the runner, the printed command, `nros run` and the
  harness share both the facts and the renderer.
* The harness reads a board with `target_exec::board_run_named` through
  `nros_tests::qemu::DescriptorBoard` (`MPS2_AN385`, `MPS3_AN536`). Every
  MPS launcher starts from `DescriptorBoard::boot()` and adds only what is
  its own: the network the NIC model attaches to (QEMU's user network, a
  multicast socket, `hostfwd` rules, an image's network), a serial chardev, a
  monitor, or a measurement's fixed `-icount` (issue 2252).
* The MPS3-AN536 descriptor now states `icount = "shift=auto"` and
  `nic_model = "lan9118"`. That is the line its harness pairs measured, so
  `cargo run` and `nros run` boot it the same way.
* Guards. `check-harness-qemu-literals` rule 2 refuses, in
  `nros-tests/src` code, the QEMU machine or the NIC model of any tracked
  descriptor that states a `[board.run.qemu]` (the set is derived from the
  descriptors; negative control: the old `qemu.rs` gives 12 hits). Unit tests:
  `board_run_reader_agrees_with_the_catalog` (CLI: the harness's reader equals
  the catalog's, for every name of every in-tree descriptor);
  `the_mps_boots_are_the_descriptors_and_follow_an_edit` (harness: an edit to
  a descriptor's `icount` reaches the rendered line);
  `boards_on_one_qemu_machine_state_one_boot` (harness: the bare-metal and
  FreeRTOS MPS2 descriptors state one boot, so reading one is not a choice);
  `board_run_named_reads_one_board_and_maps_its_qemu_facts`
  (orchestration-ir).

Not changed, by design: the NuttX and ThreadX boots already come from the
one ladder (`nros_qemu_ladder::Board`), shared with the CLI's plan.
`start_cortex_m3` boots `lm3s6965evb`, which no descriptor states. The
Zephyr boots (`zephyr_qemu_boot`) restate Zephyr's `board.cmake`, because no
Zephyr board states a `[board.run.qemu]`: `nros run` boots a Zephyr image
through the `west` runner. The gate fires if one ever does.
**Superseded 2026-11-09 by [issue 2379](../2379-zephyr-qemu-boards-state-board-run-qemu.md):**
the Zephyr QEMU boards now state `[board.run.qemu]`, `zephyr_qemu_boot` is
gone, and the harness reads them through `nros_tests::zephyr::west_qemu_boot`,
which holds each to its build's own `run_qemu` line. `nros run` keeps the
`west` road.

## Verified (2026-11-09)

Pinned QEMU `11.0.0-nros8` from the box store; fixtures were built in the
same tree before the run.

* Unit: `nros-cli-core --lib` 2247 passed; `nros-tests --lib` 295 passed;
  `nros-orchestration-ir --lib` 341 passed. Clippy `-D warnings
  --all-targets` was clean on all three crates.
* QEMU cells (`cargo nextest`): `logging_smoke_qemu_baremetal_mps2_emits_every_severity`
  PASS, `test_qemu_lan9118_driver` PASS, `test_qemu_bsp_pubsub_e2e` PASS
  (bare-metal MPS2, networked pair), `logging_smoke_freertos_mps2_emits_every_severity`
  PASS (FreeRTOS MPS2, networked), and
  `an536_cyclonedds_cpp_entry_delivers_within_the_dds_heap_default` PASS
  (MPS3-AN536, multicast socket). The last one's line was `qemu-system-arm
  -machine mps3-an536 -nographic -icount shift=auto -semihosting-config
  enable=on,target=native -nic socket,model=lan9118,mcast=... -kernel ...`,
  so the descriptor's new `-icount` holds on a harness cell.
