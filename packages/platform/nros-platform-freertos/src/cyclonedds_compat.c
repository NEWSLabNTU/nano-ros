/*
 * FreeRTOS compatibility symbols required by Cyclone DDS's upstream ddsrt
 * FreeRTOS/lwIP port when linked into the nano-ros MPS2 bare-metal image.
 */

#include <nros/platform.h>

#include <errno.h>
#include <stddef.h>
#include <stdint.h>
#include <string.h>
#include <sys/types.h> /* clockid_t: newlib has it via time.h, TI Arm Clang's libc only here */
#include <time.h>

/* The ARM EABI thread-pointer helper. It exists only on an ARM EABI target,
 * whose linker script defines `__tls_base`; on any other target the compiler
 * never calls it, and defining it only demands a symbol nothing provides. */
#if defined(__ARM_EABI__)
extern char __tls_base[];

void *__aeabi_read_tp(void) {
    return __tls_base;
}
#endif

int gethostname(char *name, size_t len) {
    static const char hostname[] = "nano-ros";

    if (name == NULL || len == 0) {
        return -1;
    }

    size_t copy_len = sizeof(hostname);
    if (copy_len > len) {
        copy_len = len;
    }
    memcpy(name, hostname, copy_len);
    name[len - 1] = '\0';
    return 0;
}

/* `clock_gettime` answers each clock from the one symbol that owns it
 * (RFC-0141 D6, issue 2691):
 *
 *   - CLOCK_REALTIME from `nros_platform_time_now_ns`, the image's wall clock.
 *     With no source synced it is 0, which reads as the epoch: true, where
 *     the tick count used to pose as the time of day. Cyclone's ddsrt wall
 *     clock (a sample's source timestamp) reads this clock.
 *   - CLOCK_MONOTONIC from `nros_platform_clock_ns`, the port's monotonic
 *     clock, never from the wall clock: a wall clock steps when a source
 *     arrives, and a monotonic reading must not.
 *
 * Any other clock is refused with EINVAL, as POSIX says, rather than answered
 * with one of the two above. */
#if defined(CLOCK_MONOTONIC)
#define NROS_COMPAT_CLOCK_MONOTONIC CLOCK_MONOTONIC
#elif defined(__NEWLIB__)
/* newlib numbers it 4 but names it only under _POSIX_MONOTONIC_CLOCK, which
 * its arm-none-eabi build leaves unset; a caller that spells the number
 * itself still reaches this case. */
#define NROS_COMPAT_CLOCK_MONOTONIC ((clockid_t) 4)
#endif

/* The errno values set below. newlib defines both; a C library that lacks one
 * (TI Arm Clang's has fewer POSIX errnos, phase-575 W3) gets 1000 + its Linux
 * value, the convention lwIP's arch/cc.h uses, so it equals no number of the
 * library's own. */
#ifndef EINVAL
#define EINVAL 1022
#endif
#ifndef EFAULT
#define EFAULT 1014
#endif

static int compat_fail(int posix_errno) {
    errno = posix_errno;
    return -1;
}

int clock_gettime(clockid_t clock_id, struct timespec *ts) {
    if (ts == NULL) {
        return compat_fail(EFAULT);
    }

    uint64_t now_ns;
    if (clock_id == CLOCK_REALTIME) {
        now_ns = nros_platform_time_now_ns();
#if defined(NROS_COMPAT_CLOCK_MONOTONIC)
    } else if (clock_id == NROS_COMPAT_CLOCK_MONOTONIC) {
        now_ns = nros_platform_clock_ns();
#endif
    } else {
        return compat_fail(EINVAL);
    }
    ts->tv_sec = (time_t) (now_ns / 1000000000ull);
    ts->tv_nsec = (long) (now_ns % 1000000000ull);
    return 0;
}
