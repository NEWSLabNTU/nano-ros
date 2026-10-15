/* The slice that bounds zenoh-pico's `_z_condvar_wait` in code we own
 * (issue 1196, phase-502).
 *
 * zenoh-pico's `_z_condvar_wait` has no deadline in its contract. Every
 * caller re-tests its predicate in a loop, because a condition variable may
 * wake spuriously, so we bound each platform wait by this slice and report an
 * expired slice as a spurious wakeup. The caller's loop keeps the wait
 * unbounded where its own contract says so, and a lost signal costs one slice
 * instead of the thread.
 *
 * One spelling, included by both implementations:
 *   - `platform_aliases.c` (POSIX, NuttX, ThreadX — through
 *     `nros_platform_condvar_wait_until`);
 *   - `zephyr/nros_zenoh_zephyr_system.c` (Zephyr — `pthread_cond_timedwait`).
 * FreeRTOS uses zenoh-pico's own system file, patched on the fork. */
#ifndef NROS_ZP_CONDVAR_SLICE_H
#define NROS_ZP_CONDVAR_SLICE_H

#ifndef NROS_ZP_CONDVAR_SLICE_MS
#define NROS_ZP_CONDVAR_SLICE_MS 1000u
#endif

#endif /* NROS_ZP_CONDVAR_SLICE_H */
