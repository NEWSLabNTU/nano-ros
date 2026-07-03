# examples/qemu-esp32-baremetal — bare-metal ESP32-C3 (esp-hal) on QEMU

Pure-Rust `esp-hal` examples (no ESP-IDF), riscv32, OpenETH networking under
the Espressif QEMU fork. Just module: **`esp32`** (`just/esp32.just`).

## Prerequisites

```sh
source ./activate.sh
just esp32 setup              # nros setup qemu-esp32-baremetal (+ optional esp32-qemu tool)
```

The build uses rustup + `-Z build-std` (no separate toolchain package); the
e2e tests need Espressif's QEMU fork (`esp32c3` machine), the build does not.

## RMW selection

Board-driven, no knob: `nros-board-esp32-qemu` selects zenoh (the only backend
on this platform).

## Build & run one example

```sh
just esp32 build-examples     # workspace lane
just esp32 build-qemu         # QEMU flash images
just esp32 zenohd &           # router on tcp port 7454
just esp32 talker             # boot talker image under qemu-system-riscv32
just esp32 listener           # peer in a second shell
```

Test lanes: `just esp32 test`, `test-basic`, `test-all`.

## Cases (rust only — C/C++ intentionally absent: this is the no-IDF path)

| Role | present |
| --- | --- |
| talker | yes |
| listener | yes |

`rust/dds/` is build support, not a case. See the
[coverage matrix](../README.md) for the platform's intentionally-empty cells.
