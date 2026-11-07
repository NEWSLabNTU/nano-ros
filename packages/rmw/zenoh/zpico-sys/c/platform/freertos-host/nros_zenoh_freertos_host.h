/*
 * zenoh-pico platform header for FreeRTOS on a HOST-owned network stack
 * (issue 2342, phase-546, RFC-0109 D2).
 *
 * The FreeRTOS POSIX port runs the kernel inside a host process: its tasks
 * are FreeRTOS tasks (each one a host pthread the port schedules), and its
 * network is the host kernel's own BSD sockets. zenoh-pico ships no platform
 * for that pair. `freertos/lwip.h` has the FreeRTOS half and `#include`s
 * `lwip/sockets.h`; `unix.h` has the socket half and takes pthreads for the
 * system half. So this header is the two halves side by side, each copied
 * from the vendor header it belongs to:
 *
 *   - the SYSTEM types (task, mutex, condvar, clock) are `freertos/lwip.h`'s,
 *     because `src/system/freertos/system.c` implements them;
 *   - the SOCKET types are `unix.h`'s (`{ int _fd; }`), because
 *     `src/link/transport/{tcp,udp}/*_posix.c` and `src/system/unix/network.c`
 *     implement them.
 *
 * It is reached through zenoh-pico's own custom-platform hook,
 * `ZP_SYSTEM_PLATFORM_HEADER`, which `system/common/platform.h` tests before
 * every built-in arm. The zenoh port row (`nros-rmw.toml`, `[port.freertos]`)
 * defines it when the board's `host_stack` capability is true, i.e. when the
 * board says `[board.net] stack = "host"`. Every TU in the archive, the zpico
 * shim and the ABI size probe included, gets the same define, so they agree on
 * the socket type (issue 0135).
 *
 * Keep the two halves in step with the vendor headers they copy: a field
 * added to `freertos/lwip.h`'s `_z_task_t` or to `unix.h`'s
 * `_z_sys_net_socket_t` has to be added here too.
 */

#ifndef NROS_ZENOH_FREERTOS_HOST_H
#define NROS_ZENOH_FREERTOS_HOST_H

#include <netdb.h>
#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>
#include <sys/time.h>
#include <time.h>

#include "FreeRTOS.h"
#include "semphr.h"

/* The socket links are the POSIX ones. `system/common/platform.h` derives
 * this marker from `ZENOH_LINUX` and friends, which this image must not
 * claim (issue 1039: asserting a platform to reach one of its capabilities
 * makes the image claim it for every other guard). It is defined here, after
 * the marker block has run and before any link TU tests it. */
#ifndef ZP_PLATFORM_SOCKET_POSIX
#define ZP_PLATFORM_SOCKET_POSIX 1
#endif

/* The nano-ros zpico shim keys its FreeRTOS KERNEL arms (semaphores, task
 * attributes, stack reporting) on this, beside `ZENOH_FREERTOS_LWIP`. */
#define NROS_ZENOH_FREERTOS_HOST 1

#ifdef __cplusplus
extern "C" {
#endif

#if Z_FEATURE_MULTI_THREAD == 1
#include "event_groups.h"

typedef struct {
    const char *name;
    UBaseType_t priority;
    size_t stack_depth;
#if (configSUPPORT_STATIC_ALLOCATION == 1)
    bool static_allocation;
    StackType_t *stack_buffer;
    StaticTask_t *task_buffer;
#endif /* SUPPORT_STATIC_ALLOCATION */
} z_task_attr_t;

typedef struct {
    TaskHandle_t handle;
    EventGroupHandle_t join_event;
    void *(*fun)(void *);
    void *arg;
#if (configSUPPORT_STATIC_ALLOCATION == 1)
    StaticEventGroup_t join_event_buffer;
#endif /* SUPPORT_STATIC_ALLOCATION */
} _z_task_t;

typedef struct {
    SemaphoreHandle_t handle;
#if (configSUPPORT_STATIC_ALLOCATION == 1)
    StaticSemaphore_t buffer;
#endif /* SUPPORT_STATIC_ALLOCATION */
} _z_mutex_t;

typedef _z_mutex_t _z_mutex_rec_t;

typedef struct {
    SemaphoreHandle_t mutex;
    SemaphoreHandle_t sem;
    int waiters;
#if (configSUPPORT_STATIC_ALLOCATION == 1)
    StaticSemaphore_t mutex_buffer;
    StaticSemaphore_t sem_buffer;
#endif /* SUPPORT_STATIC_ALLOCATION */
} _z_condvar_t;

typedef TaskHandle_t _z_task_id_t;
#endif  // Z_MULTI_THREAD == 1

typedef TickType_t z_clock_t;
typedef struct timeval z_time_t;

typedef struct {
    union {
#if Z_FEATURE_LINK_TCP == 1 || Z_FEATURE_LINK_UDP_MULTICAST == 1 || Z_FEATURE_LINK_UDP_UNICAST == 1
        int _fd;
#endif
    };
} _z_sys_net_socket_t;

typedef struct {
    union {
#if Z_FEATURE_LINK_TCP == 1 || Z_FEATURE_LINK_UDP_MULTICAST == 1 || Z_FEATURE_LINK_UDP_UNICAST == 1
        struct addrinfo *_iptcp;
#endif
    };
} _z_sys_net_endpoint_t;

#if Z_FEATURE_LINK_TLS == 1
#error "TLS is not supported on the FreeRTOS host-socket port of zenoh-pico"
#endif

#if Z_FEATURE_LINK_SERIAL == 1
#error "Serial is not supported on the FreeRTOS host-socket port of zenoh-pico"
#endif

#if Z_FEATURE_RAWETH_TRANSPORT == 1
#error "Raw ethernet is not supported on the FreeRTOS host-socket port of zenoh-pico"
#endif

#ifdef __cplusplus
}
#endif

#endif /* NROS_ZENOH_FREERTOS_HOST_H */
