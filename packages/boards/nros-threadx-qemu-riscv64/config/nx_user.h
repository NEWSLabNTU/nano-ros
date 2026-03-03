/*
 * nx_user.h — NetX Duo user configuration for QEMU RISC-V 64-bit virt
 *
 * Included by NetX Duo when NX_INCLUDE_USER_DEFINE_FILE is defined.
 * Enables the BSD socket compatibility layer used by zenoh-pico's
 * ThreadX network transport.
 */

#ifndef NX_USER_H
#define NX_USER_H

/* BSD socket layer.
 * NX_BSD_ENABLE_NATIVE_API: use nx_bsd_* prefixed names to avoid
 * conflicting with any system POSIX headers.
 * NX_ENABLE_EXTENDED_NOTIFY_SUPPORT: required for BSD socket layer. */
#define NX_BSD_ENABLE
#define NX_BSD_ENABLE_NATIVE_API
#define NX_BSD_MAX_SOCKETS          16
#define NX_BSD_TIMEOUT              (20 * NX_IP_PERIODIC_RATE)
#define NX_ENABLE_EXTENDED_NOTIFY_SUPPORT

/* Network parameters */
#define NX_PHYSICAL_HEADER          16
#define NX_MAX_PORT                 65535

/* Extended notify — required by BSD socket layer */
#define NX_ENABLE_EXTENDED_NOTIFY_SUPPORT

/* Deferred processing — required for virtio-net interrupt-driven RX */
#define NX_DRIVER_DEFERRED_PROCESSING

#endif /* NX_USER_H */
