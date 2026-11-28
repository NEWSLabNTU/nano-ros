/* The tick-drift probe for the threadx-linux board (issue 3017).
 *
 * A bare ThreadX kernel process, no nano-ros runtime: on this board
 * `nros_platform_clock_ns` is the kernel tick, so the question is what the
 * KERNEL's tick does. Ten times a second one thread masks interrupts
 * (TX_DISABLE, which on the Linux port holds `_tx_linux_mutex`, so the
 * simulated timer interrupt blocks) for 30 ms of host time, then sleeps
 * 70 ms of ticks. A port that delivers one tick per timer wakeup loses the
 * ticks that fell due while it was blocked; one that counts ticks from the
 * host's monotonic clock delivers them late but delivers them all.
 *
 *   tick-drift [seconds]     (default 5)
 *
 * Prints one line, then exits:
 *   TICK_DRIFT host_s= tick_s= drift_pct= worst_period_ms=
 * `drift_pct` is positive when the tick runs slow. */
#include "tx_api.h"

#include <stdio.h>
#include <stdlib.h>
#include <time.h>

#define MASK_NS 30000000LL
#define SLEEP_TICKS (TX_TIMER_TICKS_PER_SECOND * 7 / 100)

static TX_THREAD probe_thread;
static UCHAR probe_stack[262144];
static int probe_seconds = 5;

static long long host_ns(void) {
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    return (long long)ts.tv_sec * 1000000000LL + ts.tv_nsec;
}

static void probe_entry(ULONG arg) {
    (void)arg;
    long long worst_ns = 0;

    /* Start on a tick boundary so the first period is a whole one. */
    tx_thread_sleep(1);
    ULONG tick0 = tx_time_get();
    long long host0 = host_ns();

    /* Run on HOST time: on a port that catches up, the ticks owed for a mask
       are delivered after TX_RESTORE and count toward the sleep that follows,
       so a period is shorter than 100 ms of host time. Counting periods would
       then run less host time than asked. */
    while (host_ns() - host0 < (long long)probe_seconds * 1000000000LL) {
        long long p0 = host_ns();
        {
            TX_INTERRUPT_SAVE_AREA
            TX_DISABLE
            while (host_ns() - p0 < MASK_NS) {
            }
            TX_RESTORE
        }
        tx_thread_sleep(SLEEP_TICKS);
        long long p = host_ns() - p0;
        if (p > worst_ns) {
            worst_ns = p;
        }
    }

    double host_s = (double)(host_ns() - host0) / 1e9;
    double tick_s = (double)(tx_time_get() - tick0) / TX_TIMER_TICKS_PER_SECOND;
    printf("TICK_DRIFT host_s=%.3f tick_s=%.3f drift_pct=%.4f worst_period_ms=%.1f\n", host_s,
           tick_s, 100.0 * (host_s - tick_s) / host_s, (double)worst_ns / 1e6);
    fflush(stdout);
    exit(0);
}

void tx_application_define(void* first_unused_memory) {
    (void)first_unused_memory;
    tx_thread_create(&probe_thread, "tick-drift", probe_entry, 0, probe_stack, sizeof probe_stack,
                     1, 1, TX_NO_TIME_SLICE, TX_AUTO_START);
}

int main(int argc, char** argv) {
    if (argc > 1) {
        probe_seconds = atoi(argv[1]);
    }
    if (probe_seconds < 1) {
        fprintf(stderr, "tick-drift: seconds must be at least 1\n");
        return 2;
    }
    tx_kernel_enter();
    return 1;
}
