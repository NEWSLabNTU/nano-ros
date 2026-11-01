/* Phase-519 W3 — a declared ingress rate becomes the agent's pace, or nothing.
 *
 * `xrce_ingress_max_bytes_per_second` is the only logic between a subscription's
 * declared `rate_hz` and the `max_bytes_per_second` the agent enforces, and two
 * of its wrong answers are silent on a real system: a pace below one sample a
 * second stops the stream with no error (the agent's token bucket refuses a
 * sample bigger than itself forever), and a product past 65535 wraps in the
 * `uint16_t` wire field. No agent: the function is pure.
 */

#include "internal.h"

#include <stdio.h>

static int failures = 0;

static void check(uint32_t rate_mhz, uint32_t bound, uint16_t want, int want_why,
                  const char* what) {
    const char* why = "unset";
    uint16_t got = xrce_ingress_max_bytes_per_second(rate_mhz, bound, &why);
    int has_why = (why != NULL);
    if (got != want || has_why != want_why) {
        printf("  FAIL: %s: rate_mhz=%u bound=%u -> %u (why %s), expected %u (why %s)\n", what,
               (unsigned)rate_mhz, (unsigned)bound, (unsigned)got, has_why ? why : "none",
               (unsigned)want, want_why ? "a reason" : "none");
        failures++;
    } else {
        printf("  ok    %s\n", what);
    }
}

/* The symbol the backend's register path calls; the real one is Rust. */
rmw_ret_t nros_rmw_cffi_register_named(const char* name, const nros_rmw_vtable_t* vtable) {
    (void)name;
    (void)vtable;
    return NROS_RMW_RET_OK;
}

int main(void) {
    printf("ingress_pace:\n");
    check(0, 209, 0, 0, "undeclared asks for everything, silently");
    check(10000, 0, 0, 1, "a declaration with no bound asks for everything, with a reason");
    check(10000, 209, 2090, 0, "10 Hz x 209 B is the measured 2090 B/s");
    check(1000, 209, 209, 0, "1 Hz is the floor: the bucket holds exactly one sample");
    check(999, 209, 0, 1, "below 1 Hz refuses, with a reason");
    check(500, 64, 0, 1, "0.5 Hz refuses even for a small type");
    check(2500, 100, 250, 0, "a fractional rate is exact when it divides");
    check(1001, 3, 4, 0, "a fractional product rounds UP, never below the declaration");
    check(65535000, 1, 65535, 0, "the largest pace the wire field holds");
    check(100000, 656, 0, 1, "100 Hz x 656 B overflows the uint16_t field, with a reason");
    check(UINT32_MAX, UINT32_MAX, 0, 1, "no 64-bit overflow at the extremes");
    if (failures != 0) {
        printf("ingress_pace: %d failure(s)\n", failures);
        return 1;
    }
    printf("ingress_pace: OK\n");
    return 0;
}
