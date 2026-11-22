/*
 * tx_user.h — ThreadX user configuration for Linux simulation
 *
 * Included by ThreadX when TX_INCLUDE_USER_DEFINE_FILE is defined.
 */

#ifndef TX_USER_H
#define TX_USER_H

#define TX_MAX_PRIORITIES           32
#define TX_TIMER_TICKS_PER_SECOND   100

/* Enable 64-bit pointer support.
 * On x86_64, ULONG is 32-bit but pointers are 64-bit. Without TX_64_BIT,
 * pointers passed as ULONG thread/timer entry arguments get truncated.
 * TX_64_BIT activates extension pointer macros in tx_api.h that store
 * the full 64-bit pointer in thread/timer extension fields. */
#if defined(__x86_64__) && __x86_64__
#define TX_64_BIT
#endif

/* The least stack a thread may have on this port (issue 2800).
 *
 * The port runs each ThreadX thread ON its ThreadX stack
 * (`pthread_attr_setstack` in `tx_thread_stack_build.c`), so every stack must
 * also be one the host can run a thread on: at least the host's
 * PTHREAD_STACK_MIN, plus the port's top reserve and the two 16-byte
 * alignments (at most 94 bytes; 128 here). PTHREAD_STACK_MIN differs by
 * architecture, and glibc 2.34 and later make it a `sysconf` call, so the value
 * is a constant per architecture here and the board checks it against the
 * running host at init (`board_threadx_linux.c`).
 *
 * The one definition. TX_MINIMUM_STACK and the timer thread's stack take it
 * below, so `tx_thread_create` refuses a smaller stack and the platform's
 * task floor (`nros_platform_task_init`) raises a small request to it; the
 * zenoh-pico task stack reads it (`zpico-sys/c/platform/threadx/platform.h`). */
#if defined(__aarch64__) || defined(__powerpc64__) || defined(__loongarch64)
#define NROS_THREADX_HOST_STACK_MIN 131072u
#else
#define NROS_THREADX_HOST_STACK_MIN 16384u
#endif
#define NROS_THREADX_HOST_STACK_FLOOR (NROS_THREADX_HOST_STACK_MIN + 128u)

#define TX_MINIMUM_STACK            NROS_THREADX_HOST_STACK_FLOOR
/* tx_port.h's 400 was annotated "Not used in Linux port!", true only while
 * threads ran on host stacks: the timer thread is a ThreadX thread too. */
#define TX_TIMER_THREAD_STACK_SIZE  NROS_THREADX_HOST_STACK_FLOOR

/* NetX Duo BSD layer stores errno per-thread in this extension field. */
#define TX_THREAD_USER_EXTENSION    int bsd_errno;

#endif /* TX_USER_H */
