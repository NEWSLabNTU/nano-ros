/*
 * TI linker command file: fixture-r5f-tiarmclang (phase-575 W3).
 *
 * tiarmlnk's counterpart of fixture_r5f.ld + the family's
 * nros-freertos-cortex-m.ld: the same memory map, the image running from DDR
 * at 0x0010_0000, and the symbols c/board_fixture_r5f.c's startup reads
 * (_estack, _etext, _sdata/_edata, _sbss/_ebss, the init-array bounds).
 * The image boots from the board's own Reset_Handler, not TI's _c_int00, and
 * FreeRTOS's heap_4 owns the heap, so the C library's stack and heap are 0.
 */
--entry_point=Reset_Handler
--stack_size=0
--heap_size=0

MEMORY
{
    FLASH (RWX) : origin = 0x00100000, length = 0x00800000
    RAM   (RWX) : origin = 0x00900000, length = 0x03800000
}

/* ORIGIN(RAM) + LENGTH(RAM), as in fixture_r5f.ld. */
_estack = 0x04100000;

SECTIONS
{
    .isr_vector        : {} > 0x00100000
    .text              : {} > FLASH
    .rodata            : {} > FLASH
    .preinit_array     : {} > FLASH, START(__preinit_array_start), END(__preinit_array_end)
    .init_array        : {} > FLASH, START(__init_array_start), END(__init_array_end)
    .fini_array        : {} > FLASH, START(__fini_array_start), END(__fini_array_end)
    .ARM.extab         : {} > FLASH
    .ARM.exidx         : {} > FLASH
    .nros_boot_config  : {} > FLASH
    .data              : {} load = FLASH, run = RAM, LOAD_START(_etext), RUN_START(_sdata), RUN_END(_edata)
    .bss               : {} > RAM, RUN_START(_sbss), RUN_END(_ebss)
}
