---
id: 2077
title: "A typed entry's .nros_boot_config is an orphan section, and on the S32K344 the linker puts it in IVT_HEADER"
status: open
type: bug
area: [zephyr, boards, build]
severity: medium
found: 2026-10-26
related: [rfc-0045, phase-512, 1799, 0902]
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

## Closes when

nano-ros's Zephyr module places `.nros_boot_config` itself (a RODATA
`zephyr_linker_sources` fragment, or an iterable section), so no board
depends on the orphan placement. A build of a typed entry for
`mr_canhubk3/s32k344` links without a local fragment; a build lane for that
board, or a link-map check that the section is in rodata, holds it.
