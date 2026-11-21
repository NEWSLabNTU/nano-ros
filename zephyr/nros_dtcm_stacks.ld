/*
 * Issue 2793 -- nano-ros's stacks in the devicetree's "DTCM" region, in an
 * output section of the module's own. Included by zephyr/CMakeLists.txt
 * through `zephyr_linker_sources(SECTIONS ...)`, only when
 * CONFIG_NROS_ZEPHYR_STACKS_IN_DTCM or CONFIG_NROS_ZEPHYR_SCHED_TASK_STACKS_IN_DTCM
 * is on.
 *
 * The stacks used to be placed with `section("DTCM")`, into the output section
 * Zephyr generates for the region:
 *
 *     DTCM (NOLOAD) : { __DTCM_start = .; KEEP(*(DTCM)) KEEP(*(DTCM.*)) __DTCM_end = .; } > DTCM
 *
 * Everything it collects is KEEP, so --gc-sections could not drop an unused
 * pool there: an image that declares no sched task carried the whole sched
 * task pool (65,792 B on the nucleo-f767zi, half its DTCM), which sram0 drops.
 * Here nothing is KEEP, and each array is its own input section
 * (`.nros_dtcm_stacks.<array>`), because the linker drops whole input sections:
 * one section holding every array would be kept whole by the one that is used.
 *
 * `> DTCM` names the region, so a devicetree without one fails the LINK, and
 * the shim's BUILD_ASSERT refuses it earlier, by name. NOLOAD: stacks are
 * uninitialised.
 */
.nros_dtcm_stacks (NOLOAD) :
{
    *(.nros_dtcm_stacks.*)
} > DTCM
