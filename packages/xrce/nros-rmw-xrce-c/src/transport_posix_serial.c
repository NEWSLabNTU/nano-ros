/* Phase 115.K.2.5.1.5-serial — POSIX serial transport via custom-transport.
 *
 * Mirrors `transport_posix_udp.c` but for tty / pseudo-tty devices.
 * Serial is byte-stream, so framing=true and HDLC framing comes from
 * the upstream `UCLIENT_PROFILE_STREAM_FRAMING` profile.
 *
 * Per-session state lives in `xrce_session_state_t::serial_bridge`
 * (just an `int fd`). Multi-session safe — the trampolines reach the
 * fd via `uxrCustomTransport.args`.
 *
 * Reference: `packages/xrce/nros-rmw-xrce/src/platform_serial.rs`
 * (legacy Rust impl) — same termios + read/write pattern, just
 * routed through `nros-platform-posix::serial` there.
 */

#include "internal.h"

#include "nros/rmw_ret.h"

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>

#include <errno.h>
#include <fcntl.h>
#include <poll.h>
#include <termios.h>
#include <unistd.h>

#include <uxr/client/profile/transport/custom/custom_transport.h>

/* Bridge struct hung off `uxrCustomTransport.args`. Carries the
 * connected serial fd. */
typedef struct xrce_posix_serial_bridge {
    int fd;
} xrce_posix_serial_bridge;

/* Map an integer baud-rate to a termios `Bxxx` constant. Returns 0 if
 * the baud is unsupported by termios on the current platform. */
static speed_t baud_to_speed(long baud) {
    switch (baud) {
#ifdef B9600
        case 9600:    return B9600;
#endif
#ifdef B19200
        case 19200:   return B19200;
#endif
#ifdef B38400
        case 38400:   return B38400;
#endif
#ifdef B57600
        case 57600:   return B57600;
#endif
#ifdef B115200
        case 115200:  return B115200;
#endif
#ifdef B230400
        case 230400:  return B230400;
#endif
#ifdef B460800
        case 460800:  return B460800;
#endif
#ifdef B921600
        case 921600:  return B921600;
#endif
        default:      return 0;
    }
}

/* ---- Trampolines (registered with uxr) -------------------------- */

static bool posix_serial_open(struct uxrCustomTransport *t) {
    /* Open is a no-op — the fd was created in
     * `xrce_posix_serial_init` before
     * `uxr_set_custom_transport_callbacks`. */
    (void)t;
    return true;
}

static bool posix_serial_close(struct uxrCustomTransport *t) {
    if (t == NULL) return true;
    xrce_posix_serial_bridge *b = (xrce_posix_serial_bridge *)t->args;
    if (b == NULL) return true;
    if (b->fd >= 0) {
        close(b->fd);
        b->fd = -1;
    }
    return true;
}

static size_t posix_serial_write(struct uxrCustomTransport *t,
                                 const uint8_t *buf, size_t len,
                                 uint8_t *err) {
    (void)err;
    if (t == NULL) return 0;
    xrce_posix_serial_bridge *b = (xrce_posix_serial_bridge *)t->args;
    if (b == NULL || b->fd < 0) return 0;
    size_t total = 0;
    while (total < len) {
        ssize_t n = write(b->fd, buf + total, len - total);
        if (n < 0) {
            if (errno == EINTR) continue;
            if (errno == EAGAIN || errno == EWOULDBLOCK) break;
            return 0;
        }
        if (n == 0) break;
        total += (size_t)n;
    }
    return total;
}

static size_t posix_serial_read(struct uxrCustomTransport *t,
                                uint8_t *buf, size_t len,
                                int timeout, uint8_t *err) {
    (void)err;
    if (t == NULL) return 0;
    xrce_posix_serial_bridge *b = (xrce_posix_serial_bridge *)t->args;
    if (b == NULL || b->fd < 0) return 0;

    struct pollfd pfd;
    pfd.fd = b->fd;
    pfd.events = POLLIN;
    int rc = poll(&pfd, 1, timeout);
    if (rc <= 0) {
        return 0; /* timeout or error */
    }
    ssize_t n = read(b->fd, buf, len);
    if (n < 0) {
        return 0;
    }
    return (size_t)n;
}

/* ---- Init ------------------------------------------------------- */

nros_rmw_ret_t xrce_posix_serial_init(xrce_session_state_t *st,
                                      const char *path) {
    if (st == NULL || path == NULL) {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }

    /* Open the tty in raw read/write mode. O_NOCTTY: do not become
     * the controlling terminal. O_NONBLOCK on open to avoid blocking
     * on DCD assertion; we drop NONBLOCK after configuring termios
     * so reads block, then we use poll() for the timeout. */
    int fd = open(path, O_RDWR | O_NOCTTY | O_NONBLOCK);
    if (fd < 0) {
        return NROS_RMW_RET_ERROR;
    }

    struct termios tio;
    if (tcgetattr(fd, &tio) != 0) {
        close(fd);
        return NROS_RMW_RET_ERROR;
    }

    /* Raw mode (8N1, no flow control). */
    cfmakeraw(&tio);
    tio.c_cflag &= (tcflag_t)~(PARENB | CSTOPB | CSIZE | CRTSCTS);
    tio.c_cflag |= (tcflag_t)(CS8 | CLOCAL | CREAD);
    tio.c_iflag &= (tcflag_t)~(IXON | IXOFF | IXANY);
    /* read() blocks; poll() drives the timeout. */
    tio.c_cc[VMIN] = 0;
    tio.c_cc[VTIME] = 0;

    /* Baud rate. Env override XRCE_SERIAL_BAUD; default 115200. */
    long baud = 115200;
    const char *baud_env = getenv("XRCE_SERIAL_BAUD");
    if (baud_env != NULL && baud_env[0] != '\0') {
        char *end = NULL;
        long parsed = strtol(baud_env, &end, 10);
        if (end != NULL && *end == '\0' && parsed > 0) {
            baud = parsed;
        }
    }
    speed_t speed = baud_to_speed(baud);
    if (speed == 0) {
        close(fd);
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    cfsetispeed(&tio, speed);
    cfsetospeed(&tio, speed);

    if (tcsetattr(fd, TCSANOW, &tio) != 0) {
        close(fd);
        return NROS_RMW_RET_ERROR;
    }

    /* Drop O_NONBLOCK now that termios is set. read() / write() are
     * still effectively non-blocking via poll() in the trampoline. */
    int flags = fcntl(fd, F_GETFL, 0);
    if (flags >= 0) {
        (void)fcntl(fd, F_SETFL, flags & ~O_NONBLOCK);
    }

    st->serial_bridge.fd = fd;

    /* Wire the custom transport with framing=true (serial is
     * byte-stream — needs HDLC framing from
     * UCLIENT_PROFILE_STREAM_FRAMING). */
    uxr_set_custom_transport_callbacks(
        &st->custom, /*framing=*/true,
        posix_serial_open,
        posix_serial_close,
        posix_serial_write,
        posix_serial_read);

    if (!uxr_init_custom_transport(&st->custom, &st->serial_bridge)) {
        close(fd);
        st->serial_bridge.fd = -1;
        return NROS_RMW_RET_ERROR;
    }
    return NROS_RMW_RET_OK;
}
