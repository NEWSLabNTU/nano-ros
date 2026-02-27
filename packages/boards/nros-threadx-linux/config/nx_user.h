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

/* Network parameters */
#define NX_PHYSICAL_HEADER          16
#define NX_MAX_PORT                 65535

/* Default interface name for the Linux TAP driver.
 * The actual name is set at runtime via nx_linux_set_interface_name(). */
#define NX_LINUX_INTERFACE_NAME     "tap-qemu0"

#endif /* NX_USER_H */
