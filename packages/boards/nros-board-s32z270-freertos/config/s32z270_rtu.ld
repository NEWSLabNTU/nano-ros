/*
 * Linker script for NXP S32Z270 RTU0 (Cortex-R52 lock-step) with FreeRTOS.
 *
 * phase-372 W2 — the board's contribution is the memory map and the initial
 * stack pointer; the section layout is shared
 * (`nros-board-freertos/config/nros-freertos-cortex-m.ld` — despite the
 * name it is profile-agnostic: sections + .init_array KEEP, issue 0733).
 *
 * Memory map (public: S32Z2 RM / Zephyr nxp_s32z27x dts; RTU0 SRAM base
 * 0x3178_0000, mirrored at 0x3578_0000 for RTU1):
 *
 *   0x3178_0000 - 0x317F_FFFF : SRAM_NC (512 KiB) — NON-CACHEABLE window
 *       (matches the consumer MPU tables' non-cacheable region: the NXP
 *       NETC buffer-descriptor rings and DMA data buffers MUST live here
 *       or the D-cache reads stale descriptors and RX stalls after one
 *       ring — the hardware lesson recorded in phase-372's findings).
 *   0x3180_0000 - 0x31EF_FFFF : SRAM (7 MiB) — code + data + bss + heap +
 *       stacks. Cyclone needs multi-MiB heap (the QEMU cell budgets 3 MiB;
 *       the Zephyr S32Z bring-up needed the 7 MiB region).
 *
 * FIRST-CUT CAVEAT (deliberate): the exact split between cacheable and
 * non-cacheable is a function of the consumer's MPU tables (S32 Config
 * Tools output). This map is self-consistent and link-complete; align it
 * with the consumer's `s32ct_config` MPU regions during phase-372 W5
 * hardware bring-up before first boot.
 */

MEMORY
{
    SRAM_NC (rw)  : ORIGIN = 0x31780000, LENGTH = 512K
    FLASH   (rwx) : ORIGIN = 0x31800000, LENGTH = 4M
    RAM     (rwx) : ORIGIN = 0x31C00000, LENGTH = 3584K
}

/* Initial stack pointer — top of RAM */
_estack = ORIGIN(RAM) + LENGTH(RAM);

/* Non-cacheable placements: NETC buffer descriptors + DMA buffers. The
 * consumer's RTD glue tags them with these input sections (ASI already
 * does via its AUTOSAR MemMap shim). NOLOAD: cleared by driver init. */
SECTIONS
{
    .nc_buffers (NOLOAD) :
    {
        . = ALIGN(64);
        *(.netc_bd)
        *(.netc_buffers)
        *(.non_cacheable)
        . = ALIGN(64);
    } > SRAM_NC
}

INCLUDE nros-freertos-cortex-m.ld
