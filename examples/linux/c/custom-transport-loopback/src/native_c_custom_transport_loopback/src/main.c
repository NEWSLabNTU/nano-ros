/// @file main.c
/// @brief Phase 115.F — C-side custom-transport demo.
///
/// Installs an `nros_transport_ops_t` via `rclc_set_custom_transport`, opens
/// an nros session over the `custom/loopback` locator, and exercises the four
/// callbacks: `open` once at session bring-up, `write` on every outgoing wire
/// frame, `read` on every incoming poll, `close` at teardown.
///
/// The callbacks carry zenoh-pico's wire bytes over a TCP connection the
/// APPLICATION opens to a zenoh router (`NROS_CUSTOM_TCP_TARGET`, `host:port`,
/// default `127.0.0.1:7447`). The session needs a peer that answers its
/// handshake: a ring buffer that echoes the image's own frames back hands
/// zenoh-pico its own `InitSyn` where it waits for an `InitAck`, and the
/// session refuses (issue 2452). Swap the socket for a UART, a USB CDC
/// endpoint or a shared-memory ring with something listening on the far end,
/// and nothing else in this file changes.
///
/// Pass / fail criteria (exit 0 only when all hold):
///  1. `rclc_set_custom_transport(&ops)` returns `RCL_RET_OK`.
///  2. The session opens through the transport (`open` fired, and the
///     handshake's reply arrived through `read`).
///  3. `write_count >= 1` after publishing.
///  4. `read_count >= 1` after the executor's spin.
///  5. `close_count >= 1` after `rclc_support_fini` /
///     `rclc_set_custom_transport(NULL)`.

#include <errno.h>
#include <netdb.h>
#include <netinet/in.h>
#include <netinet/tcp.h>
#include <poll.h>
#include <signal.h>
#include <stdatomic.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/socket.h>
#include <sys/types.h>
#include <time.h>
#include <unistd.h>

#include <rclc/rclc.h>

#include "std_msgs.h"

/* --------------------------------------------------------------
 * The transport: zenoh wire bytes over a TCP socket this program owns
 * -------------------------------------------------------------- */

#define DEFAULT_TCP_TARGET "127.0.0.1:7447"

typedef struct {
    int fd;
    atomic_int open_count;
    atomic_int close_count;
    atomic_int write_count;
    atomic_int read_count;
} link_state_t;

static link_state_t g_link = {.fd = -1};

/* Split `host:port` and connect. Returns the socket, or -1. */
static int connect_tcp_target(const char* target) {
    char host[256];
    const char* colon = strrchr(target, ':');
    if (colon == NULL || colon == target || (size_t)(colon - target) >= sizeof(host)) {
        fprintf(stderr, "custom link: `%s` is not host:port\n", target);
        return -1;
    }
    memcpy(host, target, (size_t)(colon - target));
    host[colon - target] = '\0';

    struct addrinfo hints;
    memset(&hints, 0, sizeof(hints));
    hints.ai_family = AF_UNSPEC;
    hints.ai_socktype = SOCK_STREAM;
    struct addrinfo* res = NULL;
    int gai = getaddrinfo(host, colon + 1, &hints, &res);
    if (gai != 0) {
        fprintf(stderr, "custom link: resolve %s: %s\n", target, gai_strerror(gai));
        return -1;
    }
    int fd = -1;
    for (struct addrinfo* ai = res; ai != NULL; ai = ai->ai_next) {
        fd = socket(ai->ai_family, ai->ai_socktype, ai->ai_protocol);
        if (fd < 0) {
            continue;
        }
        if (connect(fd, ai->ai_addr, ai->ai_addrlen) == 0) {
            break;
        }
        close(fd);
        fd = -1;
    }
    freeaddrinfo(res);
    if (fd < 0) {
        fprintf(stderr, "custom link: connect %s: %s\n", target, strerror(errno));
        return -1;
    }
    int one = 1;
    (void)setsockopt(fd, IPPROTO_TCP, TCP_NODELAY, &one, sizeof(one));
    return fd;
}

static rcl_ret_t link_open(void* user_data, const void* params) {
    (void)user_data;
    (void)params;
    atomic_fetch_add(&g_link.open_count, 1);
    const char* target = getenv("NROS_CUSTOM_TCP_TARGET");
    if (target == NULL || target[0] == '\0') {
        target = DEFAULT_TCP_TARGET;
    }
    g_link.fd = connect_tcp_target(target);
    return g_link.fd < 0 ? RCL_RET_ERROR : RCL_RET_OK;
}

static void link_close(void* user_data) {
    (void)user_data;
    atomic_fetch_add(&g_link.close_count, 1);
    if (g_link.fd >= 0) {
        close(g_link.fd);
        g_link.fd = -1;
    }
}

static rcl_ret_t link_write(void* user_data, const uint8_t* buf, size_t len) {
    (void)user_data;
    atomic_fetch_add(&g_link.write_count, 1);
    while (len > 0) {
        ssize_t n = send(g_link.fd, buf, len, MSG_NOSIGNAL);
        if (n < 0) {
            if (errno == EINTR) {
                continue;
            }
            return RCL_RET_ERROR;
        }
        buf += n;
        len -= (size_t)n;
    }
    return RCL_RET_OK;
}

/* Up to `len` bytes, waiting at most `timeout_ms`. 0 = nothing arrived this
 * tick; negative = the link is gone. */
static int32_t link_read(void* user_data, uint8_t* buf, size_t len, uint32_t timeout_ms) {
    (void)user_data;
    atomic_fetch_add(&g_link.read_count, 1);
    if (buf == NULL || len == 0) {
        return 0;
    }
    /* zenoh-pico's receive loop asks with 0 ms and leaves the bound to the
     * transport; wait at least 1 ms so that loop does not spin a core. */
    int wait_ms = timeout_ms == 0 ? 1 : (int)timeout_ms;
    struct pollfd pfd = {.fd = g_link.fd, .events = POLLIN, .revents = 0};
    int ready = poll(&pfd, 1, wait_ms);
    if (ready == 0 || (ready < 0 && errno == EINTR)) {
        return 0;
    }
    if (ready < 0) {
        return -1;
    }
    ssize_t n = recv(g_link.fd, buf, len, 0);
    if (n > 0) {
        return (int32_t)n;
    }
    if (n < 0 && (errno == EINTR || errno == EAGAIN)) {
        return 0;
    }
    return -1; /* peer closed, or a socket error */
}

/* --------------------------------------------------------------
 * Application state (static .bss — no malloc)
 * -------------------------------------------------------------- */

typedef struct {
    rcl_publisher_t* publisher;
    std_msgs__msg__Int32 message;
    int32_t tick;
} demo_ctx_t;

static struct {
    rcl_clock_t clock;
    rclc_support_t support;
    rcl_node_t node;
    rcl_publisher_t publisher;
    rcl_timer_t timer;
    rclc_executor_t executor;
    demo_ctx_t demo_ctx;
} app;

static volatile sig_atomic_t g_running = 1;
static rclc_executor_t* g_executor = NULL;

static void signal_handler(int signum) {
    (void)signum;
    g_running = 0;
    if (g_executor != NULL) {
        rclc_executor_cancel(g_executor);
    }
}

static void timer_callback(rcl_timer_t* timer, int64_t last_call_time) {
    (void)timer;
    demo_ctx_t* ctx = &app.demo_ctx;
    (void)last_call_time;
    ctx->message.data = ctx->tick++;
    // Phase 212.M native/c sweep — replace the never-defined
    // ~~`nros_publisher_publish`~~ symbol with the canonical typed
    // `rcl_publish` (phase-492 W8, as the talker does).
    if (rcl_publish(ctx->publisher, &ctx->message, NULL) == RCL_RET_OK) {
        printf("Published: %d\n", (int)ctx->message.data);
    }
}

/* --------------------------------------------------------------
 * main
 * -------------------------------------------------------------- */

int main(int argc, char** argv) {
    (void)argc;
    (void)argv;

    // Line-buffer stdout: glibc full-buffers non-tty stdout, so when piped to
    // a test harness each line must flush on its newline (Phase 177.34).
#ifdef _IOLBF /* absent on the bare-metal riscv64-threadx libc */
    setvbuf(stdout, NULL, _IOLBF, 0);
#endif

    memset(&app, 0, sizeof(app));

    /* 1. Install the custom transport BEFORE session bring-up. */
    const nros_transport_ops_t ops = {
        .abi_version = NROS_TRANSPORT_OPS_ABI_VERSION_V1,
        ._reserved = 0,
        .user_data = NULL,
        .open = link_open,
        .close = link_close,
        .write = link_write,
        .read = link_read,
    };
    rcl_ret_t set_rc = rclc_set_custom_transport(&ops);
    if (set_rc != RCL_RET_OK) {
        fprintf(stderr, "rclc_set_custom_transport failed: %d\n", set_rc);
        return 1;
    }
    fprintf(stdout, "custom link: transport installed\n");

    /* 2. Spin up the nros stack on the `custom/` locator: zenoh-pico routes
     *    every wire frame through the ops above. The bringup's `system.toml`
     *    names the same locator, which is what compiles zenoh-pico's custom
     *    link into this image (issue 2452). */
    (void)rcl_clock_init(RCL_SYSTEM_TIME, &app.clock, NULL);
    /* phase-426 W4 — the ~~`nros_parameter_server_t`~~ that stood here is GONE. It
     * was initialised and never touched again: no declare, no get, no set. A
     * caller-storage store is a SECOND store (`ros2 param` reads the
     * executor's), so the one thing it demonstrated to a reader copying this
     * file out was the defect this phase removes. `examples/linux/c/
     * parameters` is where a C image's parameters are shown. */
    RCLC_CHECK_RET(rclc_support_init_with_locator(&app.support, "custom/loopback", 0), 1);
    RCLC_CHECK_RET(rclc_node_init_default(&app.node, "loopback_demo", "/", &app.support), 1);

    RCLC_CHECK_RET(rclc_publisher_init_default(&app.publisher, &app.node,
                                               ROSIDL_GET_MSG_TYPE_SUPPORT(std_msgs, msg, Int32),
                                               "/loopback_chatter"),
                   1);

    app.demo_ctx.publisher = &app.publisher;
    app.demo_ctx.tick = 0;
    memset(&app.demo_ctx.message, 0, sizeof(app.demo_ctx.message));

    RCLC_CHECK_RET(rclc_timer_init_default(&app.timer, &app.support,
                                           500ULL * 1000 * 1000 /* 500ms */, timer_callback),
                   1);
    RCLC_CHECK_RET(rclc_executor_init(&app.executor, &app.support.context, 4, NULL), 1);
    RCLC_CHECK_RET(rclc_executor_add_timer(&app.executor, &app.timer), 1);
    g_executor = &app.executor;

    signal(SIGINT, signal_handler);
    signal(SIGTERM, signal_handler);

    fprintf(stdout, "custom link: spinning for ~3 seconds (Ctrl-C to stop sooner)\n");
    struct timespec deadline;
    clock_gettime(CLOCK_MONOTONIC, &deadline);
    deadline.tv_sec += 3;
    while (g_running) {
        struct timespec now;
        clock_gettime(CLOCK_MONOTONIC, &now);
        if (now.tv_sec > deadline.tv_sec ||
            (now.tv_sec == deadline.tv_sec && now.tv_nsec >= deadline.tv_nsec)) {
            break;
        }
        (void)rclc_executor_spin_some(&app.executor, 100ULL * 1000 * 1000 /* 100ms */);
    }

    /* 3. Teardown — exercises the transport's close callback. */
    (void)rcl_timer_fini(&app.timer);
    (void)rcl_publisher_fini(&app.publisher, &app.node);
    (void)rcl_node_fini(&app.node);
    (void)rclc_executor_fini(&app.executor);
    (void)rclc_support_fini(&app.support);

    /* Drop the registered transport explicitly so `close` fires
     * even on backends that don't tear it down at session-end. */
    (void)rclc_set_custom_transport(NULL);

    /* 4. Report bookkeeping. */
    int oc = atomic_load(&g_link.open_count);
    int wc = atomic_load(&g_link.write_count);
    int rc = atomic_load(&g_link.read_count);
    int cc = atomic_load(&g_link.close_count);
    fprintf(stdout, "custom link callback counts:\n");
    fprintf(stdout, "  open:  %d\n", oc);
    fprintf(stdout, "  write: %d\n", wc);
    fprintf(stdout, "  read:  %d\n", rc);
    fprintf(stdout, "  close: %d\n", cc);

    int fail = 0;
    if (oc < 1) {
        fprintf(stderr, "FAIL: open callback never fired\n");
        fail = 1;
    }
    if (wc < 1) {
        fprintf(stderr, "FAIL: write callback never fired\n");
        fail = 1;
    }
    if (rc < 1) {
        fprintf(stderr, "FAIL: read callback never fired\n");
        fail = 1;
    }
    if (cc < 1) {
        fprintf(stderr, "FAIL: close callback never fired\n");
        fail = 1;
    }

    return fail;
}
