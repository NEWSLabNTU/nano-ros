/*
 * Issue 2738 -- a thread on a small stack logs through nros_log.
 *
 * The thread logs RECORDS records and reports how many bytes of its stack it
 * used. Its stack is large enough that neither sink overflows it, so the
 * number is a measurement rather than a crash: the harness holds the pool
 * image to a budget a small thread can afford, and checks that the old
 * caller-stack buffer (`legacy-sink.conf`) exceeds it.
 *
 * An overflow is not the verdict because it is not reliably detected: the old
 * sink's buffer is written from its low end, past the sentinel word, so it
 * corrupts whatever lies below the stack and the thread often finishes.
 */

#include <zephyr/kernel.h>

#include <rclc/log.h>

#define THREAD_STACK 8192
#define RECORDS 50

uint32_t nros_zephyr_log_sink_fallbacks(void);

K_THREAD_STACK_DEFINE(small_stack, THREAD_STACK);
static struct k_thread small_thread;

static void small_entry(void* a, void* b, void* c) {
    (void)a;
    (void)b;
    (void)c;
    rclc_logger_t logger = rclc_log_get_logger("small");
    for (int i = 0; i < RECORDS; i++) {
        RCLC_LOG_INFO(logger, "small-stack record %d", i);
    }
    /* stack_info.size is the region k_thread_stack_space_get scans, so the
     * subtraction is in its units (THREAD_STACK_INFO). */
    size_t unused = 0;
    (void)k_thread_stack_space_get(k_current_get(), &unused);
    size_t size = k_current_get()->stack_info.size;
    printk("log-sink-small-stack: thread done, stack used %u of %u\n", (unsigned)(size - unused),
           (unsigned)size);
}

int main(void) {
    printk("log-sink-small-stack: boot\n");
    rclc_log_init();
    printk("log-sink-small-stack: start, sink buffers %d, cpus %d\n",
           CONFIG_NROS_ZEPHYR_LOG_SINK_BUFFERS, CONFIG_MP_MAX_NUM_CPUS);
    k_thread_create(&small_thread, small_stack, K_THREAD_STACK_SIZEOF(small_stack), small_entry,
                    NULL, NULL, NULL, K_PRIO_PREEMPT(5), 0, K_NO_WAIT);
    k_thread_name_set(&small_thread, "small_logger");
    (void)k_thread_join(&small_thread, K_SECONDS(10));
    /* Deferred mode: let the log thread drain before the summary. */
    k_sleep(K_MSEC(500));
    printk("log-sink-small-stack: done, fallbacks %u\n", nros_zephyr_log_sink_fallbacks());
    return 0;
}
