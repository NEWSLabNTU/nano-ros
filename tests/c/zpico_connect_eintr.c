// B46 -- a signal is not a failed connect.
//
// Drives the zenoh-pico fork's shared BSD-socket connect helper
// (`src/link/transport/tcp/tcp_connect_within.h`, used by tcp_posix.c and
// tcp_zephyr.c) against a real loopback listener, with `connect()` and
// `poll()` wrapped so that the FIRST call is interrupted (EINTR), as a
// FreeRTOS POSIX SIGALRM tick interrupts it. The real call still runs: the
// kernel goes on connecting, exactly as it does when a signal lands.
//
// Before the fix the helper returned -1 on that EINTR and a one-attempt open
// failed at once (the tier-2 nightly's freertos-posix zenoh red). The helper
// must now finish the same attempt and connect. A refused port must still
// fail, so a genuine failure is never hidden.

#define _GNU_SOURCE
#include <arpa/inet.h>
#include <errno.h>
#include <netinet/in.h>
#include <poll.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>
#include <sys/socket.h>
#include <time.h>
#include <unistd.h>

// The two clock calls the helper uses, from zenoh-pico's platform API.
typedef struct timespec z_clock_t;
__attribute__((unused)) static z_clock_t z_clock_now(void) {
    z_clock_t t;
    clock_gettime(CLOCK_MONOTONIC, &t);
    return t;
}
__attribute__((unused)) static unsigned long z_clock_elapsed_ms(z_clock_t *start) {
    z_clock_t now = z_clock_now();
    return (unsigned long)((now.tv_sec - start->tv_sec) * 1000L + (now.tv_nsec - start->tv_nsec) / 1000000L);
}

static int interrupt_connect;
static int interrupt_poll;

// Run the real call, then report the interruption once.
static int t_connect(int fd, const struct sockaddr *addr, socklen_t len) {
    int rc = connect(fd, addr, len);
    if (interrupt_connect) {
        interrupt_connect = 0;
        errno = EINTR;
        return -1;
    }
    return rc;
}
static int t_poll(struct pollfd *fds, nfds_t n, int timeout) {
    if (interrupt_poll) {
        interrupt_poll = 0;
        errno = EINTR;
        return -1;
    }
    return poll(fds, n, timeout);
}

#define connect t_connect
#define poll t_poll
#include "tcp_connect_within.h"
#undef connect
#undef poll

static int failures;
static void check(int ok, const char *what) {
    printf("  %s %s\n", ok ? "ok  " : "FAIL", what);
    if (!ok) {
        failures++;
    }
}

static int listener(struct sockaddr_in *addr) {
    int l = socket(AF_INET, SOCK_STREAM, 0);
    memset(addr, 0, sizeof(*addr));
    addr->sin_family = AF_INET;
    addr->sin_addr.s_addr = htonl(INADDR_LOOPBACK);
    socklen_t len = sizeof(*addr);
    if (l < 0 || bind(l, (struct sockaddr *)addr, len) != 0 || listen(l, 8) != 0 ||
        getsockname(l, (struct sockaddr *)addr, &len) != 0) {
        perror("listener");
        return -1;
    }
    return l;
}

static int connected(int fd) {
    struct sockaddr_in peer;
    socklen_t len = sizeof(peer);
    return getpeername(fd, (struct sockaddr *)&peer, &len) == 0;
}

static void attempt(const char *what, int32_t timeout_ms, int on_connect, int on_poll, const struct sockaddr_in *to,
                    int want_ok) {
    int fd = socket(AF_INET, SOCK_STREAM, 0);
    interrupt_connect = on_connect;
    interrupt_poll = on_poll;
    int rc = _z_tcp_connect_within(fd, (const struct sockaddr *)to, sizeof(*to), timeout_ms);
    int err = errno;
    if (want_ok) {
        check(rc == 0 && connected(fd), what);
        if (rc != 0) {
            printf("       returned %d, errno %d (%s)\n", rc, err, strerror(err));
        }
    } else {
        check(rc != 0, what);
    }
    close(fd);
}

int main(void) {
    struct sockaddr_in addr;
    int l = listener(&addr);
    if (l < 0) {
        return 2;
    }
    printf("zpico connect helper vs EINTR (loopback port %u):\n", ntohs(addr.sin_port));

    attempt("control: a plain one-attempt connect connects", -1, 0, 0, &addr, 1);
    attempt("blocking connect interrupted once still connects", -1, 1, 0, &addr, 1);
    attempt("bounded connect interrupted once still connects", 1000, 1, 0, &addr, 1);
    attempt("bounded connect whose poll is interrupted once still connects", 1000, 0, 1, &addr, 1);
    attempt("connect and poll both interrupted still connects", 1000, 1, 1, &addr, 1);

    // A port nobody listens on: the fix must not turn a refusal into success.
    struct sockaddr_in closed = addr;
    close(l);
    attempt("a refused port still fails (blocking)", -1, 0, 0, &closed, 0);
    attempt("a refused port still fails (bounded)", 1000, 0, 0, &closed, 0);

    printf("%d failure(s)\n", failures);
    return failures == 0 ? 0 : 1;
}
