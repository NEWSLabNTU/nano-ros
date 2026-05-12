/*
 * tx_user.h — ThreadX user configuration for QEMU RISC-V 64-bit virt
 *
 * Included by ThreadX when TX_INCLUDE_USER_DEFINE_FILE is defined.
 */

#ifndef TX_USER_H
#define TX_USER_H

#define TX_MAX_PRIORITIES           32
#define TX_TIMER_TICKS_PER_SECOND   100

/* Phase 120.3: bump timer-thread stack from the 1 KB default. On rv64
 * LP64 the default 1024 bytes is exhausted once multiple thread-sleep
 * entries are in the timer list (each call frame is ~80–120 bytes
 * with the RV64 ABI's spill area, vs ~40 bytes on Cortex-M). Stack
 * overflow corrupts adjacent kernel state and the timer thread stops
 * making forward progress — `tx_thread_sleep` calls from zenoh-pico's
 * lease task never return as a result. */
#define TX_TIMER_THREAD_STACK_SIZE  8192

/* NetX Duo BSD layer stores errno per-thread in this extension field. */
#define TX_THREAD_USER_EXTENSION    int bsd_errno;

#endif /* TX_USER_H */
