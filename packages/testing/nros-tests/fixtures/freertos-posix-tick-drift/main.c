/* Tick-vs-host drift probe for the FreeRTOS POSIX port (issue 3010).
 *
 * The port's tick is a host timer thread signalling the scheduler thread. A
 * signal that falls due while the tick is masked coalesces with the next one,
 * so a port that advances the tick once per signal loses time whenever the
 * scheduler thread cannot take the signal promptly — under host load, in
 * practice. `nros_platform_clock_ns` on this board reads the tick count, so
 * every timer and rate in an image ran slow by the same amount.
 *
 * Host load is not reproducible and would reach every other process on the
 * machine, so the probe makes its own: ten times a second it masks the tick
 * (a critical section) for 30 ms of host time, then sleeps 70 ms. Ticks that
 * fall due under the mask must still be delivered once it lifts. Measured:
 * the old port runs ~36 % slow under this load, the catch-up port within
 * 0.01 %.
 *
 *   tick-drift [seconds]     default 5
 *
 * Prints one line, then exits 0:
 *   TICK_DRIFT host_s=<f> tick_s=<f> drift_pct=<f> worst_period_ms=<f>
 * `drift_pct` is positive when the tick runs slow. `worst_period_ms` is the
 * longest host time one 100 ms period (30 ms mask + 70 ms delay) took. */
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <time.h>

#include "FreeRTOS.h"
#include "task.h"

#define MASK_NS 30000000ULL
#define DELAY_MS 70

static int g_seconds = 5;

static uint64_t host_ns(void) {
    struct timespec t;
    clock_gettime(CLOCK_MONOTONIC, &t);
    return (uint64_t)t.tv_sec * 1000000000ULL + (uint64_t)t.tv_nsec;
}

static void probe(void *arg) {
    (void)arg;
    vTaskDelay(10);
    const TickType_t t0 = xTaskGetTickCount();
    const uint64_t h0 = host_ns();
    uint64_t worst_ns = 0;
    /* Run by HOST time, not by a period count: with the tick on time a
     * period can end a little early, so a count of periods would run short
     * of the seconds asked for. */
    const uint64_t run_ns = (uint64_t)g_seconds * 1000000000ULL;
    while (host_ns() - h0 < run_ns) {
        const uint64_t a = host_ns();
        taskENTER_CRITICAL();
        while (host_ns() - a < MASK_NS) {
        }
        taskEXIT_CRITICAL();
        vTaskDelay(pdMS_TO_TICKS(DELAY_MS));
        const uint64_t d = host_ns() - a;
        if (d > worst_ns) {
            worst_ns = d;
        }
    }
    const double tick_s = (double)(xTaskGetTickCount() - t0) / (double)configTICK_RATE_HZ;
    const double host_s = (double)(host_ns() - h0) / 1e9;
    printf("TICK_DRIFT host_s=%.3f tick_s=%.3f drift_pct=%.4f worst_period_ms=%.1f\n", host_s,
           tick_s, 100.0 * (host_s - tick_s) / host_s, (double)worst_ns / 1e6);
    fflush(stdout);
    exit(0);
}

int main(int argc, char **argv) {
    if (argc > 1) {
        g_seconds = atoi(argv[1]);
    }
    if (g_seconds < 1) {
        fprintf(stderr, "tick-drift: seconds must be >= 1\n");
        return 2;
    }
    xTaskCreate(probe, "probe", 4096, NULL, 3, NULL);
    vTaskStartScheduler();
    return 1;
}

void vApplicationMallocFailedHook(void) { abort(); }
void vApplicationStackOverflowHook(TaskHandle_t t, char *n) {
    (void)t;
    fprintf(stderr, "stack overflow in %s\n", n);
    abort();
}
void vApplicationIdleHook(void) {}
void vApplicationTickHook(void) {}
void vAssertCalled(const char *f, unsigned long l) {
    fprintf(stderr, "assert %s:%lu\n", f, l);
    abort();
}
void freertos_assert_failed(const char *f, int l) {
    fprintf(stderr, "assert %s:%d\n", f, l);
    abort();
}
void vApplicationGetIdleTaskMemory(StaticTask_t **tcb, StackType_t **stack,
                                   configSTACK_DEPTH_TYPE *depth) {
    static StaticTask_t t;
    static StackType_t s[configMINIMAL_STACK_SIZE];
    *tcb = &t;
    *stack = s;
    *depth = configMINIMAL_STACK_SIZE;
}
void vApplicationGetTimerTaskMemory(StaticTask_t **tcb, StackType_t **stack,
                                    configSTACK_DEPTH_TYPE *depth) {
    static StaticTask_t t;
    static StackType_t s[configTIMER_TASK_STACK_DEPTH];
    *tcb = &t;
    *stack = s;
    *depth = configTIMER_TASK_STACK_DEPTH;
}
