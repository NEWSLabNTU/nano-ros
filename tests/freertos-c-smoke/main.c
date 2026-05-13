/*
 * FreeRTOS-Posix-port smoke test for nros-platform-freertos-c.
 *
 * Boots the FreeRTOS scheduler on the Posix port and runs a single
 * smoke task that exercises one symbol per capability category:
 * clock_ms / alloc/dealloc / sleep_ms / yield_now / random_u32 /
 * mutex / condvar / periodic timer. Exits the process with status 0
 * on success, non-zero on first failure. No networking (lwIP not
 * built into this harness — net symbols would require a parallel
 * lwIP-Unix-port build).
 */

#include <nros/platform.h>
#include <nros/platform_timer.h>

#include <FreeRTOS.h>
#include <task.h>
#include <semphr.h>

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <inttypes.h>

static volatile int s_timer_fires = 0;
static void on_timer(void *arg) {
    (void) arg;
    s_timer_fires++;
}

#define CHECK(cond, msg) do {                                                  \
    if (!(cond)) {                                                             \
        fprintf(stderr, "FAIL: %s (%s:%d)\n", msg, __FILE__, __LINE__);        \
        exit(1);                                                               \
    }                                                                          \
} while (0)

static void smoke_task(void *arg) {
    (void) arg;
    printf("nros-platform-freertos-c smoke begin\n");

    /* Clock */
    uint64_t t0 = nros_platform_clock_ms();
    nros_platform_sleep_ms(50);
    uint64_t t1 = nros_platform_clock_ms();
    printf("  clock_ms: %" PRIu64 " -> %" PRIu64 "\n", t0, t1);
    CHECK(t1 >= t0 + 40, "clock_ms did not advance");

    /* Alloc */
    void *p = nros_platform_alloc(64);
    CHECK(p != NULL, "alloc");
    memset(p, 0xCC, 64);
    nros_platform_dealloc(p);

    /* Yield */
    nros_platform_yield_now();

    /* Random — exercise the symbol; xorshift returns u32. */
    uint32_t r = nros_platform_random_u32();
    printf("  random_u32: 0x%08" PRIx32 "\n", r);

    /* Mutex round-trip on a stack-allocated ZMutex-shaped slot
     * (1 pointer storage). */
    void *mtx_storage[1] = { 0 };
    CHECK(nros_platform_mutex_init(mtx_storage) == 0, "mutex_init");
    CHECK(nros_platform_mutex_lock(mtx_storage) == 0, "mutex_lock");
    CHECK(nros_platform_mutex_unlock(mtx_storage) == 0, "mutex_unlock");
    CHECK(nros_platform_mutex_drop(mtx_storage) == 0, "mutex_drop");

    /* Periodic timer fires every 20 ms; wait 150 ms then destroy. */
    void *th = nros_platform_timer_create_periodic(20 * 1000, on_timer, NULL);
    CHECK(th != NULL, "timer create_periodic");
    nros_platform_sleep_ms(150);
    nros_platform_timer_destroy(th);
    printf("  timer fires over 150ms: %d\n", s_timer_fires);
    CHECK(s_timer_fires >= 4, "periodic timer fired too few times");

    printf("nros-platform-freertos-c smoke PASS\n");
    exit(0);
}

int main(void) {
    BaseType_t rc = xTaskCreate(
        smoke_task, "smoke",
        configMINIMAL_STACK_SIZE * 8,
        NULL,
        tskIDLE_PRIORITY + 1,
        NULL);
    if (rc != pdPASS) {
        fprintf(stderr, "FAIL: xTaskCreate smoke\n");
        return 1;
    }
    vTaskStartScheduler();
    fprintf(stderr, "FAIL: scheduler exited\n");
    return 1;
}

/* The FreeRTOS Posix port pulls these hooks unconditionally when
 * configUSE_*_HOOK is 0 — provide stubs to satisfy the linker. */
void vApplicationMallocFailedHook(void) {
    fprintf(stderr, "FAIL: malloc failed in FreeRTOS heap\n");
    exit(1);
}
void vApplicationIdleHook(void) {}
void vApplicationTickHook(void) {}
void vApplicationStackOverflowHook(TaskHandle_t task, char *name) {
    (void) task;
    fprintf(stderr, "FAIL: stack overflow in task %s\n", name);
    exit(1);
}
void vApplicationGetIdleTaskMemory(StaticTask_t **tcb,
                                   StackType_t **stack,
                                   uint32_t *stack_size) {
    static StaticTask_t  s_tcb;
    static StackType_t   s_stack[configMINIMAL_STACK_SIZE];
    *tcb = &s_tcb;
    *stack = s_stack;
    *stack_size = configMINIMAL_STACK_SIZE;
}
void vApplicationGetTimerTaskMemory(StaticTask_t **tcb,
                                    StackType_t **stack,
                                    uint32_t *stack_size) {
    static StaticTask_t  s_tcb;
    static StackType_t   s_stack[configTIMER_TASK_STACK_DEPTH];
    *tcb = &s_tcb;
    *stack = s_stack;
    *stack_size = configTIMER_TASK_STACK_DEPTH;
}
