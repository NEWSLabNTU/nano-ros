/*
 * ThreadX-linux smoke test for nros-platform-threadx-c.
 *
 * Boots ThreadX on the linux/gnu port; tx_application_define creates
 * a byte pool (registered with nros-platform-threadx-c via
 * nros_platform_threadx_set_byte_pool + the existing
 * nros_platform_threadx_set_byte_pool from platform.c, plus the
 * sibling _set_timer_pool from timer.c) and a smoke thread. Smoke
 * thread runs the same probes as the POSIX smoke (clock / alloc /
 * sleep / yield / random / mutex / timer) and exits the process on
 * PASS or FAIL.
 *
 * No networking — NetX Duo is heavy to spin up just for a unit
 * smoke. Network paths covered separately on real hardware /
 * application-level integration.
 */

#include <nros/platform.h>
#include <nros/platform_timer.h>

#include <tx_api.h>

#include <stdio.h>
#include <stdlib.h>
#include <unistd.h>     /* _exit */
#include <inttypes.h>
#include <string.h>

extern void nros_platform_threadx_set_byte_pool(void *pool);
extern void nros_platform_threadx_set_timer_pool(void *pool);

#define HEAP_BYTES        (256 * 1024)
#define SMOKE_STACK_BYTES (32 * 1024)

static TX_BYTE_POOL s_pool;
static uint8_t      s_pool_storage[HEAP_BYTES];

static volatile int s_timer_fires = 0;
static void on_timer(void *user_data) {
    (void) user_data;
    s_timer_fires++;
}

#define CHECK(cond, msg) do {                                                  \
    if (!(cond)) {                                                             \
        fprintf(stderr, "FAIL: %s (%s:%d)\n", msg, __FILE__, __LINE__);        \
        fflush(NULL);                                                          \
        _exit(1);                                                              \
    }                                                                          \
} while (0)

static void smoke_entry(ULONG arg) {
    (void) arg;
    printf("nros-platform-threadx-c smoke begin\n");

    uint64_t t0 = nros_platform_clock_ms();
    nros_platform_sleep_ms(50);
    uint64_t t1 = nros_platform_clock_ms();
    printf("  clock_ms: %" PRIu64 " -> %" PRIu64 "\n", t0, t1);
    CHECK(t1 >= t0 + 30, "clock_ms did not advance");

    void *p = nros_platform_alloc(64);
    CHECK(p != NULL, "alloc");
    memset(p, 0xCC, 64);
    nros_platform_dealloc(p);

    nros_platform_yield_now();

    uint32_t r = nros_platform_random_u32();
    printf("  random_u32: 0x%08" PRIx32 "\n", r);

    /* Mutex storage = sizeof(TX_MUTEX). Use a static buffer so the
     * layout + alignment is unambiguous; tx_byte_allocate's returned
     * pointer doesn't zero the memory and tx_mutex_create wants a
     * fresh struct. */
    static TX_MUTEX s_test_mtx;
    memset(&s_test_mtx, 0, sizeof(s_test_mtx));
    void *mtx = &s_test_mtx;
    CHECK(nros_platform_mutex_init(mtx) == 0, "mutex_init");
    printf("  mutex_init ok\n");
    CHECK(nros_platform_mutex_lock(mtx) == 0, "mutex_lock");
    CHECK(nros_platform_mutex_unlock(mtx) == 0, "mutex_unlock");
    CHECK(nros_platform_mutex_drop(mtx) == 0, "mutex_drop");
    printf("  mutex round-trip ok\n");

    void *th = nros_platform_timer_create_periodic(20 * 1000, on_timer, NULL);
    CHECK(th != NULL, "timer create_periodic");
    nros_platform_sleep_ms(200);
    nros_platform_timer_destroy(th);
    printf("  timer fires over 200ms: %d\n", s_timer_fires);
    CHECK(s_timer_fires >= 4, "periodic timer fired too few times");

    printf("nros-platform-threadx-c smoke PASS\n");
    fflush(NULL);
    /* _exit avoids running atexit hooks; ThreadX's signal-driven timer
     * thread races with libc shutdown otherwise. */
    _exit(0);
}

void tx_application_define(void *first_unused_memory) {
    (void) first_unused_memory;

    UINT rc = tx_byte_pool_create(&s_pool, "nros pool",
                                  s_pool_storage, sizeof(s_pool_storage));
    if (rc != TX_SUCCESS) {
        fprintf(stderr, "FAIL: tx_byte_pool_create rc=%u\n", (unsigned) rc);
        exit(1);
    }
    nros_platform_threadx_set_byte_pool(&s_pool);
    nros_platform_threadx_set_timer_pool(&s_pool);

    /* Allocate smoke-thread stack from the same byte pool so the
     * static linker doesn't fight us. */
    void *stack = NULL;
    rc = tx_byte_allocate(&s_pool, &stack, SMOKE_STACK_BYTES, TX_NO_WAIT);
    if (rc != TX_SUCCESS) {
        fprintf(stderr, "FAIL: smoke stack tx_byte_allocate rc=%u\n", (unsigned) rc);
        exit(1);
    }
    static TX_THREAD s_smoke_thread;
    rc = tx_thread_create(&s_smoke_thread, "smoke", smoke_entry, 0,
                          stack, SMOKE_STACK_BYTES,
                          16, 16, TX_NO_TIME_SLICE, TX_AUTO_START);
    if (rc != TX_SUCCESS) {
        fprintf(stderr, "FAIL: tx_thread_create rc=%u\n", (unsigned) rc);
        exit(1);
    }
}

int main(void) {
    setvbuf(stdout, NULL, _IOLBF, 0);
    setvbuf(stderr, NULL, _IOLBF, 0);
    tx_kernel_enter();   /* does not return on the linux port */
    fprintf(stderr, "FAIL: tx_kernel_enter returned\n");
    return 1;
}
