/*
 * tx_user.h — ThreadX user configuration for QEMU RISC-V 64-bit virt
 *
 * Included by ThreadX when TX_INCLUDE_USER_DEFINE_FILE is defined.
 */

#ifndef TX_USER_H
#define TX_USER_H

#define TX_MAX_PRIORITIES           32
#define TX_TIMER_TICKS_PER_SECOND   100

/* NetX Duo BSD layer stores errno per-thread in this extension field. */
#define TX_THREAD_USER_EXTENSION    int bsd_errno;

#endif /* TX_USER_H */
