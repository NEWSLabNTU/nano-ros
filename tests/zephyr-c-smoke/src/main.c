/*
 * Phase 121.6.zephyr-c smoke test main.
 *
 * Runs on native_sim (host x86_64 binary). Exercises one symbol
 * per capability of <nros/platform.h> + <nros/platform_timer.h>
 * through the canonical extern declarations.
 *
 * Exits the process with status 0 on PASS, 1 on first FAIL.
 * Networking paths are out of scope here (no IP stack in this
 * profile).
 */

#include <nros/platform.h>
#include <nros/platform_timer.h>

#include <zephyr/kernel.h>

#include <inttypes.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static volatile int s_timer_fires = 0;
static void on_timer(void *user_data) {
    (void) user_data;
    s_timer_fires++;
}

#define CHECK(cond, msg) do {                                                  \
    if (!(cond)) {                                                             \
        printk("FAIL: %s (%s:%d)\n", msg, __FILE__, __LINE__);                 \
        exit(1);                                                               \
    }                                                                          \
} while (0)

int main(void) {
    printk("nros-platform-zephyr-c smoke begin\n");

    /* Clock */
    uint64_t t0 = nros_platform_clock_ms();
    nros_platform_sleep_ms(50);
    uint64_t t1 = nros_platform_clock_ms();
    printk("  clock_ms: %" PRIu64 " -> %" PRIu64 "\n", t0, t1);
    CHECK(t1 >= t0 + 30, "clock_ms did not advance");

    /* Alloc */
    void *p = nros_platform_alloc(64);
    CHECK(p != NULL, "alloc");
    memset(p, 0xCC, 64);
    nros_platform_dealloc(p);

    /* Yield */
    nros_platform_yield_now();

    /* Random */
    uint32_t r = nros_platform_random_u32();
    printk("  random_u32: 0x%08" PRIx32 "\n", r);

    /* Mutex round-trip on a Zephyr k_mutex (heap-allocated). */
    void *mtx = k_malloc(sizeof(struct k_mutex));
    CHECK(mtx != NULL, "mutex k_malloc");
    memset(mtx, 0, sizeof(struct k_mutex));
    CHECK(nros_platform_mutex_init(mtx) == 0, "mutex_init");
    CHECK(nros_platform_mutex_lock(mtx) == 0, "mutex_lock");
    CHECK(nros_platform_mutex_unlock(mtx) == 0, "mutex_unlock");
    CHECK(nros_platform_mutex_drop(mtx) == 0, "mutex_drop");
    k_free(mtx);
    printk("  mutex round-trip ok\n");

    /* Periodic timer fires every 20 ms; wait 200 ms then destroy. */
    void *th = nros_platform_timer_create_periodic(20 * 1000, on_timer, NULL);
    CHECK(th != NULL, "timer create_periodic");
    nros_platform_sleep_ms(200);
    nros_platform_timer_destroy(th);
    printk("  timer fires over 200ms: %d\n", s_timer_fires);
    CHECK(s_timer_fires >= 4, "periodic timer fired too few times");

    printk("nros-platform-zephyr-c smoke PASS\n");
    /* native_sim runs in its own process; exit() returns the
     * status to the host shell. */
    exit(0);
}
