---
id: 1753
title: "The rv-virt-threadx typed-entry carrier image fails to link: libc `__libc_fini_array` relocation out of range"
status: resolved
type: bug
area: [build, cmake, threadx]
severity: low
found: 2026-10-08
related: [issue-1742, issue-1795, phase-370]
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

## Resolution (2026-10-11)

### Cause: a C++ static object in a crt-free image

The reproduction showed four symbols, not three. With the template's
`pkg_c_talker`, `DEPLOY threadx` and the riscv64-threadx toolchain, the link
failed on `__dso_handle` as well as `__libc_fini`, `__fini_array_start` and
`__fini_array_end`.

The generated carrier TU holds `static ::rclcpp::Node __nros_node_0;`. A
static object with a destructor registers it through
`__cxa_atexit(dtor, obj, &__dso_handle)`, and a `-nostartfiles` image has
no crt to supply either symbol:

- `__dso_handle` comes from `crtbegin.o`, which is not linked.
- libc's `__cxa_atexit` pulls in newlib's exit machinery.
  `__call_atexit.o`'s `register_fini` holds a weak reference to
  `__libc_fini`, and `fini.o`'s `__libc_fini_array` reads
  `__fini_array_start/end`.
- The board's `link.lds` placed `.init_array` only, so the linker resolved
  those symbols near address 0. On RISC-V medany, `.text` sits at
  0x80000000, about 2 GiB away, which is out of `R_RISCV_PCREL_HI20` range.

The `nano_ros_entry()` road never met this: an entry-road C image has no
C++ static object. Phase-370 W4 had already solved the same thing on
FreeRTOS. `freertos_hooks.c` defined no-op `__dso_handle`, `__cxa_atexit`,
`__cxa_finalize` and `_fini`, which are correct for an image that never
exits. The ThreadX family had none of these.

### Fix

- **`packages/boards/nros-board-common/c/cxx_static_abi.c`** is now the one
  definition. FreeRTOS's copy moved here. It is compiled by every
  never-exiting board build: rv-virt-threadx, mps2-an385-freertos,
  mps3-an536-freertos, s32z270-freertos, and `nros-board-freertos/build.rs`.
  The hosted-crt builds (freertos-posix, threadx-linux, NuttX) do not
  compile it, because their libc defines all four.
- **`link.lds`** (rv-virt-threadx) and **`nros-freertos-cortex-m.ld`**
  (shared by the three FreeRTOS cross boards) now also place
  `.preinit_array` and `.fini_array` with `PROVIDE`d bounds, next to
  `.init_array`. An image that does reach newlib's exit path still links.
- **The gate `check-crt-free-runtime-symbols`** runs on the fast line and
  enforces two rules:
  - **R1:** a tracked linker script that owns `.init_array` owns all three
    arrays and all six bounds.
  - **R2:** every board build that compiles `nros_rtos_run_components.c` is
    ruled NEVER_EXITS (it must compile the stubs) or HOSTED_CRT (it must
    not). An unruled build or a stale ruling fails.
  Each part of the fix was reverted on its own: the riscv `link.lds`, the
  Cortex-M fragment, the rv-virt cmake list, and the FreeRTOS `build.rs`.
  Each revert gave rc=1. The clean tree gives rc=0.

### Measured

- **The carrier image** (the reproduction above) now links. Booted in QEMU
  with user networking against `rmw_zenohd` on the baked port 7553, it runs
  the `.init_array` constructors, brings NetX up, and publishes
  `[pkg_c_talker] sent: 0` … `23`. The ELF no longer contains
  `__libc_fini_array`, `register_fini` or `__call_exitprocs`, and it defines
  `__dso_handle`, `__cxa_atexit` and `_fini` from the stub file.
- **`just threadx_riscv64 test-all`**: 4/4 `threadx_riscv64_qemu` passed
  (Cyclone C/C++/Rust two-QEMU pubsub, per-thread errno). 6/6 `rtos_e2e`
  ThreadxRiscv64 passed (zenoh pubsub and service, C/C++/Rust). The tested
  ELFs were built after the linker-script edit and carry `.preinit_array`
  and `.fini_array`.
- **`just test freertos`**: 10/10 passed (rtos_e2e pubsub, service and action
  for C/C++/Rust on mps2-an385-freertos, plus `realtime_tiers`). The moved
  stubs link there.

### Not covered

No lane builds an RTOS carrier image, which is why this and issue 1742 were
found only by hand. Filed as issue 1795.
