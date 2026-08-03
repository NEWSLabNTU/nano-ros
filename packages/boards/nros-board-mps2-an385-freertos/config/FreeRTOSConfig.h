/* FreeRTOS kernel configuration — QEMU MPS2-AN385 (Cortex-M3).
 * phase-337 W5.a: the board supplies the two board facts; the rest is shared. */
#define NROS_BOARD_CPU_CLOCK_HZ 25000000 /* QEMU MPS2-AN385 default */
#define NROS_BOARD_PRIO_BITS    3        /* 3 NVIC priority bits = 8 levels */
#include "../../nros-board-freertos/config/FreeRTOSConfig.h"
