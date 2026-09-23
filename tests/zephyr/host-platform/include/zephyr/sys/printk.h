/*
 * Host stand-in for <zephyr/sys/printk.h> (phase-460 W7). See
 * tests/zephyr/host-platform/include/zephyr/kernel.h for the rule this tree
 * follows.
 *
 * printk is REAL here, not a stub: the "HEAP EXHAUSTED" line is half of what
 * this wave is about -- the other half being that something happens AFTER it
 * -- so the test captures it. The definition is in
 * tests/zephyr/heap_exhaustion_test.c.
 */
#ifndef NROS_HOST_PLATFORM_ZEPHYR_SYS_PRINTK_H
#define NROS_HOST_PLATFORM_ZEPHYR_SYS_PRINTK_H

void printk(const char* fmt, ...) __attribute__((format(printf, 1, 2)));

#endif /* NROS_HOST_PLATFORM_ZEPHYR_SYS_PRINTK_H */
