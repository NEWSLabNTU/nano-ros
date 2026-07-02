---
id: 134
title: "nros-c action/common.rs uses AtomicU64 — no such intrinsic on riscv32 → qemu-riscv-nuttx C examples cannot build"
status: open
type: bug
area: nuttx
related: [phase-277]
---

## Summary

`packages/core/nros-c/src/action/common.rs` uses `core::sync::atomic::AtomicU64`,
which does not exist on 32-bit RISC-V targets (no 64-bit atomics). Building the
`qemu-riscv-nuttx` C example (`examples/qemu-riscv-nuttx/c/talker`) therefore
fails at the nros-c compile step. Pre-existing (baselined at `ea825a341`
during phase-277 W4); this is why the qemu-riscv-nuttx cell effectively cannot
be exercised beyond its committed state.

## Fix direction

Replace with `AtomicU32` (if the counter range allows), `portable-atomic`
(already used elsewhere in the embedded stack? verify), or a critical-section
guarded u64. Check other 32-bit-without-64-bit-atomics targets (thumbv7m) —
they build today, so either the code path is cfg'd out there or they get
atomics from elsewhere; mirror whatever mechanism they use.

## Next steps

1. `cargo check` nros-c for `riscv32imac-unknown-none-elf` (or the exact
   NuttX target triple from the example's `.cargo/config.toml`) to reproduce.
2. Land the portable fix; rebuild the qemu-riscv-nuttx c/talker example.
