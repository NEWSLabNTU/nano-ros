/*
 * Issue 2738 -- a thread on a small stack logs through nros_log.
 *
 * The thread logs RECORDS records and reports its stack's unused bytes. With
 * the sink's buffer pool (the default) every record reaches the console and
 * the thread finishes. With the old caller-stack buffer (`legacy-sink.conf`)
 * it overflows: the stack sentinel raises a fatal error and "thread done"
 * never prints. The harness reads the markers below.
 */

#include <zephyr/kernel.h>

#include <rclc/log.h>

#define SMALL_STACK 2048
#define RECORDS 50

uint32_t nros_zephyr_log_sink_fallbacks(void);

K_THREAD_STACK_DEFINE(small_stack, SMALL_STACK);
static struct k_thread small_thread;

static void small_entry(void* a, void* b, void* c) {
    (void)a;
    (void)b;
    (void)c;
    rclc_logger_t logger = rclc_log_get_logger("small");
    for (int i = 0; i < RECORDS; i++) {
        RCLC_LOG_INFO(logger, "small-stack record %d", i);
    }
    size_t unused = 0;
    (void)k_thread_stack_space_get(k_current_get(), &unused);
    printk("log-sink-small-stack: thread done, stack unused %u of %u\n", (unsigned)unused,
           (unsigned)SMALL_STACK);
}

int main(void) {
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
