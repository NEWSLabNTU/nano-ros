/*
 * Issue 2762 -- many threads and an interrupt log through nros_log at once.
 *
 * LOGGERS threads, released together on an SMP image, each log RECORDS
 * records, and a k_timer's expiry function (interrupt context) logs
 * ISR_RECORDS more. LOGGERS is well above the Zephyr sink's buffer pool (2 per
 * CPU), so records meet a full pool as well as each other.
 *
 * The verdict is the CONSOLE: the Zephyr console sink is re-entrant, so each
 * record reaches it directly in the context that raised it, and every one must
 * arrive. A record that finds every pool buffer claimed takes the sink's
 * truncating fallback, which still delivers it (marked) and is counted in
 * `fallbacks`; that is not a loss.
 *
 * The NEGATIVE CONTROL is in the same boot: a C sink installed with
 * `rclc_log_add_sink`, which does not declare itself re-entrant, so it gets
 * records one at a time behind nros_log's dispatch guard, and a record that
 * finds the guard held waits in the bounded hold queue. It spends a little
 * time per record to keep the guard held, and counts what it gets. It must get
 * FEWER than all of them: if it got every record the run never contended, and
 * the console's full count would prove nothing.
 *
 * The strings below are `nros_tests::output::LOG_STRESS_*`.
 */

#include <zephyr/kernel.h>
#include <zephyr/sys/atomic.h>

#include <rclc/log.h>

#define LOGGERS 32
#define RECORDS 200
#define ISR_RECORDS 100
#define LOGGER_STACK 4096
/* How long the serialised sink holds the dispatch guard per record. */
#define SERIAL_HOLD_US 20

uint32_t nros_zephyr_log_sink_fallbacks(void);

K_THREAD_STACK_ARRAY_DEFINE(logger_stacks, LOGGERS, LOGGER_STACK);
static struct k_thread logger_threads[LOGGERS];
static K_SEM_DEFINE(go, 0, LOGGERS);

static atomic_t serial_got = ATOMIC_INIT(0);
static atomic_t isr_sent = ATOMIC_INIT(0);
static rclc_logger_t isr_logger;

static void serial_sink(void* user_data, rcl_log_severity_t severity, const char* logger_name,
                        size_t logger_name_len, const char* message, size_t message_len,
                        const char* file, size_t file_len, uint32_t line, uint64_t timestamp_ns) {
    (void)user_data;
    (void)severity;
    (void)logger_name;
    (void)logger_name_len;
    (void)message;
    (void)message_len;
    (void)file;
    (void)file_len;
    (void)line;
    (void)timestamp_ns;
    k_busy_wait(SERIAL_HOLD_US);
    atomic_inc(&serial_got);
}

static void isr_tick(struct k_timer* timer) {
    atomic_val_t n = atomic_inc(&isr_sent);
    if (n >= ISR_RECORDS) {
        k_timer_stop(timer);
        return;
    }
    RCLC_LOG_INFO(isr_logger, "stress isr r%04d", (int)n);
}

static K_TIMER_DEFINE(isr_timer, isr_tick, NULL);

static void logger_entry(void* a, void* b, void* c) {
    (void)b;
    (void)c;
    int t = (int)(intptr_t)a;
    rclc_logger_t logger = rclc_log_get_logger("stress");
    /* Released together with the others; main gives the semaphore at once,
     * so a logger still waiting after 10 s says so and logs nothing, which
     * the cell reports as that logger's records missing. */
    if (k_sem_take(&go, K_SECONDS(10)) != 0) {
        printk("log-stress: logger %d was never released\n", t);
        return;
    }
    for (int r = 0; r < RECORDS; r++) {
        RCLC_LOG_INFO(logger, "stress t%02d r%04d", t, r);
    }
}

int main(void) {
    printk("log-stress: boot\n");
    rclc_log_init();
    if (!rclc_log_add_sink(serial_sink, NULL)) {
        printk("log-stress: FAILED to install the serialised sink\n");
        return 0;
    }
    isr_logger = rclc_log_get_logger("stress_isr");
    printk("log-stress: start, loggers %d, records %d, isr records %d, sink buffers %d, cpus %d\n",
           LOGGERS, RECORDS, ISR_RECORDS, CONFIG_NROS_ZEPHYR_LOG_SINK_BUFFERS,
           CONFIG_MP_MAX_NUM_CPUS);

    for (int t = 0; t < LOGGERS; t++) {
        k_thread_create(&logger_threads[t], logger_stacks[t], LOGGER_STACK, logger_entry,
                        (void*)(intptr_t)t, NULL, NULL, K_PRIO_PREEMPT(5), 0, K_NO_WAIT);
    }
    /* The interrupt starts with the threads, so its records interleave. */
    k_timer_start(&isr_timer, K_MSEC(1), K_MSEC(1));
    for (int t = 0; t < LOGGERS; t++) {
        k_sem_give(&go);
    }
    for (int t = 0; t < LOGGERS; t++) {
        (void)k_thread_join(&logger_threads[t], K_SECONDS(60));
    }
    while (atomic_get(&isr_sent) < ISR_RECORDS) {
        k_sleep(K_MSEC(10));
    }
    /* Deferred mode: let Zephyr's log thread drain before the summary. */
    k_sleep(K_SECONDS(2));
    printk("log-stress: done, records %d, serialised sink got %d, fallbacks %u\n",
           LOGGERS * RECORDS + ISR_RECORDS, (int)atomic_get(&serial_got),
           nros_zephyr_log_sink_fallbacks());
    return 0;
}
