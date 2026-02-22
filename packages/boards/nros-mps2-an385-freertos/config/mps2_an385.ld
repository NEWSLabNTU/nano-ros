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
        _etext = .;
    } > FLASH

    /* Initialized data — loaded from flash, copied to RAM at startup */
    .data : AT(_etext)
    {
        . = ALIGN(4);
        _sdata = .;
        *(.data)
        *(.data*)
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
    } > RAM

    /* Ensure enough room for the main stack */
    ._check_stack :
    {
        . = ALIGN(4);
        . = . + _Min_Stack_Size;
        . = ALIGN(4);
    } > RAM
}
