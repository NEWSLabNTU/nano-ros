/*
 * Linker script for QEMU MPS2-AN385 (Cortex-M3) with FreeRTOS
 *
 * Memory Map:
 *   0x0000_0000 - 0x003F_FFFF : Flash (4 MB) — code + read-only data
 *   0x2000_0000 - 0x203F_FFFF : SRAM  (4 MB) — data + bss + heap + stacks
 *
 * FreeRTOS heap is managed by heap_4.c using ucHeap[] (configTOTAL_HEAP_SIZE).
 * The main stack pointer is set to the top of RAM.
 */

MEMORY
{
    FLASH (rx)  : ORIGIN = 0x00000000, LENGTH = 4M
    RAM   (rwx) : ORIGIN = 0x20000000, LENGTH = 4M
}

/* Initial stack pointer — top of RAM */
_estack = ORIGIN(RAM) + LENGTH(RAM);

/* Minimum main stack size (used before FreeRTOS scheduler starts) */
_Min_Stack_Size = 0x2000;  /* 8 KB */

ENTRY(Reset_Handler)

SECTIONS
{
    /* Vector table + code */
    .text :
    {
        . = ALIGN(4);
        KEEP(*(.isr_vector))
        *(.text)
        *(.text*)
        *(.rodata)
        *(.rodata*)
        . = ALIGN(4);
    } > FLASH

    /* ARM exception unwinding tables (.ARM.extab) — referenced by
     * .ARM.exidx, placed before it in FLASH. Phase 97.1.linker:
     * DDS emits extab entries for its panic unwind paths;
     * without explicit placement they fall to the default
     * read-only output and overlap .data on bare-metal Cortex-M.
     */
    .ARM.extab :
    {
        *(.ARM.extab* .gnu.linkonce.armextab.*)
        . = ALIGN(4);
    } > FLASH

    /* ARM exception unwinding (placed in FLASH after .text) */
    .ARM.exidx :
    {
        __exidx_start = .;
        *(.ARM.exidx*)
        __exidx_end = .;
        . = ALIGN(4);
    } > FLASH

    /* DWARF CFI (.eh_frame / .eh_frame_hdr) — newer rustc/rust-lld emit these
     * (and lld synthesizes .eh_frame_hdr under --eh-frame-hdr) for the same DDS
     * panic-unwind paths as .ARM.extab. ARM bare-metal unwinds via exidx/extab,
     * so eh_frame is unused at runtime, but it MUST get a FLASH load address —
     * left unplaced it falls to the default output and overlaps .data/RAM
     * (`rust-lld: section .eh_frame_hdr load address range overlaps with .data`).
     */
    .eh_frame_hdr :
    {
        *(.eh_frame_hdr)
        . = ALIGN(4);
    } > FLASH

    /* Baked boot config (phase-266 / RFC-0045, issue #108) — `nros::main!()`
     * emits the node name + deploy overlay into `.nros_boot_config` via
     * `#[link_section]`. Read-only; MUST get a FLASH load address or it falls to
     * the default output and overlaps .data/RAM — exactly the class fixed above
     * for .eh_frame_hdr (`section .nros_boot_config load address range overlaps
     * with .data`). cortex-m-rt's link.x absorbs it for the bare-metal/RTIC
     * entries; this hand FreeRTOS script must place it explicitly. */
    .nros_boot_config :
    {
        KEEP(*(.nros_boot_config))
        KEEP(*(.nros_boot_config*))
        . = ALIGN(4);
    } > FLASH

    .eh_frame :
    {
        *(.eh_frame)
        . = ALIGN(4);
        _etext = .;
    } > FLASH

    /* Initialized data — loaded from flash, copied to RAM at startup */
    .data : AT(_etext)
    {
        . = ALIGN(4);
        _sdata = .;
        *(.data)
        *(.data*)
        . = ALIGN(8);
        __tls_base = .;
        *(.tdata)
        *(.tdata*)
        *(.tbss)
        *(.tbss*)
        . = ALIGN(4);
        _edata = .;
    } > RAM

    /* Zero-initialized data */
    .bss :
    {
        . = ALIGN(4);
        _sbss = .;
        __bss_start__ = _sbss;
        *(.bss)
        *(.bss*)
        *(COMMON)
        . = ALIGN(4);
        _ebss = .;
        __bss_end__ = _ebss;
        end = .;           /* newlib _sbrk heap start */
        __end__ = .;
    } > RAM

    /* Ensure enough room for the main stack */
    ._check_stack :
    {
        . = ALIGN(4);
        . = . + _Min_Stack_Size;
        . = ALIGN(4);
    } > RAM
}
