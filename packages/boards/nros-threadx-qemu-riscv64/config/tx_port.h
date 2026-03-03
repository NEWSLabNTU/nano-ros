/*
 * tx_port.h — Fixed ThreadX port header for RISC-V 64-bit bare-metal (QEMU virt)
 *
 * Shadows external/threadx/ports/risc-v64/gnu/inc/tx_port.h to fix the ULONG
 * typedef. The upstream port incorrectly defines ULONG as "unsigned long" (8
 * bytes on rv64), which breaks NetX Duo's network protocol parsing — all packet
 * code uses `ULONG *` pointer arithmetic assuming 4-byte words.
 *
 * Fix: define ULONG as "unsigned int" (4 bytes), matching the Linux x86_64 and
 * all AArch64 ThreadX ports. ThreadX kernel correctness is preserved because
 * pointer-sized operations use ALIGN_TYPE (ULONG64) and register save/restore
 * uses explicit 64-bit loads/stores in assembly.
 *
 * Based on: threadx/ports/risc-v64/gnu/inc/tx_port.h Version 6.4.2
 */

#ifndef TX_PORT_H
#define TX_PORT_H

#ifdef __ASSEMBLER__

#if __riscv_xlen == 64
# define SLL32    sllw
# define STORE    sd
# define LOAD     ld
# define LWU      lwu
# define LOG_REGBYTES 3
#else
# define SLL32    sll
# define STORE    sw
# define LOAD     lw
# define LWU      lw
# define LOG_REGBYTES 2
#endif
#define REGBYTES (1 << LOG_REGBYTES)

/*
 * TX_THREAD struct field offsets for assembly code.
 *
 * With ULONG=unsigned int (4 bytes) and VOID*=8 bytes, the critical section
 * of TX_THREAD has mixed-size fields. These offsets match the C compiler's
 * struct layout and MUST be kept in sync with tx_api.h:TX_THREAD_STRUCT.
 *
 * Layout:
 *   Offset  0: tx_thread_id              (ULONG, 4 bytes)
 *   Offset  4: tx_thread_run_count       (ULONG, 4 bytes)
 *   Offset  8: tx_thread_stack_ptr       (VOID*, 8 bytes)
 *   Offset 16: tx_thread_stack_start     (VOID*, 8 bytes)
 *   Offset 24: tx_thread_stack_end       (VOID*, 8 bytes)
 *   Offset 32: tx_thread_stack_size      (ULONG, 4 bytes)
 *   Offset 36: tx_thread_time_slice      (ULONG, 4 bytes)
 *   Offset 40: tx_thread_new_time_slice  (ULONG, 4 bytes)
 */
#define TX_TCB_ID_OFF               0
#define TX_TCB_RUN_COUNT_OFF        4
#define TX_TCB_STACK_PTR_OFF        8
#define TX_TCB_STACK_START_OFF      16
#define TX_TCB_STACK_END_OFF        24
#define TX_TCB_STACK_SIZE_OFF       32
#define TX_TCB_TIME_SLICE_OFF       36
#define TX_TCB_NEW_TIME_SLICE_OFF   40

#else   /*not __ASSEMBLER__ */

/* Include for memset.  */
#include <string.h>

/* Determine if the optional ThreadX user define file should be used.  */
#ifdef TX_INCLUDE_USER_DEFINE_FILE
#include "tx_user.h"
#endif

/* Define ThreadX basic types for this port.  */

#define VOID                                    void
typedef char                                    CHAR;
typedef unsigned char                           UCHAR;
typedef int                                     INT;
typedef unsigned int                            UINT;

/*
 * FIX: Use 32-bit LONG/ULONG on rv64 (matching cortex_a5x, linux/x86_64 ports).
 * The upstream risc-v64 port uses "unsigned long" which is 8 bytes on rv64 and
 * breaks NetX Duo's ULONG* packet parsing.
 */
typedef int                                     LONG;
typedef unsigned int                            ULONG;

typedef unsigned long long                      ULONG64;
typedef short                                   SHORT;
typedef unsigned short                          USHORT;
#define ULONG64_DEFINED
#define ALIGN_TYPE_DEFINED
#define ALIGN_TYPE                              ULONG64


/* Define the priority levels for ThreadX.  Legal values range
   from 32 to 1024 and MUST be evenly divisible by 32.  */

#ifndef TX_MAX_PRIORITIES
#define TX_MAX_PRIORITIES                       32
#endif

/* Define the minimum stack for a ThreadX thread on this processor. If the size supplied during
   thread creation is less than this value, the thread create call will return an error.  */

#ifndef TX_MINIMUM_STACK
#define TX_MINIMUM_STACK                        1024        /* Minimum stack size for this port  */
#endif

/* Define the system timer thread's default stack size and priority.  These are only applicable
   if TX_TIMER_PROCESS_IN_ISR is not defined.  */

#ifndef TX_TIMER_THREAD_STACK_SIZE
#define TX_TIMER_THREAD_STACK_SIZE              1024        /* Default timer thread stack size  */
#endif

#ifndef TX_TIMER_THREAD_PRIORITY
#define TX_TIMER_THREAD_PRIORITY                0           /* Default timer thread priority    */
#endif

/* Define various constants for the ThreadX RISC-V port.  */

#define TX_INT_DISABLE                          0x00000000  /* Disable interrupts value */
#define TX_INT_ENABLE                           0x00000008  /* Enable interrupt value   */

/* Define the clock source for trace event entry time stamp. */

#ifndef TX_TRACE_TIME_SOURCE
#define TX_TRACE_TIME_SOURCE                    ++_tx_trace_simulated_time
#endif
#ifndef TX_TRACE_TIME_MASK
#define TX_TRACE_TIME_MASK                      0xFFFFFFFFUL
#endif

/* Define the port specific options for the _tx_build_options variable.  */

#define TX_PORT_SPECIFIC_BUILD_OPTIONS          0

/* Define the in-line initialization constant.  */

#define TX_INLINE_INITIALIZATION

/* Stack checking.  */

#ifdef TX_ENABLE_STACK_CHECKING
#undef TX_DISABLE_STACK_FILLING
#endif

/* Define the TX_THREAD control block extensions for this port.  */

#define TX_THREAD_EXTENSION_0
#define TX_THREAD_EXTENSION_1
#define TX_THREAD_EXTENSION_2
#define TX_THREAD_EXTENSION_3

/* Define the port extensions of the remaining ThreadX objects.  */

#define TX_BLOCK_POOL_EXTENSION
#define TX_BYTE_POOL_EXTENSION
#define TX_EVENT_FLAGS_GROUP_EXTENSION
#define TX_MUTEX_EXTENSION
#define TX_QUEUE_EXTENSION
#define TX_SEMAPHORE_EXTENSION
#define TX_TIMER_EXTENSION

/* Define the user extension field of the thread control block.  */

#ifndef TX_THREAD_USER_EXTENSION
#define TX_THREAD_USER_EXTENSION
#endif

/* Define the macros for processing extensions in tx_thread_create, tx_thread_delete,
   tx_thread_shell_entry, and tx_thread_terminate.  */

#define TX_THREAD_CREATE_EXTENSION(thread_ptr)
#define TX_THREAD_DELETE_EXTENSION(thread_ptr)
#define TX_THREAD_COMPLETED_EXTENSION(thread_ptr)
#define TX_THREAD_TERMINATED_EXTENSION(thread_ptr)

/* Define the ThreadX object creation extensions for the remaining objects.  */

#define TX_BLOCK_POOL_CREATE_EXTENSION(pool_ptr)
#define TX_BYTE_POOL_CREATE_EXTENSION(pool_ptr)
#define TX_EVENT_FLAGS_GROUP_CREATE_EXTENSION(group_ptr)
#define TX_MUTEX_CREATE_EXTENSION(mutex_ptr)
#define TX_QUEUE_CREATE_EXTENSION(queue_ptr)
#define TX_SEMAPHORE_CREATE_EXTENSION(semaphore_ptr)
#define TX_TIMER_CREATE_EXTENSION(timer_ptr)

/* Define the ThreadX object deletion extensions for the remaining objects.  */

#define TX_BLOCK_POOL_DELETE_EXTENSION(pool_ptr)
#define TX_BYTE_POOL_DELETE_EXTENSION(pool_ptr)
#define TX_EVENT_FLAGS_GROUP_DELETE_EXTENSION(group_ptr)
#define TX_MUTEX_DELETE_EXTENSION(mutex_ptr)
#define TX_QUEUE_DELETE_EXTENSION(queue_ptr)
#define TX_SEMAPHORE_DELETE_EXTENSION(semaphore_ptr)
#define TX_TIMER_DELETE_EXTENSION(timer_ptr)


/* Define ThreadX interrupt lockout and restore macros for protection on
   access of critical kernel information.  */

#ifdef TX_DISABLE_INLINE

ULONG64                                         _tx_thread_interrupt_control(unsigned int new_posture);

#define TX_INTERRUPT_SAVE_AREA                  register ULONG64 interrupt_save;

#define TX_DISABLE                              interrupt_save =  _tx_thread_interrupt_control(TX_INT_DISABLE);
#define TX_RESTORE                              _tx_thread_interrupt_control(interrupt_save);

#else

#define TX_INTERRUPT_SAVE_AREA                  ULONG64 interrupt_save;
/* Atomically read mstatus into interrupt_save and clear bit 3 of mstatus.  */
#define TX_DISABLE                              {__asm__ ("csrrci %0, mstatus, 0x08" : "=r" (interrupt_save) : );};
/* We only care about mstatus.mie (bit 3), so mask interrupt_save and write to mstatus.  */
#define TX_RESTORE                              {register ULONG64 __tempmask = interrupt_save & 0x08; \
                                                __asm__ ("csrrs x0, mstatus, %0 \n\t" : : "r" (__tempmask) : );};

#endif


/* Define the interrupt lockout macros for each ThreadX object.  */

#define TX_BLOCK_POOL_DISABLE                   TX_DISABLE
#define TX_BYTE_POOL_DISABLE                    TX_DISABLE
#define TX_EVENT_FLAGS_GROUP_DISABLE            TX_DISABLE
#define TX_MUTEX_DISABLE                        TX_DISABLE
#define TX_QUEUE_DISABLE                        TX_DISABLE
#define TX_SEMAPHORE_DISABLE                    TX_DISABLE


/* Define the version ID of ThreadX.  This may be utilized by the application.  */

#ifdef TX_THREAD_INIT
CHAR                            _tx_version_id[] =
                                    "Copyright (c) 2024 Microsoft Corporation. * ThreadX RISC-V64/GNU Version 6.4.2 *";
#else
extern  CHAR                    _tx_version_id[];
#endif

#endif   /*not __ASSEMBLER__ */
#endif
