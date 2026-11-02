---
id: 2211
title: "Every mps2/an385 Zephyr Rust west row refuses to configure: the `zephyr` board states an x86_64 rust_target"
status: open
type: bug
severity: medium
area: [build, zephyr]
related: [phase-521, 2078]
found: 2026-11-02
---

## Symptom

`NROS_ZEPHYR_FIXTURE_FILTER='build-cortex-m-rust-talker-serial-zenoh' just
zephyr build-fixtures` (and the TCP twin `build-cortex-m-rust-talker-zenoh`)
stop at configure:

```
CMake Error at cmake/NanoRosBoardFacts.cmake:354 (message):
  nano-ros: Error: board `zephyr` states `[board.zephyr] rust_target =
  "x86_64-unknown-none"` (packages/boards/zephyr/nros-board.toml), but
  zephyr-lang-rust compiles this image for `thumbv7m-none-eabi` (its
  `_rust_map_target`, from this image's Kconfig — on Arm the CPU and
  `CONFIG_FP_HARDABI`).
Call Stack (most recent call first):
  zephyr/CMakeLists.txt:125 (nros_check_zephyr_rust_target)
```

Seen on a worktree at `c25eca840e` (origin/main of 2026-11-02). The leaf
(`examples/zephyr/rust/talker`) names `board = "zephyr"`, the family, and the
row's west `-b mps2/an385` should resolve the concrete board through the
catalog; the check instead reads the family descriptor's native_sim triple.

## Consequence

The two link witnesses for issue 2078 (`tests/zephyr_link_layout.rs`) cannot
be rebuilt, so they read whatever ELF a tree last built. phase-521 W6 needed
the serial row to compile zenoh-pico's Zephyr serial link after retiring its
polled receive path, and built the C talker with the same conf stack instead
(`examples/zephyr/c/talker`, which configures, compiles `uart_zephyr.c` and
links).

## Next

Find where the Zephyr configure picks the board for the rust-target check
(`nros_check_zephyr_rust_target`, `zephyr/CMakeLists.txt:125`) and make it
resolve `-b` through the board catalog the way the board facts do for the C
lane, or have the row name the concrete board.
