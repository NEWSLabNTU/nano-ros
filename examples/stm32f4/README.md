# examples/stm32f4 — STM32F4 bare metal (real hardware)

Rust examples for STM32F429 (NUCLEO-F429ZI): RTIC and Embassy variants.
**No QEMU lane** — these run on real hardware. Just module: **`stm32f4`**
(`just/stm32f4.just`).

## Prerequisites

```sh
source ./activate.sh
just stm32f4 setup            # = nros setup stm32f4 --rmw zenoh
```

## RMW selection

Board-driven, no knob: `nros-board-stm32f4` (plain) /
`nros-board-rtic-stm32f4` (RTIC variants) select zenoh — the only backend on
this platform.

## Build & flash one example

```sh
just stm32f4 build-fixtures   # builds the example set (skips if arm-none-eabi-gcc missing)

# or a single example:
cd examples/stm32f4/rust/talker-rtic
cargo build --release --target thumbv7em-none-eabihf
# flash with probe-rs / openocd (no run recipe — real hardware only)
```

## Cases (rust only — no bare-metal C/C++ harness exists)

| Role | variants |
| --- | --- |
| talker | plain, `-rtic`, `-embassy` |
| listener | `-rtic`, `-embassy` |
| service-server / service-client | `-rtic` |
| action-server / action-client | `-rtic` |

`talker_node_pkg` is a board-agnostic Node package shared by Entry pkgs
(excluded from the host workspace — it cross-builds for
`thumbv7em-none-eabihf` only).

Coverage authority: [`examples/README.md`](../README.md).
