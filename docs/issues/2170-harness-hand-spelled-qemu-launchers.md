---
id: 2170
title: "The harness's sibling-pair QEMU launchers spell their machine lines by hand, beside the shared ladder"
status: open
type: tech-debt
severity: low
area: [testing, qemu]
related: [phase-517]
found: 2026-10-31
---

**Home: [phase-517](../roadmap/archived/phase-517-user-flow-setup-build-flash.md)** — W14's **Left**.

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
