---
id: 2178
title: "The NuttX rv-virt Rust workspace image has no probe track"
status: resolved
type: test-gap
severity: low
area: [nuttx, testing]
related: [phase-517, 2149]
found: 2026-10-31
resolved: 2026-10-31
---

**Home: [phase-517](../../roadmap/archived/phase-517-user-flow-setup-build-flash.md)** — the one platform road the probe matrix does not
replay.

## What

Issue 2149 made a Rust NuttX workspace image build from `nros build`. The
new `nuttx-arm-rust` track replays it on Arm `virt`, and `nuttx-riscv`
replays the C package on rv-virt. The Rust image on `rv-virt-nuttx` is
replayed by nothing. Its kernel step reads the riscv board's own
`kernel_config` and `NUTTX_*` rows (unit-tested by
`a_nuttx_board_configures_its_kernel_from_its_own_facts`), but no build or
boot of that image has run on the installed track. Its target is a custom
JSON spec (`riscv32imac-unknown-nuttx-elf.json`) built with build-std, a
different road from the Arm triple.

## Fix direction

Add a `variant=riscv-rust` set of blocks to `integration-nuttx.md`: setup
as `riscv`, then the workspace and `nros new --image nuttx --board
rv-virt-nuttx`, `nros sync`, `nros build nuttx`, and the boot line
`nros build` prints. Give it an installed `nuttx-riscv-rust` track, and a
row in issue 2169's printed-command table.

## Resolution

`integration-nuttx.md` gains a `variant=riscv-rust` set of blocks. Setup is
shared with `riscv`. They build the workspace image with
`nros new --image nuttx --board rv-virt-nuttx` and `nros build nuttx`, then
boot it on QEMU riscv32 and expect the talker and listener. The new
installed track `nuttx-riscv-rust` runs them and is green on run
41198874371. The boot line is a row in issue 2169's printed-command table,
and it matches.
