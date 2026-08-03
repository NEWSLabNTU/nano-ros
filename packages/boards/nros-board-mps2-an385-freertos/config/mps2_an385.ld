/*
 * Linker script for QEMU MPS2-AN385 (Cortex-M3) with FreeRTOS.
 *
 * phase-337 W5.e — the board's whole contribution is the memory map and the
 * initial stack pointer; the section layout is shared. See
 * `nros-board-freertos/config/nros-freertos-cortex-m.ld`.
 *
 * Memory map:
 *   0x0000_0000 - 0x003F_FFFF : Flash (4 MB) — code + read-only data
 *   0x2000_0000 - 0x203F_FFFF : SRAM  (4 MB) — data + bss + heap + stacks
 */

MEMORY
{
    FLASH (rx)  : ORIGIN = 0x00000000, LENGTH = 4M
    RAM   (rwx) : ORIGIN = 0x20000000, LENGTH = 4M
}

/* Initial stack pointer — top of RAM */
_estack = ORIGIN(RAM) + LENGTH(RAM);

INCLUDE nros-freertos-cortex-m.ld
