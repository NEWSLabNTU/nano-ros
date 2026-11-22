/*
 * Issue 2738 -- a thread dies holding nros_log's dispatcher.
 *
 * A C log sink faults (`k_oops`) on a record whose message starts with "DIE",
 * so the thread that raised it dies INSIDE nros_log's dispatch, holding its
 * guard: deterministic, with no stack overflow to aim. This image's fatal
 * error handler returns, so Zephyr aborts that thread and runs on, which is
 * the configuration where every later record used to be held for a holder
 * that never delivers it.
 *
 * Phase 1: the handler calls `nros_zephyr_log_dead_holder_check`, which must
 * print one line naming `victim1` and hand the dispatcher back; main's next
 * record must reach the console.
 *
 * Phase 2, the NEGATIVE CONTROL: the handler skips the check. Main's next
 * record must NOT reach the console: without the check the death is silent
 * and the record is stranded, which is what makes phase 1's delivery mean
 * something. It runs second because it leaves the dispatcher held for good.
 *
 * The strings below are `nros_tests::output::LOG_DEAD_HOLDER_*`.
 */

#include <string.h>

#include <zephyr/fatal.h>
#include <zephyr/kernel.h>

#include <rclc/log.h>

/* packages/platform/nros-platform-zephyr/src/platform.c */
bool nros_zephyr_log_dead_holder_check(struct k_thread* thread);

#define VICTIM_STACK 8192

K_THREAD_STACK_DEFINE(victim_stack, VICTIM_STACK);
static struct k_thread victim_thread;

/* Phase 1 runs the check, phase 2 (the control) does not. */
static volatile bool run_check = true;

void k_sys_fatal_error_handler(unsigned int reason, const struct arch_esf* esf) {
    (void)esf;
    if (run_check) {
        (void)nros_zephyr_log_dead_holder_check(k_current_get());
    }
    printk("log-dead-holder: handler returns, reason %u\n", reason);
    /* Returning makes Zephyr abort the faulting thread, not halt. */
}

static void fatal_sink(void* user_data, rcl_log_severity_t severity, const char* logger_name,
                       size_t logger_name_len, const char* message, size_t message_len,
                       const char* file, size_t file_len, uint32_t line, uint64_t timestamp_ns) {
    (void)user_data;
    (void)severity;
    (void)logger_name;
    (void)logger_name_len;
    (void)file;
    (void)file_len;
    (void)line;
    (void)timestamp_ns;
    if (message_len >= 3 && memcmp(message, "DIE", 3) == 0) {
        k_oops();
    }
}

static void victim_entry(void* a, void* b, void* c) {
    (void)b;
    (void)c;
    rclc_logger_t logger = rclc_log_get_logger("victim");
    RCLC_LOG_INFO(logger, "DIE in victim%d", (int)(intptr_t)a);
    printk("log-dead-holder: victim survived its sink (the sink did not fault)\n");
}

static void kill_victim(int n, const char* name) {
    k_thread_create(&victim_thread, victim_stack, K_THREAD_STACK_SIZEOF(victim_stack), victim_entry,
                    (void*)(intptr_t)n, NULL, NULL, K_PRIO_PREEMPT(5), 0, K_FOREVER);
    k_thread_name_set(&victim_thread, name);
    k_thread_start(&victim_thread);
    (void)k_thread_join(&victim_thread, K_SECONDS(10));
}

int main(void) {
    printk("log-dead-holder: boot\n");
    rclc_log_init();
    if (!rclc_log_add_sink(fatal_sink, NULL)) {
        printk("log-dead-holder: FAILED to install the faulting sink\n");
        return 0;
    }
    rclc_logger_t logger = rclc_log_get_logger("main");

    kill_victim(1, "victim1");
    RCLC_LOG_INFO(logger, "record after the released death");
    /* Deferred mode: let the log thread drain. */
    k_sleep(K_MSEC(500));
    printk("log-dead-holder: phase 1 done\n");

    run_check = false;
    kill_victim(2, "victim2");
    RCLC_LOG_INFO(logger, "record after the unreported death");
    k_sleep(K_MSEC(500));
    printk("log-dead-holder: done\n");
    return 0;
}
