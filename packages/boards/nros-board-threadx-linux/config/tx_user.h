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

/* NetX Duo BSD layer stores errno per-thread in this extension field. */
#define TX_THREAD_USER_EXTENSION    int bsd_errno;

#endif /* TX_USER_H */
