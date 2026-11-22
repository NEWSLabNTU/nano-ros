/* RFC-0141 D1/D2 (phase-582 W1d) -- the FreeRTOS port's lazy wall-clock
 * resolution, checked from a FreeRTOS task on the POSIX port. One case per
 * build (CLOCK_TEST_CASE); tests/clock-board-freertos-tests.sh drives them.
 * Prints `ok`/`FAIL` lines and exits non-zero on the first failed case. */
#include <nros/platform.h>

#include <FreeRTOS.h>
#include <task.h>

#include <inttypes.h>
#include <stdio.h>
#include <stdlib.h>
#include <time.h>

#define BOARD_NS 1700000000123456789ULL

extern int hook_calls;
static int bad;

#define CHECK(name, cond)                                                                          \
    do {                                                                                           \
        if (cond) {                                                                                \
            printf("  ok   %s\n", name);                                                           \
        } else {                                                                                   \
            printf("  FAIL %s\n", name);                                                           \
            bad = 1;                                                                               \
        }                                                                                          \
    } while (0)

extern void nros_platform_register_log_writer(void (*writer)(uint8_t, const uint8_t*, uintptr_t,
                                                             const uint8_t*, uintptr_t),
                                              void (*flusher)(void));

/* The board's log writer: one line per record, as the boards' writers do. */
static void log_writer(uint8_t severity, const uint8_t* name, uintptr_t name_len,
                       const uint8_t* msg, uintptr_t msg_len) {
    (void)severity;
    (void)name;
    (void)name_len;
    printf("%.*s\n", (int)msg_len, (const char*)msg);
    fflush(stdout);
}

__attribute__((unused)) static uint64_t realtime_ns(void) {
    struct timespec ts;
    clock_gettime(CLOCK_REALTIME, &ts);
    return (uint64_t)ts.tv_sec * 1000000000ULL + (uint64_t)ts.tv_nsec;
}

/* The HOST's monotonic clock. The waits are measured on it, not on the port's
 * tick clock: the hook's 200 ms is host time, and the POSIX kernel port's
 * SIGALRM tick runs slow under load, so the tick clock can show less wait than
 * the host saw (the window is counted in ticks, which never run ahead of the
 * host, so a window measured on the host is never short). */
static uint64_t host_mono_ns(void) {
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    return (uint64_t)ts.tv_sec * 1000000000ULL + (uint64_t)ts.tv_nsec;
}

/* Reads after the verdict ask the board nothing. */
__attribute__((unused)) static void check_no_more_calls(void) {
    int calls = hook_calls;
    for (int i = 0; i < 100; i++) {
        (void)nros_platform_time_now_ns();
        (void)nros_platform_wall_clock_state();
    }
    CHECK("later reads never ask the hook again", hook_calls == calls);
}

static void clock_task(void* arg) {
    (void)arg;
    uint64_t m0 = host_mono_ns();
    uint64_t r0 = realtime_ns();
    uint64_t t = nros_platform_time_now_ns();
    uint64_t r1 = realtime_ns();
    uint64_t m1 = host_mono_ns();
    nros_wall_clock_state_t st = nros_platform_wall_clock_state();
    printf("  first read took %" PRIu64 " ms, hook asked %d times, reading %" PRIu64 "\n",
           (uint64_t)((m1 - m0) / 1000000ULL), hook_calls, t);
    (void)r0;
    (void)r1;
#if defined(CASE_BOARD_LATE)
    /* source = board, window 1000 ms, hook answers after 200 ms. */
    CHECK("the first read waited for the board (>= 200 ms)", m1 - m0 >= 200000000ULL);
    CHECK("...and then returned (under 5 s)", m1 - m0 < 5000000000ULL);
    CHECK("reading is the board's instant",
          t + 2000000ULL >= BOARD_NS && t <= BOARD_NS + 50000000ULL);
    CHECK("state: synced", st.synced);
    CHECK("state: source board", st.source == NROS_WALL_CLOCK_SOURCE_BOARD);
    CHECK("the hook was polled", hook_calls >= 2);
    check_no_more_calls();
#elif defined(CASE_BOARD_NEVER)
    /* source = board, window 300 ms, hook never answers, unstamped. */
    CHECK("the first read waited out the 300 ms window", m1 - m0 >= 300000000ULL);
    CHECK("...and then returned (under 5 s)", m1 - m0 < 5000000000ULL);
    CHECK("reading is 0 (unstamped)", t == 0ULL);
    CHECK("state: not synced", !st.synced);
    CHECK("state: names board", st.source == NROS_WALL_CLOCK_SOURCE_BOARD);
    check_no_more_calls();
#elif defined(CASE_PLATFORM_HOST)
    /* freertos-posix: source = board,platform, hook never: platform answers
     * the host's CLOCK_REALTIME. */
    CHECK("reading >= CLOCK_REALTIME before", t >= r0);
    CHECK("reading <= CLOCK_REALTIME after", t <= r1);
    CHECK("state: synced", st.synced);
    CHECK("state: source platform", st.source == NROS_WALL_CLOCK_SOURCE_PLATFORM);
    check_no_more_calls();
#elif defined(CASE_PLATFORM_NONE)
    /* bare metal: source = board,platform, hook never: platform does NOT
     * answer, so epoch_failure (unstamped) applies. */
    CHECK("reading is 0 (platform has no wall clock)", t == 0ULL);
    CHECK("state: not synced", !st.synced);
    CHECK("state: names board", st.source == NROS_WALL_CLOCK_SOURCE_BOARD);
#elif defined(CASE_DEFAULT_HOST)
    /* freertos-posix, no list: the host's CLOCK_REALTIME. */
    CHECK("default reading >= CLOCK_REALTIME before", t >= r0);
    CHECK("default reading <= CLOCK_REALTIME after", t <= r1);
    CHECK("default state: synced, platform",
          st.synced && st.source == NROS_WALL_CLOCK_SOURCE_PLATFORM);
#elif defined(CASE_DEFAULT_NONE)
    /* bare metal, no list: 0, unsynced, as before the knob existed. */
    CHECK("default reading is 0", t == 0ULL);
    CHECK("default state: unsynced, platform",
          !st.synced && st.source == NROS_WALL_CLOCK_SOURCE_PLATFORM);
#else
#error "set CLOCK_TEST_CASE"
#endif
    fflush(stdout);
    exit(bad);
}

int main(void) {
    nros_platform_register_log_writer(log_writer, NULL);
    /* Before the scheduler runs, a read resolves nothing and reads 0. */
    uint64_t pre = nros_platform_time_now_ns();
    int pre_calls = hook_calls;
    printf("  pre-scheduler read %" PRIu64 ", hook asked %d times\n", pre, pre_calls);
#if defined(CASE_BOARD_LATE) || defined(CASE_BOARD_NEVER)
    CHECK("a pre-scheduler read resolves nothing", pre == 0ULL && pre_calls == 0);
#endif
    if (xTaskCreate(clock_task, "clock", configMINIMAL_STACK_SIZE * 8, NULL, tskIDLE_PRIORITY + 1,
                    NULL) != pdPASS) {
        fprintf(stderr, "FAIL: xTaskCreate\n");
        return 1;
    }
    vTaskStartScheduler();
    fprintf(stderr, "FAIL: scheduler exited\n");
    return 1;
}

void vApplicationMallocFailedHook(void) {
    fprintf(stderr, "FAIL: malloc failed in FreeRTOS heap\n");
    exit(1);
}
void vApplicationIdleHook(void) {}
void vApplicationTickHook(void) {}
void vApplicationStackOverflowHook(TaskHandle_t task, char* name) {
    (void)task;
    fprintf(stderr, "FAIL: stack overflow in task %s\n", name);
    exit(1);
}
void vApplicationGetIdleTaskMemory(StaticTask_t** tcb, StackType_t** stack, uint32_t* stack_size) {
    static StaticTask_t s_tcb;
    static StackType_t s_stack[configMINIMAL_STACK_SIZE];
    *tcb = &s_tcb;
    *stack = s_stack;
    *stack_size = configMINIMAL_STACK_SIZE;
}
void vApplicationGetTimerTaskMemory(StaticTask_t** tcb, StackType_t** stack, uint32_t* stack_size) {
    static StaticTask_t s_tcb;
    static StackType_t s_stack[configTIMER_TASK_STACK_DEPTH];
    *tcb = &s_tcb;
    *stack = s_stack;
    *stack_size = configTIMER_TASK_STACK_DEPTH;
}
