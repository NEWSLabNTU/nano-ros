/**
 * nsos_rx_notify.c — the NetX Duo fork's per-socket receive notification
 * (`nx_bsd_socket_set_rx_notify`) over host sockets.
 *
 * phase-521 W6 (RFC-0113 D4). On real NetX Duo the IP thread calls the
 * notification when a packet is queued. Here every socket is a host kernel
 * fd, and the ThreadX wake object an executor parks on cannot be multiplexed
 * with one. So one ThreadX thread at the LOWEST priority blocks in a host
 * `poll()` over the fds that have a notification, and calls it when one turns
 * readable.
 *
 * Why a ThreadX thread and not a plain pthread: the notification signals a
 * ThreadX object (`tx_semaphore_put` through the executor's wake callback),
 * which only a ThreadX thread may call on this port (issue 1750: a foreign
 * thread entering a kernel service leaks the port's lock and wedges it).
 *
 * Why blocking in a host syscall is safe here, when it is not in zenoh's spin
 * (phase-297 W5): the ThreadX scheduler counts a thread blocked in a host call
 * as running, so it starves every LOWER-priority thread. This one is the
 * lowest, so it starves nothing; a higher thread made ready by the timer
 * interrupt preempts it through the port's suspend signal, which interrupts
 * the `poll()` (EINTR, retried). With nothing else ready the host CPU sleeps
 * in `poll()` instead of in the port's idle wait.
 *
 * Edge, not level: a notification fires once per readable fd and re-arms when
 * the application reads it (`nsos_rx_notify_rearm`, called from the shim's
 * receive calls), as NetX's fires once per queued packet. A level-triggered
 * fd left unread would otherwise call it in a loop at the lowest priority.
 *
 * Removal: the notification runs under `g_lock` and a removal takes it, so
 * once a removal returns the previous notification is never called again.
 */

#define _GNU_SOURCE
#include <errno.h>
#include <fcntl.h>
#include <poll.h>
#include <stdbool.h>
#include <stddef.h>
#include <unistd.h>

#include "tx_api.h"

#ifndef NSOS_RX_NOTIFY_MAX
#define NSOS_RX_NOTIFY_MAX 8
#endif
#if NSOS_RX_NOTIFY_MAX < 1
#error "NSOS_RX_NOTIFY_MAX must be >= 1: it sizes a C array (issue 1015)"
#endif

#ifndef NSOS_RX_NOTIFY_STACK
#define NSOS_RX_NOTIFY_STACK (16 * 1024)
#endif

typedef struct {
    int fd; /* -1 = free */
    void (*cb)(void *);
    void *arg;
    bool armed; /* false after firing, until the application reads */
} nsos_rx_entry_t;

static nsos_rx_entry_t g_entries[NSOS_RX_NOTIFY_MAX];
static TX_MUTEX g_lock;
static TX_THREAD g_thread;
static ULONG g_stack[NSOS_RX_NOTIFY_STACK / sizeof(ULONG)];
static int g_kick[2] = {-1, -1}; /* self-pipe: a table change ends the poll */
static bool g_started;

static void kick(void) {
    if (g_kick[1] >= 0) {
        char c = 0;
        ssize_t r = write(g_kick[1], &c, 1);
        (void) r; /* a full pipe already guarantees a wake */
    }
}

static void watcher(ULONG input) {
    (void) input;
    struct pollfd fds[NSOS_RX_NOTIFY_MAX + 1];
    for (;;) {
        int n = 0;
        fds[n].fd = g_kick[0];
        fds[n].events = POLLIN;
        n++;
        tx_mutex_get(&g_lock, TX_WAIT_FOREVER);
        for (int i = 0; i < NSOS_RX_NOTIFY_MAX; i++) {
            if (g_entries[i].fd >= 0 && g_entries[i].armed) {
                fds[n].fd = g_entries[i].fd;
                fds[n].events = POLLIN;
                n++;
            }
        }
        tx_mutex_put(&g_lock);

        int r = poll(fds, (nfds_t) n, -1);
        if (r < 0) {
            continue; /* EINTR: the port suspended this thread for a higher one */
        }
        if (fds[0].revents & POLLIN) {
            char buf[64];
            while (read(g_kick[0], buf, sizeof(buf)) > 0) {
            }
        }
        for (int k = 1; k < n; k++) {
            if ((fds[k].revents & (POLLIN | POLLHUP | POLLERR)) == 0) {
                continue;
            }
            tx_mutex_get(&g_lock, TX_WAIT_FOREVER);
            for (int i = 0; i < NSOS_RX_NOTIFY_MAX; i++) {
                if (g_entries[i].fd == fds[k].fd && g_entries[i].armed && g_entries[i].cb != NULL) {
                    g_entries[i].armed = false;
                    g_entries[i].cb(g_entries[i].arg);
                }
            }
            tx_mutex_put(&g_lock);
        }
    }
}

static int start_once(void) {
    if (g_started) {
        return 0;
    }
    for (int i = 0; i < NSOS_RX_NOTIFY_MAX; i++) {
        g_entries[i].fd = -1;
    }
    if (pipe2(g_kick, O_NONBLOCK | O_CLOEXEC) != 0) {
        return -1;
    }
    if (tx_mutex_create(&g_lock, "nsos-rx-notify", TX_INHERIT) != TX_SUCCESS) {
        return -1;
    }
    UINT prio = TX_MAX_PRIORITIES - 1;
    if (tx_thread_create(&g_thread, "nsos-rx-notify", watcher, 0, g_stack, sizeof(g_stack), prio,
                         prio, TX_NO_TIME_SLICE, TX_AUTO_START) != TX_SUCCESS) {
        tx_mutex_delete(&g_lock);
        return -1;
    }
    g_started = true;
    return 0;
}

int nx_bsd_socket_set_rx_notify(int sockID, void (*rx_notify)(void *arg), void *arg) {
    if (sockID < 0) {
        errno = EBADF;
        return -1;
    }
    if (rx_notify == NULL && !g_started) {
        return 0; /* nothing installed: removing (or probing) is a no-op */
    }
    if (start_once() != 0) {
        errno = ENOMEM;
        return -1;
    }
    tx_mutex_get(&g_lock, TX_WAIT_FOREVER);
    int slot = -1;
    for (int i = 0; i < NSOS_RX_NOTIFY_MAX; i++) {
        if (g_entries[i].fd == sockID) {
            slot = i;
            break;
        }
        if (slot < 0 && g_entries[i].fd < 0) {
            slot = i;
        }
    }
    if (rx_notify == NULL) {
        if (slot >= 0 && g_entries[slot].fd == sockID) {
            g_entries[slot].fd = -1;
            g_entries[slot].cb = NULL;
            g_entries[slot].arg = NULL;
        }
        tx_mutex_put(&g_lock);
        kick();
        return 0;
    }
    if (slot < 0) {
        tx_mutex_put(&g_lock);
        errno = ENOSPC;
        return -1;
    }
    g_entries[slot].fd = sockID;
    g_entries[slot].cb = rx_notify;
    g_entries[slot].arg = arg;
    g_entries[slot].armed = true;
    tx_mutex_put(&g_lock);
    /* Data already queued is not lost to the install: the watcher's next poll
     * sees the fd readable at once and calls it. */
    kick();
    return 0;
}

/* Called by the shim's receive calls: the application has read, so the next
 * arrival notifies again. */
void nsos_rx_notify_rearm(int sockID) {
    if (!g_started) {
        return;
    }
    bool kicked = false;
    tx_mutex_get(&g_lock, TX_WAIT_FOREVER);
    for (int i = 0; i < NSOS_RX_NOTIFY_MAX; i++) {
        if (g_entries[i].fd == sockID && !g_entries[i].armed) {
            g_entries[i].armed = true;
            kicked = true;
        }
    }
    tx_mutex_put(&g_lock);
    if (kicked) {
        kick();
    }
}

/* A closed fd number is reused by the next socket: drop its notification. */
void nsos_rx_notify_forget(int sockID) {
    if (!g_started) {
        return;
    }
    tx_mutex_get(&g_lock, TX_WAIT_FOREVER);
    for (int i = 0; i < NSOS_RX_NOTIFY_MAX; i++) {
        if (g_entries[i].fd == sockID) {
            g_entries[i].fd = -1;
            g_entries[i].cb = NULL;
            g_entries[i].arg = NULL;
        }
    }
    tx_mutex_put(&g_lock);
    kick();
}
