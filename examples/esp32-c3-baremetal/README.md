# examples/esp32-c3-baremetal — bare-metal ESP32-C3 (esp-hal) on QEMU

> **Dormant (issue 1525).** ESP32 support is parked: the code below is kept in
> the tree for future use, but nothing builds, tests or provisions it — the
> `esp32` just module is not mounted, `nros setup` no longer carries the ESP32
> board or tools, the CI job and fixture rows are gone, and `nros new --platform`
> does not offer `esp32`. The rest of this page records how the path worked, for
> whoever revives it.

Pure-Rust `esp-hal` examples (no ESP-IDF), riscv32, OpenETH networking under
the Espressif QEMU fork. Just module: **`esp32`** (`just/esp32.just`).

## Prerequisites

```sh
source ./activate.sh
just setup esp32              # nros setup esp32-c3-baremetal (+ optional esp32-qemu tool)
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

### Why C/C++ is absent, written down (issue 1512)

It is an AUTHORED decision, not a missing port, and it is enforced in code:
`PlatformKind::Esp32::cmake_deploy()` returns `None`, so `nros ws leaf-system`
derives no `NANO_ROS_PLATFORM` for a C/C++ leaf in this tree and one cannot
configure. The reason is the tree's identity — this is the **no-IDF** path
(`esp-hal`, pure Rust HAL); C/C++ on the same silicon is the ESP-IDF component
road (phase-139), which would belong under a sibling `esp32-idf/` dir.

Two things exist that look like the opposite and are not:
`cmake/platform/nano-ros-baremetal.cmake` serves this board's platform axis, and
`cmake/board/nano-ros-board-esp32-c3-baremetal.cmake` exists — its own header
says it is "the in-tree shim for non-IDF parents (rare — kept for symmetry with
the other board overlays)". Those are for a C parent build that states its own
platform, not for a leaf here.

The sibling bare-metal family took the other answer:
`examples/mps2-an385-baremetal/c/talker/` is a C application rooted in a cargo
image, because on THAT board the Rust port is the only startup there is. Nothing
about it is esp32-specific; what blocks the same shape here is the authored
decision above, not the toolchain.
