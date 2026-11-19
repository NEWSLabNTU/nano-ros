/* Issue 2706 -- the interrupt this image's test takes.
 *
 * A k_timer's expiry function runs in the system clock's interrupt handler,
 * so `on_expiry` is interrupt context: it checks that (`k_is_in_isr`), stamps
 * the cycle counter and calls the Rust handler, which calls
 * `GuardCondition::trigger_from_isr`. The guard's callback then asks how many
 * microseconds have passed since the stamp. */
#include <zephyr/kernel.h>
#include <stdint.h>

extern void nros_isr_test_on_interrupt(void);

static struct k_timer timer;
static volatile uint32_t stamp_cycles;
static volatile uint32_t not_in_isr;

static void on_expiry(struct k_timer *t) {
    (void)t;
    if (!k_is_in_isr()) {
        not_in_isr++;
    }
    stamp_cycles = k_cycle_get_32();
    nros_isr_test_on_interrupt();
}

void nros_isr_test_start(uint32_t period_us) {
    k_timer_init(&timer, on_expiry, NULL);
    k_timer_start(&timer, K_USEC(period_us), K_USEC(period_us));
}

void nros_isr_test_stop(void) {
    k_timer_stop(&timer);
}

/* Microseconds since the last expiry's stamp. */
uint32_t nros_isr_test_elapsed_us(void) {
    uint32_t cycles = k_cycle_get_32() - stamp_cycles;
    return (uint32_t)k_cyc_to_us_floor32(cycles);
}

/* Expiries that did NOT run in interrupt context (must stay 0). */
uint32_t nros_isr_test_not_in_isr(void) {
    return not_in_isr;
}
