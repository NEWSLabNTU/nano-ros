/*
 * Host stand-in for <zephyr/sys/printk.h> (phase-460 W6). See
 * tests/zephyr/host-shims/include/zephyr/kernel.h for the rule this tree
 * follows.
 *
 * printk is REAL here, not a stub: the shims' refusal line
 * ("OUT OF THREAD SLOTS") is the observable the negative control asserts on,
 * so it has to reach the test's stdout. The definition is in
 * tests/zephyr/thread_slot_release_test.c.
 */
#ifndef NROS_HOST_SHIMS_ZEPHYR_SYS_PRINTK_H
#define NROS_HOST_SHIMS_ZEPHYR_SYS_PRINTK_H

void printk(const char* fmt, ...) __attribute__((format(printf, 1, 2)));

#endif /* NROS_HOST_SHIMS_ZEPHYR_SYS_PRINTK_H */
