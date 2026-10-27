/**
 * NuttX clock functions for zenoh-pico.
 *
 * On NuttX (ZENOH_NUTTX), zenoh-pico's unix.h defines `z_clock_t` as
 * `struct timespec` (16 bytes), so every clock function and
 * `_z_condvar_wait_until` must take and return that shape. The alias TU
 * (`platform_aliases.c`) spells the same symbols for the generic header's
 * `uint64_t` ms, which is a different calling convention, not just a
 * different size: a `struct timespec` is returned through a hidden pointer
 * and a `uint64_t` in r0:r1.
 *
 * Issue 2094 -- this file used to be DEAD. Nothing referenced a symbol only it
 * defined, so the linker took the alias TU's copies and never pulled this
 * object, and every zenoh-pico 1.10 executor wake-up was computed from a
 * `z_clock_now()` result slot nobody wrote. A NuttX listener with no traffic
 * sent no keep-alive and lost its session at the lease. The alias TU now
 * skips its clock section on NuttX (`NROS_PLATFORM_ALIASES_SKIP_CLOCK`), and
 * `_z_condvar_wait_until` lives here, which zenoh-pico always references: if
 * the alias clock ever comes back on NuttX, the result is a duplicate-symbol
 * link error instead of a silent ABI split.
 */

#include <time.h>
#include <zenoh-pico/system/common/platform.h> /* unix.h under ZENOH_NUTTX, plus the prototypes */
#include <zenoh-pico/utils/result.h>

#include <nros/platform.h>

z_clock_t z_clock_now(void) {
    z_clock_t now;
    clock_gettime(CLOCK_MONOTONIC, &now);
    return now;
}

/* Nanoseconds as a 64-bit count. `unsigned long` is 32 bits on armv7a, where
 * nanoseconds wrap every 4.29 s: the executor measures every wake-up as
 * "elapsed since its epoch", so with a 32-bit count each one landed modulo
 * 4.29 s and the lease future fired at 20.8 s of a 60 s lease (issue 2094). */
static uint64_t _elapsed_ns(const z_clock_t *instant, const z_clock_t *now) {
    long sec = (long)(now->tv_sec - instant->tv_sec);
    long nsec = now->tv_nsec - instant->tv_nsec;
    if (nsec < 0) {
        sec -= 1;
        nsec += 1000000000L;
    }
    if (sec < 0) {
        return 0;
    }
    return (uint64_t)sec * 1000000000ULL + (uint64_t)nsec;
}

/* phase-508 — zenoh-pico 1.9's executor measures every wake-up from its own
 * epoch: elapsed from `epoch` to `instant`, 0 when `instant` is earlier. */
unsigned long zp_clock_elapsed_us_since(z_clock_t *instant, z_clock_t *epoch) {
    return (unsigned long)(_elapsed_ns(epoch, instant) / 1000ULL);
}

unsigned long zp_clock_elapsed_ms_since(z_clock_t *instant, z_clock_t *epoch) {
    return (unsigned long)(_elapsed_ns(epoch, instant) / 1000000ULL);
}

unsigned long zp_clock_elapsed_s_since(z_clock_t *instant, z_clock_t *epoch) {
    return (unsigned long)(_elapsed_ns(epoch, instant) / 1000000000ULL);
}

unsigned long z_clock_elapsed_us(z_clock_t *instant) {
    z_clock_t now;
    clock_gettime(CLOCK_MONOTONIC, &now);
    return (unsigned long)(_elapsed_ns(instant, &now) / 1000ULL);
}

unsigned long z_clock_elapsed_ms(z_clock_t *instant) {
    z_clock_t now;
    clock_gettime(CLOCK_MONOTONIC, &now);
    return (unsigned long)(_elapsed_ns(instant, &now) / 1000000ULL);
}

unsigned long z_clock_elapsed_s(z_clock_t *instant) {
    z_clock_t now;
    clock_gettime(CLOCK_MONOTONIC, &now);
    return (unsigned long)(_elapsed_ns(instant, &now) / 1000000000ULL);
}

void z_clock_advance_us(z_clock_t *clock, unsigned long duration) {
    unsigned long long total_ns =
        (unsigned long long)clock->tv_nsec + (unsigned long long)duration * 1000ULL;
    clock->tv_sec += (time_t)(total_ns / 1000000000ULL);
    clock->tv_nsec = (long)(total_ns % 1000000000ULL);
}

void z_clock_advance_ms(z_clock_t *clock, unsigned long duration) {
    unsigned long long total_ns =
        (unsigned long long)clock->tv_nsec + (unsigned long long)duration * 1000000ULL;
    clock->tv_sec += (time_t)(total_ns / 1000000000ULL);
    clock->tv_nsec = (long)(total_ns % 1000000000ULL);
}

void z_clock_advance_s(z_clock_t *clock, unsigned long duration) {
    clock->tv_sec += (time_t)duration;
}

/* The platform's condvar deadline is milliseconds on the
 * `nros_platform_clock_ns()` timeline, and zenoh-pico's is a monotonic
 * `struct timespec`. Carry the REMAINING time across rather than the absolute
 * value, so the two clocks never have to share an epoch. Codes are translated
 * the way the alias TU does (issue 1906): the platform returns 0 when
 * signalled, 1 on timeout and -1 on error. */
z_result_t _z_condvar_wait_until(_z_condvar_t *cv, _z_mutex_t *m, const z_clock_t *abstime) {
    if (abstime == NULL) {
        return _Z_ERR_GENERIC;
    }
    z_clock_t now;
    clock_gettime(CLOCK_MONOTONIC, &now);
    uint64_t remaining_ms = _elapsed_ns(&now, abstime) / 1000000ULL;
    uint64_t deadline_ms = nros_platform_clock_ns() / 1000000ULL + remaining_ms;
    int8_t rc = nros_platform_condvar_wait_until(cv, m, deadline_ms);
    if (rc == 0) {
        return _Z_RES_OK;
    }
    return rc > 0 ? Z_ETIMEDOUT : _Z_ERR_GENERIC;
}
