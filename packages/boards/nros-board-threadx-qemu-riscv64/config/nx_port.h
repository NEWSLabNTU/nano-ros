/*
 * NetX Duo port header for RISC-V 64-bit bare-metal (QEMU virt).
 *
 * Based on the Linux/GNU port (netxduo/ports/linux/gnu/inc/nx_port.h).
 * Adapted for bare-metal: uses picolibc instead of glibc.
 */

#ifndef NX_PORT_H
#define NX_PORT_H

#ifdef NX_INCLUDE_USER_DEFINE_FILE
#include "nx_user.h"
#endif

/* RISC-V is little endian. */
#define NX_LITTLE_ENDIAN    1

#include <stdio.h>
#include <string.h>

/* Define various constants for the port. */
#ifndef NX_IP_PERIODIC_RATE
#ifdef TX_TIMER_TICKS_PER_SECOND
#define NX_IP_PERIODIC_RATE         TX_TIMER_TICKS_PER_SECOND
#else
#define NX_IP_PERIODIC_RATE         100
#endif
#endif

/* Define macros that swap the endian for little endian ports.
 * Matches the official Cortex-M7/GNU port (netxduo/ports/cortex_m7/gnu/inc/nx_port.h).
 *
 * NetX Duo stores IP addresses in HOST byte order internally.
 * NX_CHANGE_ULONG_ENDIAN converts packet header fields between host
 * and network byte order. htonl/ntohl are real byte-swaps used by the
 * BSD socket layer (nxd_bsd.c) to convert between BSD sockaddr (network
 * byte order) and NetX internal structures (host byte order).
 *
 * picolibc's <machine/endian.h> also defines htonl as __bswap32 on LE,
 * which is functionally equivalent. The #ifndef guards below allow
 * picolibc's definition to take precedence when already defined. */
#ifdef NX_LITTLE_ENDIAN
#define NX_CHANGE_ULONG_ENDIAN(arg)       (arg) = __builtin_bswap32(arg)
#define NX_CHANGE_USHORT_ENDIAN(arg)      (arg) = __builtin_bswap16(arg)

#ifndef htonl
#define htonl(val)  __builtin_bswap32(val)
#endif
#ifndef ntohl
#define ntohl(val)  __builtin_bswap32(val)
#endif
#ifndef htons
#define htons(val)  __builtin_bswap16(val)
#endif
#ifndef ntohs
#define ntohs(val)  __builtin_bswap16(val)
#endif

#else /* big endian */
#define NX_CHANGE_ULONG_ENDIAN(a)
#define NX_CHANGE_USHORT_ENDIAN(a)

#ifndef htons
#define htons(val) (val)
#endif
#ifndef ntohs
#define ntohs(val) (val)
#endif
#ifndef ntohl
#define ntohl(val) (val)
#endif
#ifndef htonl
#define htonl(val) (val)
#endif
#endif

/* Define several macros for the error checking shell in NetX. */
#ifndef TX_TIMER_PROCESS_IN_ISR

#define NX_CALLER_CHECKING_EXTERNS          extern  TX_THREAD           *_tx_thread_current_ptr; \
                                            extern  TX_THREAD           _tx_timer_thread; \
                                            extern  volatile ULONG      _tx_thread_system_state;

#define NX_THREADS_ONLY_CALLER_CHECKING     if ((_tx_thread_system_state) || \
                                                (_tx_thread_current_ptr == TX_NULL) || \
                                                (_tx_thread_current_ptr == &_tx_timer_thread)) \
                                                return(NX_CALLER_ERROR);

#define NX_INIT_AND_THREADS_CALLER_CHECKING if (((_tx_thread_system_state) && (_tx_thread_system_state < ((ULONG) 0xF0F0F0F0))) || \
                                                (_tx_thread_current_ptr == &_tx_timer_thread)) \
                                                return(NX_CALLER_ERROR);

#define NX_NOT_ISR_CALLER_CHECKING          if ((_tx_thread_system_state) && (_tx_thread_system_state < ((ULONG) 0xF0F0F0F0))) \
                                                return(NX_CALLER_ERROR);

#define NX_THREAD_WAIT_CALLER_CHECKING      if ((wait_option) && \
                                               ((_tx_thread_current_ptr == NX_NULL) || (_tx_thread_system_state) || (_tx_thread_current_ptr == &_tx_timer_thread))) \
                                            return(NX_CALLER_ERROR);

#else

#define NX_CALLER_CHECKING_EXTERNS          extern  TX_THREAD           *_tx_thread_current_ptr; \
                                            extern  volatile ULONG      _tx_thread_system_state;

#define NX_THREADS_ONLY_CALLER_CHECKING     if ((_tx_thread_system_state) || \
                                                (_tx_thread_current_ptr == TX_NULL)) \
                                                return(NX_CALLER_ERROR);

#define NX_INIT_AND_THREADS_CALLER_CHECKING if (((_tx_thread_system_state) && (_tx_thread_system_state < ((ULONG) 0xF0F0F0F0)))) \
                                                return(NX_CALLER_ERROR);

#define NX_NOT_ISR_CALLER_CHECKING          if ((_tx_thread_system_state) && (_tx_thread_system_state < ((ULONG) 0xF0F0F0F0))) \
                                                return(NX_CALLER_ERROR);

#define NX_THREAD_WAIT_CALLER_CHECKING      if ((wait_option) && \
                                               ((_tx_thread_current_ptr == NX_NULL) || (_tx_thread_system_state))) \
                                            return(NX_CALLER_ERROR);

#endif

/* Define the version ID of NetX. */
#ifdef NX_SYSTEM_INIT
CHAR                            _nx_version_id[] =
                                    "Copyright (c) 2024 Microsoft Corporation.  *  NetX Duo RISC-V64/GNU Version 6.4.1 *";
#else
extern  CHAR                    _nx_version_id[];
#endif

#endif
