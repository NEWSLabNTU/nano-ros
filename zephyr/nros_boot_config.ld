/*
 * RFC-0045 -- the baked boot config (`NROS_BOOT_CONFIG`, magic "NRBC"), in
 * rodata. Included by zephyr/CMakeLists.txt through
 * `zephyr_linker_sources(RODATA ...)`.
 *
 * Every typed entry emits its blob into `.nros_boot_config` (the C/C++ entry
 * pack's `__attribute__((section(".nros_boot_config")))`, `nros::main!`'s
 * `#[link_section]`). Zephyr's linker scripts name no such section, so it was
 * an ORPHAN, placed wherever the linker chose. On `mr_canhubk3/s32k344` that
 * was the 0x100-byte IVT_HEADER region ahead of the vector table: "region
 * `IVT_HEADER' overflowed by 268 bytes" (issue 2077). Named here, it is
 * read-only data in the ROM region on every board.
 *
 * Readers find the blob by its symbol (`NROS_BOOT_CONFIG`), and a post-link
 * tool by its magic; neither depends on the output section's name.
 */
KEEP(*(.nros_boot_config))
KEEP(*(.nros_boot_config*))
