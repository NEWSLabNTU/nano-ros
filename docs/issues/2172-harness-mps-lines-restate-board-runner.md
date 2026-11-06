---
id: 2172
title: "The MPS2/MPS3 QEMU line has three spellings: the board's cargo runner, the CLI's printed line, and the harness's launchers"
status: open
type: tech-debt
severity: medium
area: [testing, qemu, cli]
related: [phase-544, phase-528, 2169, 2170]
found: 2026-10-31
---

**Home: [phase-544](../roadmap/phase-544-harness-run-plan-followups.md) W1** — rehomed 2026-11-06 from phase-528 (archived), whose W5 put the harness on `nros run --plan` and its QEMU lines on the shared renderer; reading `[board.run.qemu]` is what is left.

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
