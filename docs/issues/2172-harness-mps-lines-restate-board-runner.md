---
id: 2172
title: "The MPS2/MPS3 QEMU line has three spellings: the board's cargo runner, the CLI's printed line, and the harness's launchers"
status: open
type: tech-debt
severity: medium
area: [testing, qemu, cli]
related: [2169, 2170, phase-517]
found: 2026-10-31
---

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
  `docs/reference/qemu-icount.md` rule says why.
* Issue 2169's per-board check of the printed command would have caught the
  missing NIC; land that check against this board too.
