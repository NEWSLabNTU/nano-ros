/*
 * A HOST stand-in for <zephyr/init.h> (issue 2780), under the rule
 * <zephyr/kernel.h> in this tree states: present so `platform.c` compiles, and
 * never runs what it registers.
 *
 * A host program has no init levels, so SYS_INIT here only takes the address
 * of its function (keeping it "used" under -Werror) and registers nothing. The
 * reporter it would start calls the work API, whose stubs abort, so a test that
 * ever reached it would fail loudly instead of passing against behaviour this
 * tree does not implement.
 */
#ifndef NROS_HOST_PLATFORM_ZEPHYR_INIT_H
#define NROS_HOST_PLATFORM_ZEPHYR_INIT_H

#define SYS_INIT(fn, level, prio) \
    static int (*const nros_host_sys_init_##fn)(void) __attribute__((unused)) = fn

#endif
