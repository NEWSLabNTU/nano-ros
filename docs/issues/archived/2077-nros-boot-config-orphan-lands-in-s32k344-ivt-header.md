---
id: 2077
title: "A typed entry's .nros_boot_config is an orphan section, and on the S32K344 the linker puts it in IVT_HEADER"
status: resolved
type: bug
area: [zephyr, boards, build]
severity: medium
found: 2026-10-26
related: [rfc-0045, phase-512, 1799, 0902]
resolved: 2026-10-26
resolved_in: "PR i2077-2078 (2026-10-26)"
---

## Measured

The phase-512 batch A run on the MR-CANHUBK344 (`mr_canhubk3/s32k344`), on
nano-ros 9f2d7b4bb3. Every image built from a typed C entry
(`examples/zephyr/c/talker`, `examples/zephyr/c/action-server`) and the Rust
`realtime-rust` `derived_bringup` image failed to link:

```
region `IVT_HEADER' overflowed by 268 bytes
```

The LMA also overlapped `rom_start`. `.nros_boot_config` (RFC-0045, the one
embedded bake site, `KEEP` in the linker script) is not named by any Zephyr
linker fragment nano-ros contributes. As an orphan section it goes to the
first region the linker picks, and on this board that is the 0x100-byte
`IVT_HEADER` ahead of the vector table.

It was worked around locally, not committed, with a RODATA fragment:

```
zephyr_linker_sources(RODATA boot_config_rodata.ld)
/* boot_config_rodata.ld */
KEEP(*(.nros_boot_config))
KEEP(*(.nros_boot_config*))
```

The diff is in the batch archive (`h7/local-workaround.diff`, `rust-frame/`).

## Resolution

The nano-ros Zephyr module places the section itself:
`zephyr/CMakeLists.txt` adds `zephyr/nros_boot_config.ld`
(`KEEP(*(.nros_boot_config))`, `KEEP(*(.nros_boot_config*))`) with
`zephyr_linker_sources(RODATA ...)`, so the blob is read-only data in the ROM
region on every board. Readers find it by its symbol (`NROS_BOOT_CONFIG`) and a
post-link tool by its magic, so nothing depended on the output section's name.
native_sim keeps working (the blob lands in its `rodata` section).

Verified on the MR-CANHUBK344 on 2026-10-26: `examples/zephyr/c/talker` for
`mr_canhubk3/s32k344` (serial transport) links with no local fragment, boots,
and `ros2 topic echo /chatter` receives it. The `derived_bringup` Rust image
from batch A also links (see 2078).

Held by `tests/zephyr_link_layout.rs::c_typed_entry_boot_config_is_in_rodata`,
which reads the `build-cortex-m-c-talker-zenoh` ELF (mps2/an385) and requires
`NROS_BOOT_CONFIG` in the `rodata` output section and no `.nros_boot_config`
output section. With the module line removed it fails: the orphan is its own
output section there.

## Closes when

nano-ros's Zephyr module places `.nros_boot_config` itself (a RODATA
`zephyr_linker_sources` fragment, or an iterable section), so no board
depends on the orphan placement. A build of a typed entry for
`mr_canhubk3/s32k344` links without a local fragment; a build lane for that
board, or a link-map check that the section is in rodata, holds it.
