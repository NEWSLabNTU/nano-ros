/* Issue 2706 -- a periodic callback in INTERRUPT context, for the
 * interrupt-wake test image (`isr-wake-nuttx-qemu-arm`).
 *
 * A NuttX watchdog's function runs from the timer interrupt, so it is the
 * one portable way an application on this board reaches interrupt context
 * without owning a peripheral. The callback re-arms the watchdog, so it
 * fires every `period_us` until stopped. `nros_isr_timer_not_in_isr` counts
 * fires that did NOT run in interrupt context (`up_interrupt_context`), so a
 * test can refuse a run that tested a thread wake under an interrupt's name.
 *
 * One timer per image, which is all the test needs. */

#include <nuttx/config.h>
#include <nuttx/arch.h>
#include <nuttx/clock.h>
#include <nuttx/wdog.h>

#if defined(__ARM_ARCH_7A__)
#include <arch/armv7-a/cp15.h>
#endif

#include <stddef.h>
#include <stdint.h>

static struct wdog_s nros_isr_timer_wdog;
static void (*nros_isr_timer_cb)(void *);
static void *nros_isr_timer_arg;
static sclock_t nros_isr_timer_ticks;
static volatile uint32_t nros_isr_timer_not_in_isr;
static volatile int nros_isr_timer_running;

static void nros_isr_timer_fire(wdparm_t arg) {
    (void) arg;
    if (!up_interrupt_context()) {
        nros_isr_timer_not_in_isr++;
    }
    if (!nros_isr_timer_running) {
        return;
    }
    (void) wd_start(&nros_isr_timer_wdog, nros_isr_timer_ticks, nros_isr_timer_fire, 0);
    nros_isr_timer_cb(nros_isr_timer_arg);
}

/* Start the periodic callback; 0 on success. The period is rounded up to
 * whole system ticks (`USEC2TICK`), never below one. */
int nros_board_nuttx_isr_timer_start(uint32_t period_us, void (*cb)(void *), void *arg) {
    if (cb == NULL) return -1;
    nros_isr_timer_cb = cb;
    nros_isr_timer_arg = arg;
    sclock_t ticks = (sclock_t) USEC2TICK(period_us);
    nros_isr_timer_ticks = ticks > 0 ? ticks : 1;
    nros_isr_timer_running = 1;
    return wd_start(&nros_isr_timer_wdog, nros_isr_timer_ticks, nros_isr_timer_fire, 0) == 0 ? 0
                                                                                             : -1;
}

/* Stop it. Safe to call when it is not running. */
void nros_board_nuttx_isr_timer_stop(void) {
    nros_isr_timer_running = 0;
    (void) wd_cancel(&nros_isr_timer_wdog);
}

/* Fires that ran outside interrupt context (expected 0). */
uint32_t nros_board_nuttx_isr_timer_not_in_isr(void) {
    return nros_isr_timer_not_in_isr;
}

/* Nanoseconds from the ARM generic timer (CNTVCT over CNTFRQ), a clock finer
 * than the system tick: `clock_gettime` here counts whole ticks (1 ms), which
 * cannot resolve the sub-millisecond latency the interrupt-wake test measures
 * (issue 2748). 0 on a core without the generic timer. Callable from an
 * interrupt. */
uint64_t nros_board_nuttx_isr_timer_now_ns(void) {
#if defined(__ARM_ARCH_7A__)
    uint64_t count = CP15_GET64(CNTVCT);
    uint32_t freq = CP15_GET(CNTFRQ);
    if (freq == 0) return 0;
    /* Whole seconds, then the remainder: no 128-bit product on a 32-bit core,
     * and `rem * 1e9` fits 64 bits for any counter frequency below 18 GHz. */
    uint64_t rem = count % freq;
    return (count / freq) * 1000000000ull + (rem * 1000000000ull) / freq;
#else
    return 0;
#endif
}
