---
id: 399
title: qemu-esp32-baremetal declares no C toolchain, so cc-rs guesses
  `riscv32-unknown-elf-gcc` and the example cannot build on a provisioned host
status: open
type: bug
area: build
related: [0368, 0398]
---

## Problem

`just check-examples` builds `examples/qemu-esp32-baremetal/rust/{talker,listener}`
for `riscv32imc-unknown-none-elf`. Those pull `zpico-sys`, which compiles C. No
one tells cc-rs which compiler to use:

- the example's `.cargo/config.toml` sets `target` and `linker`, but no
  `CC_riscv32imc-unknown-none-elf` (the build log confirms:
  `CC_riscv32imc-unknown-none-elf = None`),
- `nros-sdk-index.toml` has `[board.qemu-esp32-baremetal] packages = []` — the
  board installs NO toolchain,
- `nros-zpico-build`'s own candidate list (`src/lib.rs:520,530,559`) offers
  `riscv64-unknown-elf-gcc` and `riscv32-esp-elf-gcc`, and is not consulted on
  this path anyway.

So cc-rs falls back to its derived default and the build dies:

```
error occurred in cc-rs: failed to find tool "riscv32-unknown-elf-gcc": No such file or directory
```

`nros setup` provisions `riscv-none-elf-gcc` (`[tool.riscv-none-elf-gcc]`, for
the RISC-V NuttX board) — a multilib toolchain that can target rv32imc — but
nothing bridges the name, so a host provisioned exactly as documented still
cannot build this example. It passes only where some *other* riscv32 gcc
happens to be installed under one of the two names above, which is why CI is
green.

## Repro

```sh
cd examples/qemu-esp32-baremetal/rust/talker && cargo clippy
```

Fails on any host whose only riscv toolchain came from `nros setup`.

## Fix sketch

Decide which compiler this board is supposed to use, then say it in ONE place:

- if `riscv-none-elf-gcc` is intended, add `packages = ["riscv-none-elf-gcc"]`
  to `[board.qemu-esp32-baremetal]` and set
  `CC_riscv32imc-unknown-none-elf = "riscv-none-elf-gcc"` (plus the
  `-march=rv32imc -mabi=ilp32` flags) where the board's env is defined;
- if the ESP toolchain is intended, add it to the index as a package of this
  board so `nros setup qemu-esp32-baremetal` installs it.

Either way the board should declare its C toolchain rather than relying on a
name cc-rs guesses. Worth checking the other `packages = []` boards for the
same hole.

## Notes

Found while running tier 1 in the ROS distrobox for the issue-0383 `-Werror`
work (2026-08-03). NOT caused by that change: it reproduces identically with
`NROS_CC_STRICT_DECLS=0`, which disables the new flag helper entirely.
