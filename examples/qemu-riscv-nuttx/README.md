# examples/qemu-riscv-nuttx — NuttX on QEMU (riscv32) — partial platform

A **partial platform**: it ships only `c/talker`, built by its own riscv
toolchain/board lane — not the `qemu-arm-nuttx` build path. Just module:
**`nuttx`** (`just/nuttx.just`, recipe `build-riscv-c`).

## Prerequisites

```sh
source ./activate.sh
nros setup qemu-riscv-nuttx    # riscv-none-elf toolchain + NuttX sources
```

## RMW selection

Board-driven, zenoh only.

## Build

```sh
just nuttx build-riscv-c       # builds c/talker via the riscv fixtures lane
```

There is no dedicated run recipe; the lane is exercised by `just nuttx test` /
CI fixtures.

## Cases

| Role | c | cpp | rust |
| --- | --- | --- | --- |
| talker | yes | – | – |

Known limitation: nros-c currently cannot compile for riscv32 (no 64-bit
atomics) — see issue #130.
