---
id: 2084
title: "CONFIG_NROS_ZEPHYR_STACKS_IN_DTCM moves the task stacks but not the tier stack pool, so the S32K344 serial Rust level image overflows RAM"
status: resolved
type: limitation
area: [zephyr, memory, boards]
severity: medium
found: 2026-10-26
resolved: 2026-10-27
resolved_in: "PR #2708 (2026-10-27)"
related: [phase-512, 2065, 2078, 1232]
---

**Home: [phase-512](../../roadmap/phase-512-on-hardware-measurements.md)** (batch A, H9).

## Measured

The phase-512 batch A H9 image: the `realtime-rust` `derived_bringup` Rust
image as a local `[image.mcu_derived]` (Zephyr, `prj-zenoh.conf`, locator
`serial/uart@40330000#baudrate=921600`) for `mr_canhubk3/s32k344`, with a
board conf that sets

```
CONFIG_NROS_ZEPHYR_TIER_STACK_SIZE=65536
CONFIG_NROS_ZEPHYR_MAX_TIERS=1
CONFIG_NROS_ZEPHYR_TASK_STACK_SIZE=8192
CONFIG_NROS_ZEPHYR_STACKS_IN_DTCM=y
CONFIG_MAIN_STACK_SIZE=32768
```

The 64 KiB slot is what the level needs: issue 2065 measured its setup peak
at 46,868 B on this board (release), well past the 16 KiB default.

The derived entry declares no level stack (`__NROS_TIER_TASK_MEMORY` holds a
null stack), so the level is spawned through `nros_zephyr_tier_task_create`
on a slot of the shim's pool, `nros_tier_stacks`
(`zephyr/nros_platform_zephyr_shims.c`, `K_THREAD_STACK_ARRAY_DEFINE`).
`CONFIG_NROS_ZEPHYR_STACKS_IN_DTCM` places only `nros_thread_stacks` in DTCM.
The tier pool stays in SRAM, and on the S32K344 (320 KiB SRAM) the link fails:

```
region `RAM' overflowed by 30028 bytes
```

It links only with a local change, not committed, that defines the tier
pool in DTCM under the same knob:

```c
#if defined(NROS_ZEPHYR_STACKS_IN_DTCM) && NROS_ZEPHYR_STACKS_IN_DTCM
Z_KERNEL_STACK_ARRAY_DEFINE_IN(nros_tier_stacks, NROS_ZEPHYR_MAX_TIERS, NROS_ZEPHYR_TIER_STACK_SIZE,
                               __attribute__((section("DTCM"))));
#else
K_THREAD_STACK_ARRAY_DEFINE(nros_tier_stacks, NROS_ZEPHYR_MAX_TIERS, NROS_ZEPHYR_TIER_STACK_SIZE);
#endif
```

The memory map of that linked image (nano-ros with the 2077/2078 fixes plus
the local change):

| region | used | size |
| --- | ---: | ---: |
| RAM | 292,044 B (89.12 %) | 327,680 B |
| DTCM | 90,624 B (69.14 %) | 131,072 B |

DTCM holds exactly the two pools: `nros_tier_stacks` at `0x20000000`,
65,664 B (`0x10080`, one 64 KiB slot plus the reserved area), and
`nros_thread_stacks` at `0x20010080`, 24,960 B (`0x6180`, three 8 KiB task
stacks). Without the change the tier pool returns to RAM:
292,044 + 65,664 = 357,708 B against 327,680 B, the 30,028 B overflow above.
DTCM has room for it (40,448 B free with it placed there).

The image, its build log, config and the local diff are kept with the
island's run archive for 2026-10-26 (`board-batch/rust-frame/local.diff`,
`i2077-2078/rust-serial/`).

## Current state

- `CONFIG_NROS_ZEPHYR_STACKS_IN_DTCM`'s help says "the nano-ros task stack
  array"; the tier pool is a second stack array it does not name, and nothing
  says so.
- The same holds for a level stack an entry declares itself (issue 1232's
  `TierTaskMemory` / `K_THREAD_STACK_DEFINE` statics): their placement is the
  default, SRAM.
- No in-tree image covers this board with a Rust level image, so no lane
  sees the overflow. The H9 image and its board conf exist only in the
  archive.

## Direction

Not decided here. Candidates: put `nros_tier_stacks` under the existing knob
(the local change above), or give level stacks their own knob, so an image
whose DTCM is already claimed (the safety island places its trace buffer
there) can keep them in SRAM. An entry-declared level stack should follow the
same rule as the pool.

## Resolution

`CONFIG_NROS_ZEPHYR_STACKS_IN_DTCM` now means all of nano-ros's stacks. A new
knob, `CONFIG_NROS_ZEPHYR_TIER_STACKS_IN_DTCM`, defaults to it and places the
tier/level stacks in the devicetree's "DTCM" region:

- the shim's pool, `nros_tier_stacks` (`zephyr/nros_platform_zephyr_shims.c`,
  `Z_KERNEL_STACK_ARRAY_DEFINE_IN(..., section("DTCM"))`), which a level whose
  entry declares no stack runs on (every Rust `nros::main!` derived level);
- a generated C/C++ entry's declared level stack
  (`NROS_TIER_TASK_MEMORY_DEFINE`, `<rclc/detail/tier_task_memory_zephyr.h>`).

An image whose DTCM is already claimed keeps level stacks in SRAM by turning
the tier knob off. Either knob on a devicetree with no okay
`zephyr,memory-region = "DTCM"` node is now a compile error (a `BUILD_ASSERT`
in the shim). Before, the section was an orphan the linker placed anywhere,
and the Kconfig help's "link error" did not hold. DTCM placement is refused
with `CONFIG_USERSPACE`: a user-capable stack must live where the kernel's
object tracking finds it.

Not covered: a Rust entry's own `TierTaskMemory` statics (a level that
declares `stack_bytes` on the Rust road) keep the default section. The Kconfig
help says so.

Held by a new west row, `build-cortex-m-c-tier-stacks-dtcm-zenoh`
(`packages/testing/nros-tests/bins/tier-stacks-dtcm-zephyr-mps2`): mps2_an385
with a "DTCM" region carved from SRAM by the fixture's `app.overlay`, one
level on the pool and one on a declared stack.
`tests/zephyr_link_layout.rs::tier_stack_pool_follows_stacks_in_dtcm`
requires `nros_tier_stacks`, `__nros_tier_stack_1` and `nros_thread_stacks` in
the DTCM output section. Without the shim change it fails: "`nros_tier_stacks`
at 0x20016dc0 is in `noinit`, not in DTCM". In QEMU the image prints
"level `pool` stack in DTCM" and "level `declared` stack in DTCM". The same
app without the overlay stops at the `BUILD_ASSERT`.

Verified on the MR-CANHUBK344 on 2026-10-27. The H9 image (`derived_bringup`
Rust, serial, one 64 KiB level slot) links with no local source change. The
image entry, board conf and board selection were still local config. The
memory map: RAM 292,044 B (89.12 %), DTCM 90,624 B (69.14 %),
`nros_tier_stacks` at `0x20000000` (`0x10080`), `nros_thread_stacks` at
`0x20010080`. These are the figures the local diff gave. The image boots, and
through the serial gateway `ros2 node list` shows `/control_node`,
`/telem_node` and `/mcu_derived_entry`, and `ros2 topic echo --once /telem`
gets a sample.

## Closes when

The H9 image (`derived_bringup` Rust, serial, `mr_canhubk3/s32k344`, a 64 KiB
level stack) links from `main` with no local source change, and a build row or
a link-layout check holds the tier stack's placement.
