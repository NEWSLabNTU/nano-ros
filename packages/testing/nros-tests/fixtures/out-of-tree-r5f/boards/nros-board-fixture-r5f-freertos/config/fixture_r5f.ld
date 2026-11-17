/*
 * Linker script: fixture-r5f-freertos. The ZynqMP RPU's tightly coupled
 * memory is too small for an image with a network stack, so the image runs
 * from DDR at 0x0010_0000; the section layout is the FreeRTOS family's
 * (nros-freertos-cortex-m.ld, which is profile-agnostic despite its name).
 */
MEMORY
{
    FLASH (rwx) : ORIGIN = 0x00100000, LENGTH = 8M
    RAM   (rwx) : ORIGIN = 0x00900000, LENGTH = 56M
}

_estack = ORIGIN(RAM) + LENGTH(RAM);

INCLUDE nros-freertos-cortex-m.ld
