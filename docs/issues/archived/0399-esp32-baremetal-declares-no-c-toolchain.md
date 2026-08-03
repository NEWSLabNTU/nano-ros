---
id: 399
title: qemu-esp32-baremetal declares no C toolchain, so cc-rs guesses
  `riscv32-unknown-elf-gcc` and the example cannot build on a provisioned host
status: resolved
type: bug
area: build
related: [0368, 0400]
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

## RESOLVED (2026-08-03)

The board is `riscv32` bare-metal, and `nros setup` already ships
`riscv-none-elf-gcc` (xPack multilib, rv32imc/ilp32 capable). So `riscv-none-elf-gcc`
is the intended compiler — the fix is to *say so* and make zpico's own detection
look for it, rather than pinning a per-example `CC_*` env.

The root was the CLASS, not one env var: `nros-zpico-build`'s three riscv probes
(`detect_riscv_compiler`, `get_picolibc_sysroot`, `has_picolibc_specs`) each
hard-coded the SAME two-name candidate list
(`["riscv64-unknown-elf-gcc", "riscv32-esp-elf-gcc"]`) and all three omitted the
provisioned toolchain. And the shim build path (`runner.rs` `build_c_shim`,
`if target.contains("riscv32imc")`) set `-march`/`-mabi` but NEVER set the
compiler, so cc-rs derived `riscv32-unknown-elf-gcc` from the triple — a name
nothing installs.

Fix (`packages/rmw/zenoh/nros-zpico-build/src/{lib,runner}.rs`,
`nros-sdk-index.toml`):

1. one shared `RISCV_GCC_CANDIDATES` const (the three probes now reference it,
   so they can never diverge again) with `riscv-none-elf-gcc` appended;
2. `build_c_shim`'s riscv32imc branch now calls `detect_riscv_compiler(&mut build)`,
   mirroring the manifest-driven lib path (`apply_arch`) — so BOTH C-build paths
   pick a real compiler instead of letting cc-rs guess;
3. `[board.qemu-esp32-baremetal] packages = ["riscv-none-elf-gcc"]` so
   `nros setup qemu-esp32-baremetal` provisions exactly what the detection looks
   for.

No per-example `CC_*` env — detection is the single place that names the compiler.

**Verified.** `examples/qemu-esp32-baremetal/rust/talker` builds clean on this
host (riscv64 present, picked first → no regression), and a forced
`CC_riscv32imc_unknown_none_elf=riscv-none-elf-gcc` clean build (simulating a
documented-provisioned-only host, since riscv64 lives in both `/usr/bin` and
`/bin` here and can't be PATH-hidden without breaking coreutils) compiles the
zpico C shim + the whole example — proving the provisioned toolchain works
end-to-end.

The other `packages = []` boards are NOT the same hole: `native`/`posix` take
their daemon from the RMW axis (no cross toolchain), and `zephyr` is west-built.
`qemu-esp32-baremetal` was the only genuine miss.

Fixed in `packages/rmw/zenoh/nros-zpico-build/src/{lib,runner}.rs` + `nros-sdk-index.toml`.
