/*
 * RFC-0132 D3 -- the `socketcan` backend of the CAN frame driver.
 *
 * `<nros/platform_can.h>` over Linux SocketCAN (`PF_CAN` / `SOCK_RAW` /
 * `CAN_RAW`, with `CAN_RAW_FD_FRAMES` when FD is wanted and the interface's MTU
 * carries it). Linux-only, so it lives in the board layer:
 * `nros-platform-posix` is named for a standard and
 * `check-posix-platform-purity` refuses SocketCAN there.
 *
 * ONE source for every board that is a Linux process: `nros-board-linux`
 * compiles it, and `nros-board-freertos-posix` and `nros-board-threadx-linux`
 * compile this same file by its workspace path (as they compile
 * `nros-board-common/c/`), with no Cargo dependency on `nros-board-linux`.
 *
 * What it does not do (RFC-0132 D1): set a bitrate, bring an interface up, or
 * choose FD. The host OS owns the interface (`ip link set can0 type can bitrate
 * ... dbitrate ... fd on`); the application owns `want_fd`.
 *
 * Blocking: the socket is non-blocking and every wait is a `poll` bounded by
 * the call's `timeout_us`, retried on EINTR, because on the FreeRTOS and ThreadX
 * host ports a timer signal interrupts system calls on every tick.
 */

#include <errno.h>
#include <fcntl.h>
#include <net/if.h>
#include <poll.h>
#include <stdint.h>
#include <string.h>
#include <sys/ioctl.h>
#include <sys/socket.h>
#include <time.h>
#include <unistd.h>

#include <linux/can.h>
#include <linux/can/raw.h>

#include <nros/platform_can.h>

/* "NCSC": an open socketcan handle. A zeroed handle is closed. */
#define SOCKETCAN_OPEN_MAGIC 0x4e435343u

struct socketcan_can {
    uint32_t magic;
    int32_t fd;
    uint8_t fd_active;
};

/* RFC-0132 D2a -- the state fits the handle the header sizes.
 * Measured: 12 bytes, alignment 4 (x86_64). */
NROS_PLATFORM_CAN_STORAGE_FITS(struct socketcan_can);

static struct socketcan_can* socketcan_state(const nros_platform_can_t* can) {
    struct socketcan_can* s;
    if (can == NULL) {
        return NULL;
    }
    s = (struct socketcan_can*)(void*)can;
    return s->magic == SOCKETCAN_OPEN_MAGIC ? s : NULL;
}

static uint64_t socketcan_now_us(void) {
    struct timespec ts;
    (void)clock_gettime(CLOCK_MONOTONIC, &ts);
    return (uint64_t)ts.tv_sec * 1000000u + (uint64_t)ts.tv_nsec / 1000u;
}

/* Wait until `fd` is ready for `events` or `deadline_us` passes. 1 ready,
 * 0 timed out, -1 error. EINTR restarts the wait with the time left. */
static int socketcan_wait(int fd, short events, uint64_t deadline_us) {
    for (;;) {
        uint64_t now = socketcan_now_us();
        struct pollfd p;
        int ms;
        int rc;
        if (now >= deadline_us) {
            return 0;
        }
        /* Round up, so a sub-millisecond remainder still waits. */
        ms = (int)((deadline_us - now + 999u) / 1000u);
        p.fd = fd;
        p.events = events;
        p.revents = 0;
        rc = poll(&p, 1, ms);
        if (rc > 0) {
            return 1;
        }
        if (rc == 0) {
            continue; /* re-check the deadline; poll's clock is coarser */
        }
        if (errno != EINTR) {
            return -1;
        }
    }
}

static int8_t socketcan_errno_code(int err) {
    switch (err) {
    case EAGAIN:
#if EWOULDBLOCK != EAGAIN
    case EWOULDBLOCK:
#endif
    case ENOBUFS:
        return NROS_PLATFORM_CAN_EAGAIN;
    case EINVAL:
        return NROS_PLATFORM_CAN_EINVAL;
    case ENETDOWN:
    case ENXIO:
    case ENODEV:
        return NROS_PLATFORM_CAN_ENODEV;
    default:
        return NROS_PLATFORM_CAN_EIO;
    }
}

int8_t nros_platform_can_open(nros_platform_can_t* can, const char* device, uint8_t want_fd) {
    struct socketcan_can* s;
    struct ifreq ifr;
    struct sockaddr_can addr;
    size_t name_len;
    int fd;
    int flags;

    if (can == NULL) {
        return NROS_PLATFORM_CAN_EINVAL;
    }
    /* The image's device (`open(NULL)`) reaches this backend with the
     * `[image.<id>.can] device` carrier (RFC-0132 D4, phase-560 W9.d/e). Until
     * then only a named device can be opened here. */
    if (device == NULL) {
        return NROS_PLATFORM_CAN_ENODEV;
    }
    name_len = strlen(device);
    if (name_len == 0u || name_len >= sizeof(ifr.ifr_name)) {
        return NROS_PLATFORM_CAN_ENODEV;
    }

    fd = socket(PF_CAN, SOCK_RAW | SOCK_CLOEXEC, CAN_RAW);
    if (fd < 0) {
        return NROS_PLATFORM_CAN_EIO;
    }
    memset(&ifr, 0, sizeof(ifr));
    memcpy(ifr.ifr_name, device, name_len);
    if (ioctl(fd, SIOCGIFINDEX, &ifr) < 0) {
        (void)close(fd);
        return NROS_PLATFORM_CAN_ENODEV;
    }
    memset(&addr, 0, sizeof(addr));
    addr.can_family = AF_CAN;
    addr.can_ifindex = ifr.ifr_ifindex;

    if (want_fd != 0u) {
        /* The interface carries FD only when its MTU holds an FD frame
         * (`ip link set <dev> ... fd on` gives CANFD_MTU). Enabling
         * CAN_RAW_FD_FRAMES on a classic interface succeeds and then every FD
         * send fails, so the MTU is the fact read here, at open. */
        struct ifreq mtu_req;
        int on = 1;
        memset(&mtu_req, 0, sizeof(mtu_req));
        memcpy(mtu_req.ifr_name, device, name_len);
        if (ioctl(fd, SIOCGIFMTU, &mtu_req) < 0) {
            (void)close(fd);
            return NROS_PLATFORM_CAN_ENODEV;
        }
        if (mtu_req.ifr_mtu < (int)CANFD_MTU) {
            (void)close(fd);
            return NROS_PLATFORM_CAN_ENOFD;
        }
        if (setsockopt(fd, SOL_CAN_RAW, CAN_RAW_FD_FRAMES, &on, sizeof(on)) < 0) {
            (void)close(fd);
            return NROS_PLATFORM_CAN_ENOFD;
        }
    }
    if (bind(fd, (struct sockaddr*)&addr, sizeof(addr)) < 0) {
        int8_t code = socketcan_errno_code(errno);
        (void)close(fd);
        return code == NROS_PLATFORM_CAN_EAGAIN ? NROS_PLATFORM_CAN_EIO : code;
    }
    flags = fcntl(fd, F_GETFL, 0);
    if (flags < 0 || fcntl(fd, F_SETFL, flags | O_NONBLOCK) < 0) {
        (void)close(fd);
        return NROS_PLATFORM_CAN_EIO;
    }

    s = (struct socketcan_can*)(void*)can;
    s->magic = SOCKETCAN_OPEN_MAGIC;
    s->fd = fd;
    s->fd_active = (uint8_t)(want_fd != 0u);
    return NROS_PLATFORM_CAN_OK;
}

int8_t nros_platform_can_send(nros_platform_can_t* can, const nros_platform_can_frame_t* frame,
                              uint32_t timeout_us) {
    struct socketcan_can* s = socketcan_state(can);
    union {
        struct can_frame classic;
        struct canfd_frame fd;
    } wire;
    size_t size;
    uint64_t deadline;
    int8_t rc;

    if (s == NULL) {
        return NROS_PLATFORM_CAN_ENOTOPEN;
    }
    rc = nros_platform_can_frame_check(frame, s->fd_active);
    if (rc != NROS_PLATFORM_CAN_OK) {
        return rc;
    }
    /* Completion is reported by CAN_RAW_RECV_OWN_MSGS, which no consumer has
     * asked for yet (RFC-0132 open question 1). Never a silent downgrade. */
    if ((frame->flags & NROS_PLATFORM_CAN_SEND_WAIT_TX) != 0u) {
        return NROS_PLATFORM_CAN_ENOTSUP;
    }

    memset(&wire, 0, sizeof(wire));
    if ((frame->flags & NROS_PLATFORM_CAN_FLAG_FD) != 0u) {
        wire.fd.can_id = frame->id;
        if ((frame->flags & NROS_PLATFORM_CAN_FLAG_EXT) != 0u) {
            wire.fd.can_id |= CAN_EFF_FLAG;
        }
        wire.fd.len = frame->len;
        if ((frame->flags & NROS_PLATFORM_CAN_FLAG_BRS) != 0u) {
            wire.fd.flags |= CANFD_BRS;
        }
#ifdef CANFD_FDF
        wire.fd.flags |= CANFD_FDF;
#endif
        memcpy(wire.fd.data, frame->data, frame->len);
        size = CANFD_MTU;
    } else {
        wire.classic.can_id = frame->id;
        if ((frame->flags & NROS_PLATFORM_CAN_FLAG_EXT) != 0u) {
            wire.classic.can_id |= CAN_EFF_FLAG;
        }
        wire.classic.can_dlc = frame->len;
        memcpy(wire.classic.data, frame->data, frame->len);
        size = CAN_MTU;
    }

    deadline = socketcan_now_us() + timeout_us;
    for (;;) {
        ssize_t n = write(s->fd, &wire, size);
        int err;
        if (n == (ssize_t)size) {
            return NROS_PLATFORM_CAN_OK;
        }
        if (n >= 0) {
            return NROS_PLATFORM_CAN_EIO; /* a raw CAN socket writes whole frames */
        }
        err = errno;
        if (err == EINTR) {
            continue;
        }
        rc = socketcan_errno_code(err);
        if (rc != NROS_PLATFORM_CAN_EAGAIN) {
            return rc;
        }
        /* Queue full. POLLOUT is not a promise on a CAN socket (the device
         * queue, not the socket buffer, may be the full one), so wait at most
         * a millisecond at a time and try again until the deadline. */
        if (timeout_us == 0u || socketcan_now_us() >= deadline) {
            return NROS_PLATFORM_CAN_EAGAIN;
        }
        {
            uint64_t slice = socketcan_now_us() + 1000u;
            if (socketcan_wait(s->fd, POLLOUT, slice < deadline ? slice : deadline) < 0) {
                return NROS_PLATFORM_CAN_EIO;
            }
        }
    }
}

uint8_t nros_platform_can_fd_active(const nros_platform_can_t* can) {
    const struct socketcan_can* s = socketcan_state(can);
    return s == NULL ? 0u : s->fd_active;
}

int8_t nros_platform_can_recv(nros_platform_can_t* can, nros_platform_can_frame_t* frame,
                              uint32_t timeout_us) {
    struct socketcan_can* s = socketcan_state(can);
    uint64_t deadline;

    if (s == NULL) {
        return NROS_PLATFORM_CAN_ENOTOPEN;
    }
    if (frame == NULL) {
        return NROS_PLATFORM_CAN_EINVAL;
    }
    deadline = socketcan_now_us() + timeout_us;
    for (;;) {
        struct canfd_frame wire;
        ssize_t n = read(s->fd, &wire, sizeof(wire));
        if (n == (ssize_t)CAN_MTU || n == (ssize_t)CANFD_MTU) {
            /* Remote and error frames have no place in the frame ABI. */
            if ((wire.can_id & (CAN_RTR_FLAG | CAN_ERR_FLAG)) != 0u) {
                continue;
            }
            memset(frame, 0, sizeof(*frame));
            if ((wire.can_id & CAN_EFF_FLAG) != 0u) {
                frame->id = wire.can_id & CAN_EFF_MASK;
                frame->flags = NROS_PLATFORM_CAN_FLAG_EXT;
            } else {
                frame->id = wire.can_id & CAN_SFF_MASK;
            }
            if (n == (ssize_t)CANFD_MTU) {
                frame->flags |= NROS_PLATFORM_CAN_FLAG_FD;
                if ((wire.flags & CANFD_BRS) != 0u) {
                    frame->flags |= NROS_PLATFORM_CAN_FLAG_BRS;
                }
            }
            frame->len = wire.len;
            memcpy(frame->data, wire.data, wire.len);
            return NROS_PLATFORM_CAN_OK;
        }
        if (n >= 0) {
            return NROS_PLATFORM_CAN_EIO;
        }
        if (errno == EINTR) {
            continue;
        }
        if (errno != EAGAIN && errno != EWOULDBLOCK) {
            return socketcan_errno_code(errno);
        }
        if (timeout_us == 0u) {
            return NROS_PLATFORM_CAN_EAGAIN;
        }
        switch (socketcan_wait(s->fd, POLLIN, deadline)) {
        case 1:
            continue;
        case 0:
            return NROS_PLATFORM_CAN_EAGAIN;
        default:
            return NROS_PLATFORM_CAN_EIO;
        }
    }
}

void nros_platform_can_close(nros_platform_can_t* can) {
    struct socketcan_can* s = socketcan_state(can);
    if (s == NULL) {
        return;
    }
    (void)close(s->fd);
    s->magic = 0u;
    s->fd = -1;
    s->fd_active = 0u;
}
