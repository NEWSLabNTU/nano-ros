/*
 * phase-460 W6 -- the two shim entry points the host thread-slot test uses.
 *
 * `zephyr/nros_platform_zephyr_shims.c` has no header of its own: the Rust
 * side declares its symbols `extern "C"` and `nros_zenoh_zephyr_system.c`
 * declares the release inline at its one call site. This file is the test
 * harness's copy of those two prototypes, kept here rather than in each test
 * TU so the test and the mutant cannot drift from one another.
 *
 * If a signature here stops matching the shims, the harness stops compiling --
 * which is the intended failure. It is not a public header and nothing outside
 * `tests/zephyr/` may include it.
 */
#ifndef NROS_TESTS_ZEPHYR_THREAD_SLOT_SHIM_DECLS_H
#define NROS_TESTS_ZEPHYR_THREAD_SLOT_SHIM_DECLS_H

#include <pthread.h>

/* Claim a stack slot and start `entry(arg)` on it. 0 on success; -1 when the
 * pool is full (and the shim says so on printk). */
int nros_zephyr_task_create(pthread_t* thread, void* (*entry)(void*), void* arg);

/* Give `owner`'s slot back. Called from `_z_task_join` once the join has
 * RETURNED -- never on exit, and never after a detach. */
void nros_zephyr_task_slot_release(pthread_t owner);

#endif /* NROS_TESTS_ZEPHYR_THREAD_SLOT_SHIM_DECLS_H */
