---
id: 2170
title: "The harness's sibling-pair QEMU launchers spell their machine lines by hand, beside the shared ladder"
status: resolved
type: tech-debt
severity: low
area: [testing, qemu]
related: [phase-517]
found: 2026-10-31
resolved: 2026-10-31
---

**Home: [phase-517](../../roadmap/archived/phase-517-user-flow-setup-build-flash.md)** — W14's **Left**.

## What

W14 made `packages/tooling/nros-qemu-ladder` the one source of each board's
QEMU line: the CLI prints `board_line`, and the harness builds the same line
through `nros_tests::qemu::board_line_cmd`, `nuttx_virt_line` and
`threadx_riscv64_line`. The launchers for SIBLING-PAIR networking (two
guests on one host segment, which the CLI never prints) were left as they
were, each with its own `-M` / `-cpu` / `-netdev` block in
`packages/testing/nros-tests/src/qemu.rs`:

* `QemuProcess::start_mps2_an385_mcast`, `start_mps3_an536_mcast`;
* `QemuProcess::start_nuttx_virt_mcast`, `start_nuttx_virt_dgram`;
* `QemuProcess::start_riscv64_virt_dgram` (ThreadX riscv64).

Their machine half (`-M`, `-cpu`, `-icount`, memory) restates what the
ladder states for the same board, so a change to a board's line in the
ladder does not reach them.

## Fix direction

* Give `nros-qemu-ladder` a socket network kind
  (`Net::Mcast { addr_port, localaddr }`, `Net::Dgram { local, remote }`)
  beside `Net::User` / `Net::None`, and build these launchers from
  `board_line`.
* The harness keeps the pair-specific plumbing (addresses, MAC per peer); the
  machine line comes from the ladder, as `threadx_riscv64_line` does.

## Resolution

`nros-qemu-ladder` gains `Net::Dgram { local, remote, mac }`, the AF_UNIX
pair link. It drops `-icount`: phase-160.K measured a 1 Hz timer firing
about once per 30 s under icount on a dgram pair. The ThreadX pair launcher
`start_riscv64_virt_dgram`, the one with callers, builds from `board_line`
now. `a_dgram_pair_shares_the_boards_machine_line` holds its line
byte-for-byte to what it spelled by hand.

The other three launchers this issue listed had no caller anywhere and are
deleted: `start_nuttx_virt_mcast`, `start_nuttx_virt_dgram` and
`start_mps2_an385_mcast`. `start_mps3_an536_mcast` stays. Its board is not a
ladder board: the MPS boards' line comes from their descriptor `runner`, a
second source this issue did not name, filed as issue 2172.
