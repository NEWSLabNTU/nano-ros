/**
 * @file nros_platform_zephyr_shims.c
 * @brief Real-symbol wrappers around Zephyr kernel inlines.
 *
 * Several Zephyr APIs are declared `static inline` in headers
 * (`k_msleep`, `k_uptime_get`, `sys_rand_get`, etc.) and have no exported
 * symbol. Rust FFI can only call real symbols, so we wrap them here —
 * this TU is compiled by the Zephyr module build and exports the real
 * functions that `nros-platform-zephyr` declares as `extern "C"`.
 *
 * Real-function Zephyr APIs (`k_malloc`, `k_free`, `k_usleep`,
 * `sys_rand32_get`, `pthread_*`) are called directly from Rust and do
 * not need wrappers.
 */

#include <stddef.h>
#include <stdint.h>

#include <zephyr/kernel.h>
#include <zephyr/random/random.h>

/* ── Clock / sleep / random (no POSIX dependency) ───────────────────── */

int64_t nros_zephyr_uptime_ms(void) {
    return k_uptime_get();
}

int32_t nros_zephyr_msleep(int32_t ms) {
    return k_msleep(ms);
}

void nros_zephyr_rand_fill(void *dst, size_t len) {
    sys_rand_get(dst, len);
}

/* Phase 77.22: cooperative yield. k_yield is declared `static inline`
 * in <zephyr/kernel.h>, so wrap it here to get a real callable symbol.
 */
void nros_zephyr_yield(void) {
    k_yield();
}

/* ── BSD socket wrappers ────────────────────────────────────────────
 *
 * On native_sim, glibc's getaddrinfo/freeaddrinfo symbols override
 * Zephyr's POSIX wrappers. The glibc versions return POSIX addrinfo
 * layout (ai_flags first), but Zephyr's zsock_addrinfo has ai_next
 * first. Use Zephyr's zsock_* API directly to avoid the collision.
 */

#include <zephyr/net/socket.h>

int nros_zephyr_getaddrinfo(const char *node, const char *service,
                            const struct zsock_addrinfo *hints,
                            struct zsock_addrinfo **res) {
    return zsock_getaddrinfo(node, service, hints, res);
}

void nros_zephyr_freeaddrinfo(struct zsock_addrinfo *res) {
    zsock_freeaddrinfo(res);
}

int nros_zephyr_socket(int family, int type, int proto) {
    return zsock_socket(family, type, proto);
}

int nros_zephyr_close(int fd) {
    return zsock_close(fd);
}

int nros_zephyr_connect(int fd, const struct sockaddr *addr, socklen_t addrlen) {
    return zsock_connect(fd, addr, addrlen);
}

int nros_zephyr_bind(int fd, const struct sockaddr *addr, socklen_t addrlen) {
    return zsock_bind(fd, addr, addrlen);
}

int nros_zephyr_listen(int fd, int backlog) {
    return zsock_listen(fd, backlog);
}

int nros_zephyr_accept(int fd, struct sockaddr *addr, socklen_t *addrlen) {
    return zsock_accept(fd, addr, addrlen);
}

int nros_zephyr_shutdown(int fd, int how) {
    return zsock_shutdown(fd, how);
}

int nros_zephyr_setsockopt(int fd, int level, int optname,
                           const void *optval, socklen_t optlen) {
    return zsock_setsockopt(fd, level, optname, optval, optlen);
}

int nros_zephyr_fcntl(int fd, int cmd, int arg) {
    return zsock_fcntl(fd, cmd, arg);
}

ssize_t nros_zephyr_recv(int fd, void *buf, size_t len, int flags) {
    return zsock_recv(fd, buf, len, flags);
}

ssize_t nros_zephyr_recvfrom(int fd, void *buf, size_t len, int flags,
                             struct sockaddr *src_addr, socklen_t *addrlen) {
    return zsock_recvfrom(fd, buf, len, flags, src_addr, addrlen);
}

ssize_t nros_zephyr_send(int fd, const void *buf, size_t len, int flags) {
    return zsock_send(fd, buf, len, flags);
}

ssize_t nros_zephyr_sendto(int fd, const void *buf, size_t len, int flags,
                           const struct sockaddr *dest_addr, socklen_t addrlen) {
    return zsock_sendto(fd, buf, len, flags, dest_addr, addrlen);
}

/* ── Thread creation with Zephyr-managed stacks ─────────────────────
 *
 * Requires CONFIG_POSIX_API (or equivalent CONFIG_PTHREAD).
 * Only compiled when POSIX threads are available.
 */

#if defined(CONFIG_POSIX_API) || defined(CONFIG_PTHREAD)

#include <zephyr/posix/pthread.h>

#ifndef NROS_ZEPHYR_MAX_THREADS
#define NROS_ZEPHYR_MAX_THREADS 8
#endif

#ifndef NROS_ZEPHYR_STACK_SIZE
#define NROS_ZEPHYR_STACK_SIZE CONFIG_MAIN_STACK_SIZE
#endif

K_THREAD_STACK_ARRAY_DEFINE(nros_thread_stacks, NROS_ZEPHYR_MAX_THREADS,
                            NROS_ZEPHYR_STACK_SIZE);
static int nros_thread_index;

int nros_zephyr_task_create(pthread_t *thread,
                            void *(*entry)(void *),
                            void *arg) {
    if (nros_thread_index >= NROS_ZEPHYR_MAX_THREADS) {
        return -1; /* no more stack slots */
    }

    pthread_attr_t attr;
    (void)pthread_attr_init(&attr);
    (void)pthread_attr_setstack(&attr,
                                &nros_thread_stacks[nros_thread_index++],
                                NROS_ZEPHYR_STACK_SIZE);

    int ret = pthread_create(thread, &attr, entry, arg);
    (void)pthread_attr_destroy(&attr);
    return ret;
}

#endif /* CONFIG_POSIX_API || CONFIG_PTHREAD */
