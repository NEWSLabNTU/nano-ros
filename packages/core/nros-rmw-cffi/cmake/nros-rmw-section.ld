/* Phase 128.A.1 — anchor the .nros_rmw_init linker section so the
 * runtime walker (`nros_rmw_cffi_walk_init_section`) can iterate every
 * backend init entry. The fragment defines the `__start_/__stop_*`
 * encapsulation symbols explicitly because lld's `--gc-sections` +
 * start-stop-gc combination can drop them otherwise, and KEEPs the
 * section content so embedded targets with `gc-sections` don't strip
 * the entries themselves.
 *
 * Linkage layout:
 *   ELF hosts (Linux/BSD): cargo build.rs emits
 *     `-Wl,-T,<this-file>` via `cargo:rustc-link-arg`, INSERT'd
 *     before .rodata so the section lands in a regular data segment.
 *   Bare-metal / RTOS: board crate's memory.x INCLUDEs this file
 *     from its linker script directory. Typical placement:
 *         .nros_rmw_init : ALIGN(4) {
 *             *(.nros_rmw_init)
 *         } > FLASH
 *     The version below uses INSERT for hosted ELF; bare-metal users
 *     copy-paste the SECTIONS body into their own script.
 *
 * Mach-O targets do not use this fragment — they rely on
 * `__attribute__((section("__DATA,__nros_rmw_init")))` plus the
 * native `__section_start_/__section_stop_` aliases.
 */

SECTIONS {
    .nros_rmw_init : {
        __start_nros_rmw_init = .;
        KEEP(*(.nros_rmw_init))
        __stop_nros_rmw_init = .;
    }
} INSERT BEFORE .rodata;
