---
id: 1753
title: "The rv-virt-threadx typed-entry carrier image fails to link: libc `__libc_fini_array` relocation out of range"
status: open
type: bug
area: [build, cmake, threadx]
severity: low
found: 2026-10-08
related: [issue-1742]
---

## What was measured

Issue 1742 built a `nano_ros_node_register(... TYPED DEPLOY threadx)` carrier
image for `NANO_ROS_BOARD=rv-virt-threadx` (the C talker from
`examples/templates/multi-package-workspace/src/pkg_c_talker`, `DEPLOY`
changed to `threadx`, configured with `cmake/toolchain/riscv64-threadx.cmake`
and the board's own `nros-board-threadx-qemu-riscv64/config`).

With 1742's fix the `nros-c`/`nros-cpp` staticlibs now build with
`panic-platform` and compile. The FINAL link then fails:

```
rust-lld: error: .../riscv-none-elf/lib/rv64imafdc_zicsr/lp64d/libc.a(libc_a-fini.o):
  (function __libc_fini_array: .text.__libc_fini_array+0x4): relocation
  R_RISCV_PCREL_HI20 out of range: -524375 is not in [-524288, 524287];
  references '__fini_array_start'
```

(three such lines: `__fini_array_start`, `__fini_array_end`, `__libc_fini`).
The link line is the carrier's: `-T .../nros-board-threadx-qemu-riscv64/config/link.lds
--nmagic -Wl,--gc-sections -nostartfiles -u app_main ... -lc ... -lnosys -lgcc`.

## Why nobody saw it

Before issue 1742 this carrier never got this far: it applied no panic
policy, so the `nros-c` staticlib died with `#[panic_handler] function
required, but not found` (measured by deleting the carrier's
`nros_apply_panic_policy` call: the cargo command loses `panic-platform` and
rustc stops there). No `examples/fixtures.toml` row builds a ThreadX or
NuttX CARRIER — every in-tree ThreadX/NuttX C/C++ image goes through
`nano_ros_entry()` — so neither failure was on any lane.

## Not yet known

Whether `link.lds` simply does not place `.fini_array` (an orphan section
landing ~2 GiB from `.text`), or whether the carrier's link differs from the
`nano_ros_entry()` road's (the `workspace-c-threadx-riscv64` row) in a way
that pulls `__libc_fini_array` in at all. Compare the two link lines first.
