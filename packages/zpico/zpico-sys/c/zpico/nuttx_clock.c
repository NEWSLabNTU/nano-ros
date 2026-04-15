/**
 * NuttX clock functions for zenoh-pico.
 *
 * On NuttX (ZENOH_NUTTX), zenoh-pico's unix.h defines `z_clock_t` as
 * `struct timespec` (16 bytes). The default zpico-platform-shim implementation
 * uses `usize` (4 bytes on 32-bit ARM), which silently truncates the clock
 * value and produces garbage elapsed-time readings. That caused pending
 * queries to be dropped ~immediately via `_z_pending_query_process_timeout`,
 * making service/action replies undeliverable.
 *
 * To keep the shim symbols usable for other platforms, zpico-sys enables the
 * `skip-clock-symbols` feature on zpico-platform-shim for NuttX and provides
 * the correct struct-timespec-aware implementation here.
 */

#include <time.h>
#include <zenoh-pico/system/platform/unix.h>

z_clock_t z_clock_now(void) {
    z_clock_t now;
    clock_gettime(CLOCK_MONOTONIC, &now);
    return now;
}

static unsigned long _elapsed_ns(const z_clock_t *instant, const z_clock_t *now) {
    long sec = (long)(now->tv_sec - instant->tv_sec);
    long nsec = now->tv_nsec - instant->tv_nsec;
    if (nsec < 0) {
        sec -= 1;
        nsec += 1000000000L;
    }
    if (sec < 0) {
        return 0;
    }
    return (unsigned long)sec * 1000000000UL + (unsigned long)nsec;
}

unsigned long z_clock_elapsed_us(z_clock_t *instant) {
    z_clock_t now;
    clock_gettime(CLOCK_MONOTONIC, &now);
    return _elapsed_ns(instant, &now) / 1000UL;
}

unsigned long z_clock_elapsed_ms(z_clock_t *instant) {
    z_clock_t now;
    clock_gettime(CLOCK_MONOTONIC, &now);
    return _elapsed_ns(instant, &now) / 1000000UL;
}

unsigned long z_clock_elapsed_s(z_clock_t *instant) {
    z_clock_t now;
    clock_gettime(CLOCK_MONOTONIC, &now);
    return _elapsed_ns(instant, &now) / 1000000000UL;
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
