/*
 * One console writer: C stdout reaches printk a whole line at a time
 * (CONFIG_NROS_CONSOLE_WHOLE_LINES, issue 2384).
 *
 * WHY. A Zephyr image with picolibc has two writers on its console. printk
 * goes to the logging thread under CONFIG_LOG_PRINTK (or straight to the
 * console under CONFIG_PRINTK_SYNC's spinlock without it). picolibc's stdout
 * goes to the console hook one character at a time, under no lock that printk
 * takes. On two cores the two paths interleave character by character:
 * measured on qemu_cortex_a53 SMP,
 *
 *     nros: core pin observed sched_task=`high` runn[ctrl] tick=97
 *
 * with the rest of the marker on a later line, so the test that reads the
 * marker line by line failed with placement working correctly.
 *
 * WHAT. This hook replaces the stdout hook and collects each thread's
 * characters into a thread-local line. A newline, or a full buffer, sends the
 * line with ONE printk call. Every console line then has one writer: under
 * CONFIG_LOG_PRINTK it is a log message the logging thread prints whole, and
 * without it it is one printk under CONFIG_PRINTK_SYNC. No second console path
 * is added; stdout joins the one printk already uses.
 *
 * printk never calls the stdout hook (it formats into its own FILE), so the
 * hook cannot recurse into itself.
 *
 * Before the kernel starts, and from an ISR, there is no thread whose line
 * this could be, so those characters go to printk one at a time, as before.
 */

#include <zephyr/init.h>
#include <zephyr/kernel.h>
#include <zephyr/sys/printk.h>

extern void __stdout_hook_install(int (*hook)(int c));

#define NROS_LINE_CAP CONFIG_NROS_CONSOLE_WHOLE_LINES_BUFFER

/* One line per thread. The +1 is the terminator printk's "%s" needs: under
 * CONFIG_LOG_PRINTK the string is copied into the log message, so it must end
 * where the line does. */
static __thread char nros_line[NROS_LINE_CAP + 1];
static __thread size_t nros_line_len;

static void nros_line_send(void) {
    nros_line[nros_line_len] = '\0';
    printk("%s", nros_line);
    nros_line_len = 0;
}

static int nros_stdout_whole_lines(int c) {
    if (k_is_pre_kernel() || k_is_in_isr()) {
        printk("%c", c);
        return c;
    }

    nros_line[nros_line_len++] = (char)c;
    if (c == '\n' || nros_line_len == NROS_LINE_CAP) {
        nros_line_send();
    }
    return c;
}

static int nros_console_whole_lines_init(void) {
    /* APPLICATION level runs after the console driver installed its own
     * stdout hook (PRE_KERNEL_1 or POST_KERNEL), so this one replaces it. */
    __stdout_hook_install(nros_stdout_whole_lines);
    return 0;
}

SYS_INIT(nros_console_whole_lines_init, APPLICATION, 0);
