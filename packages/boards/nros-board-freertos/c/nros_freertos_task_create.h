/*
 * The task creators nano-ros's FreeRTOS glue calls -- issue 2632.
 *
 * NXP's GCC/ARM_CR52_GIC port (the S32Z270 RTD distribution) disables
 * FPEXC.EN per task and re-enables it lazily in its undefined-instruction
 * handler, which finds the task's FP save area through TLS slot 0. Only the
 * port's `xTaskCreateFpu` / `xTaskCreateStaticFpu` set that slot up, so a task
 * that may touch floating point (every DDS and executor task) must be created
 * with them. The upstream ports have no such variants.
 *
 * A board whose port needs them says so in its FreeRTOSConfig.h with
 * `#define nrosconfigTASK_CREATE_FPU 1` (the `s32z270-freertos-rtd` board's
 * `config-rtd/FreeRTOSConfig.h`); every other board gets the plain creators.
 * Include after "task.h".
 */
#ifndef NROS_FREERTOS_TASK_CREATE_H
#define NROS_FREERTOS_TASK_CREATE_H

#if defined(nrosconfigTASK_CREATE_FPU) && (nrosconfigTASK_CREATE_FPU == 1)
/* Defined in the NXP port's port.c; its portmacro.h declares only the dynamic
 * one. Signature as the port defines it (proven by ASI's pthread shim). */
extern TaskHandle_t xTaskCreateStaticFpu(TaskFunction_t pxTaskCode, const char *pcName,
                                         uint32_t ulStackDepth, void *pvParameters,
                                         UBaseType_t uxPriority, StackType_t *puxStackBuffer,
                                         StaticTask_t *pxTaskBuffer);
#define NROS_FREERTOS_TASK_CREATE xTaskCreateFpu
#define NROS_FREERTOS_TASK_CREATE_STATIC xTaskCreateStaticFpu
#else
#define NROS_FREERTOS_TASK_CREATE xTaskCreate
#define NROS_FREERTOS_TASK_CREATE_STATIC xTaskCreateStatic
#endif

#endif /* NROS_FREERTOS_TASK_CREATE_H */
