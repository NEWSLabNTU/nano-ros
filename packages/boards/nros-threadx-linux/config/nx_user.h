/*
 * nx_user.h — NetX Duo user configuration for Linux simulation
 *
 * Included by NetX Duo when NX_INCLUDE_USER_DEFINE_FILE is defined.
 * Enables the BSD socket compatibility layer used by zenoh-pico's
 * ThreadX network transport.
 */

#ifndef NX_USER_H
#define NX_USER_H

/* BSD socket layer.
 * NX_BSD_ENABLE_NATIVE_API: use nx_bsd_* prefixed names to avoid
 * conflicting with Linux system POSIX socket headers. */
#define NX_BSD_ENABLE
#define NX_BSD_ENABLE_NATIVE_API
#define NX_BSD_MAX_SOCKETS          16
#define NX_BSD_TIMEOUT              (20 * NX_IP_PERIODIC_RATE)

/* 64-bit pointer support for NetX Duo on x86_64.
 * On x86_64, ULONG is 32-bit but pointers are 64-bit. NetX Duo passes
 * IP instance pointers as ULONG thread/timer entry arguments, which
 * truncates the upper 32 bits. These macros store the full 64-bit
 * pointer in ThreadX thread/timer extension fields and retrieve it
 * from the current thread/expired timer context. */
#if defined(__x86_64__) && __x86_64__

#include "tx_api.h"

#define NX_THREAD_EXTENSION_PTR_SET(thread_ptr, module_ptr)               \
    (thread_ptr)->tx_thread_extension_ptr = (VOID *)(module_ptr);

#define NX_THREAD_EXTENSION_PTR_GET(module_ptr, module_type, thread_input) \
    {                                                                      \
        TX_PARAMETER_NOT_USED(thread_input);                               \
        TX_THREAD *_current_thread_ptr;                                    \
        _current_thread_ptr = tx_thread_identify();                        \
        while (1)                                                          \
        {                                                                  \
            if (_current_thread_ptr -> tx_thread_extension_ptr)            \
            {                                                              \
                (module_ptr) = (module_type *)                             \
                    (_current_thread_ptr -> tx_thread_extension_ptr);      \
                break;                                                     \
            }                                                              \
            tx_thread_sleep(1);                                            \
        }                                                                  \
    }

extern TX_TIMER_INTERNAL *_tx_timer_expired_timer_ptr;

#define NX_TIMER_EXTENSION_PTR_SET(timer_ptr, module_ptr)                 \
    {                                                                      \
        TX_TIMER *_timer_ptr;                                              \
        _timer_ptr = (TX_TIMER *)(timer_ptr);                              \
        (_timer_ptr->tx_timer_internal.tx_timer_internal_extension_ptr)    \
            = (VOID *)(module_ptr);                                        \
    }

#define NX_TIMER_EXTENSION_PTR_GET(module_ptr, module_type, timer_input)  \
    {                                                                      \
        TX_PARAMETER_NOT_USED(timer_input);                                \
        if (!_tx_timer_expired_timer_ptr ->                                \
            tx_timer_internal_extension_ptr)                               \
            return;                                                        \
        (module_ptr) = (module_type *)                                     \
            (_tx_timer_expired_timer_ptr ->                                \
             tx_timer_internal_extension_ptr);                             \
    }

#endif /* __x86_64__ */

/* Required for BSD socket layer (NX_BSD_ENABLE).
 * Without this, nx_bsd_initialize() returns NX_BSD_ENVIRONMENT_ERROR. */
#define NX_ENABLE_EXTENDED_NOTIFY_SUPPORT

/* Network parameters */
#define NX_PHYSICAL_HEADER          16
#define NX_MAX_PORT                 65535

/* Default interface name for the TAP driver.
 * The actual name is set at runtime via nx_tap_set_interface_name(). */
#define NX_LINUX_INTERFACE_NAME     "tap-tx0"

#endif /* NX_USER_H */
