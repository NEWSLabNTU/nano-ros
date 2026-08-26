/*
 * Linker script for QEMU MPS3-AN536 (dual Cortex-R52) with FreeRTOS.
 *
 * phase-385 W1 — the board contributes the memory map and the initial stack
 * pointer; the section layout is shared
 * (`nros-board-freertos/config/nros-freertos-cortex-m.ld` — despite the name
 * it is profile-agnostic: sections + .init_array KEEP, issue 0733).
 *
 * Memory map (measured from the model, `info mtree`):
 *
 *   0x2000_0000 - 0xDFFF_FFFF : DDR, one flat RAM region. There is no
 *       separate flash on this image, so FLASH and RAM below are two windows
 *       into the same DDR rather than distinct devices — the shared layout
 *       wants both names, and splitting them keeps `.text` (loaded at the
 *       ELF's LMA) away from the heap/stack window.
 *
 * The FLASH window is deliberately generous (16 MiB): a cyclonedds image with
 * the C++ runtime is multi-MiB, and DDR here is 3 GiB, so there is nothing to
 * economise against. RAM gets 48 MiB, well past the multi-MiB heap Cyclone
 * needs (the QEMU MPS2 cell budgets 3 MiB on a far smaller board).
 *
 * NOTE the `.bss (NOLOAD)` rule the shared layout carries: without it `.bss`
 * continues `.data`'s load region, QEMU computes one PT_LOAD whose p_memsz
 * spans the whole heap, and `load_elf` silently REJECTS the image — the
 * failure looked like a CPU that executes zeroed memory (ASI phase-372 W-loader
 * finding, issue 0475 class). Do not "simplify" it back.
 */

MEMORY
{
    FLASH (rwx) : ORIGIN = 0x20000000, LENGTH = 16M
    RAM   (rwx) : ORIGIN = 0x21000000, LENGTH = 48M
}

/* Initial stack pointer — top of RAM. Reset_Handler carves the IRQ/abort
 * mode stacks below this (see c/board_an536.c). */
_estack = ORIGIN(RAM) + LENGTH(RAM);

INCLUDE nros-freertos-cortex-m.ld
