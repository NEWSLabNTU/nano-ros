/* FreeRTOS kernel configuration -- s32z270-freertos-rtd (issue 2632).
 *
 * The NXP RTD variant of the S32Z270 RTU board: the same board facts as
 * `../config/FreeRTOSConfig.h`, run on NXP's GCC/ARM_CR52_GIC port from a
 * provisioned FreeRTOS_S32ZE kernel instead of the in-tree GCC/ARM_CRx_No_GIC.
 *
 * That port owns the GIC and the tick, so the CRx_No_GIC tick seam and EOI
 * placeholder below are inert there. What it changes for nano-ros: its FP
 * context is per task and is set up only by `xTaskCreateFpu` /
 * `xTaskCreateStaticFpu`, so nano-ros's own task creation must use them
 * (nros-board-freertos/c/nros_freertos_task_create.h). */
#define nrosconfigTASK_CREATE_FPU 1

#include "../config/FreeRTOSConfig.h"
