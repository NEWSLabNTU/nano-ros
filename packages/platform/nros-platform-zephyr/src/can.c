/*
 * RFC-0132 D3 -- the `zephyr` backend of the CAN frame driver.
 *
 * `<nros/platform_can.h>` over Zephyr's CAN API: `can_send()` and an RX filter
 * into a message queue. On the S32K344 and S32Z270 the device is the FlexCAN
 * or CANXL controller, so this is also issue 2573's "raw FlexCAN helper on
 * Zephyr"; on `native_sim` it is a `zephyr,native-linux-can` node on a host
 * interface.
 *
 * THE DEVICE (RFC-0132 D4): `open(NULL)` opens the `nros,can` chosen node,
 * else `zephyr,canbus`. Its layer-1 facts (bitrate, data bitrate, whether it
 * runs in FD mode) are devicetree's, applied at bring-up by `can_start.c`;
 * this file sets no mode and starts no controller.
 *
 * FD (RFC-0132 revision of 2026-11-17): `open(want_fd)` succeeds, and
 * `fd_active` reports whether the controller runs in CAN FD mode
 * (`can_get_mode()`), which `can_start.c` set from the node's data bitrate.
 * An FD frame on a handle whose FD is not active is refused with
 * `NROS_PLATFORM_CAN_ENOFD` before it reaches the driver.
 *
 * SEND COMPLETION: without `NROS_PLATFORM_CAN_SEND_WAIT_TX` the frame is
 * handed to the driver with a no-op TX callback, so `send` returns once the
 * controller accepted it. With the flag the callback is NULL, which makes
 * Zephyr's `can_send()` block until the frame left the controller (or the
 * driver reported an error): `timeout_us` bounds the wait for a TX slot.
 *
 * RECEIVE: the first handle to call `recv` installs two match-all RX filters
 * (11- and 29-bit) into one static queue, and owns them until it closes. A
 * second receiving handle gets `NROS_PLATFORM_CAN_ENOTSUP`: one default device
 * per image (RFC-0132 open question 5), and both first consumers only send.
 */

#include <errno.h>
#include <stdint.h>
#include <string.h>

#include <zephyr/device.h>
#include <zephyr/devicetree.h>
#include <zephyr/drivers/can.h>
#include <zephyr/kernel.h>

#include <nros/platform_can.h>

/* "NCZC": an open zephyr handle. A zeroed handle is closed. */
#define ZEPHYR_CAN_OPEN_MAGIC 0x4e435a43u
#define ZEPHYR_CAN_NO_FILTER (-1)
/* Frames the receive queue holds before the driver drops new ones. */
#define ZEPHYR_CAN_RX_DEPTH 8

struct zephyr_can {
    uint32_t magic;
    uint8_t fd_active;
    int16_t filter_std;
    int16_t filter_ext;
    const struct device* dev;
};

/* RFC-0132 D2a -- the state fits the handle the header sizes.
 * Measured: 24 bytes, alignment 8 (native_sim/native/64); 16 and 4 on a
 * 32-bit target. */
NROS_PLATFORM_CAN_STORAGE_FITS(struct zephyr_can);

K_MSGQ_DEFINE(nros_can_rx_msgq, sizeof(struct can_frame), ZEPHYR_CAN_RX_DEPTH, 4);
static struct zephyr_can* nros_can_rx_owner;

static struct zephyr_can* zephyr_can_state(const nros_platform_can_t* can) {
    struct zephyr_can* s;
    if (can == NULL) {
        return NULL;
    }
    s = (struct zephyr_can*)(void*)can;
    return s->magic == ZEPHYR_CAN_OPEN_MAGIC ? s : NULL;
}

static const struct device* zephyr_can_image_device(void) {
#if DT_HAS_CHOSEN(nros_can)
    return DEVICE_DT_GET(DT_CHOSEN(nros_can));
#elif DT_HAS_CHOSEN(zephyr_canbus)
    return DEVICE_DT_GET(DT_CHOSEN(zephyr_canbus));
#else
    return NULL;
#endif
}

static k_timeout_t zephyr_can_timeout(uint32_t timeout_us) {
    return timeout_us == 0u ? K_NO_WAIT : K_USEC(timeout_us);
}

static int8_t zephyr_can_code(int err, uint8_t fd_frame) {
    switch (err) {
    case 0:
        return NROS_PLATFORM_CAN_OK;
    case -EAGAIN:
    case -EBUSY:
        return NROS_PLATFORM_CAN_EAGAIN;
    case -ENETUNREACH:
        return NROS_PLATFORM_CAN_EBUSOFF;
    case -ENOTSUP:
        /* The driver refuses an FD frame when the controller is not in FD
         * mode; anything else it does not support is an invalid frame. */
        return fd_frame != 0u ? NROS_PLATFORM_CAN_ENOFD : NROS_PLATFORM_CAN_EINVAL;
    case -EINVAL:
        return NROS_PLATFORM_CAN_EINVAL;
    case -ENETDOWN:
        return NROS_PLATFORM_CAN_ENODEV; /* the controller is stopped */
    default:
        return NROS_PLATFORM_CAN_EIO;
    }
}

int8_t nros_platform_can_open(nros_platform_can_t* can, const char* device, uint8_t want_fd) {
    struct zephyr_can* s;
    const struct device* dev;
    can_mode_t mode;

    if (can == NULL) {
        return NROS_PLATFORM_CAN_EINVAL;
    }
    dev = device == NULL ? zephyr_can_image_device() : device_get_binding(device);
    if (dev == NULL || !device_is_ready(dev)) {
        return NROS_PLATFORM_CAN_ENODEV;
    }
    mode = can_get_mode(dev);

    s = (struct zephyr_can*)(void*)can;
    s->magic = ZEPHYR_CAN_OPEN_MAGIC;
    s->dev = dev;
    s->fd_active = (uint8_t)(want_fd != 0u && (mode & CAN_MODE_FD) != 0u);
    s->filter_std = ZEPHYR_CAN_NO_FILTER;
    s->filter_ext = ZEPHYR_CAN_NO_FILTER;
    return NROS_PLATFORM_CAN_OK;
}

/* Acceptance, not completion: Zephyr's can_send() blocks for completion
 * only when the callback is NULL. */
static void zephyr_can_tx_accepted(const struct device* dev, int error, void* user_data) {
    ARG_UNUSED(dev);
    ARG_UNUSED(error);
    ARG_UNUSED(user_data);
}

int8_t nros_platform_can_send(nros_platform_can_t* can, const nros_platform_can_frame_t* frame,
                              uint32_t timeout_us) {
    struct zephyr_can* s = zephyr_can_state(can);
    struct can_frame zf;
    uint8_t fd_frame;
    int8_t rc;
    int err;

    if (s == NULL) {
        return NROS_PLATFORM_CAN_ENOTOPEN;
    }
    rc = nros_platform_can_frame_check(frame, s->fd_active);
    if (rc != NROS_PLATFORM_CAN_OK) {
        return rc;
    }
    fd_frame = (uint8_t)((frame->flags & NROS_PLATFORM_CAN_FLAG_FD) != 0u);

    memset(&zf, 0, sizeof(zf));
    zf.id = frame->id;
    if ((frame->flags & NROS_PLATFORM_CAN_FLAG_EXT) != 0u) {
        zf.flags |= CAN_FRAME_IDE;
    }
    if (fd_frame != 0u) {
#ifdef CONFIG_CAN_FD_MODE
        zf.flags |= CAN_FRAME_FDF;
        if ((frame->flags & NROS_PLATFORM_CAN_FLAG_BRS) != 0u) {
            zf.flags |= CAN_FRAME_BRS;
        }
#else
        /* Unreachable: without CONFIG_CAN_FD_MODE no controller reports
         * CAN_MODE_FD, so no handle has FD active and the check refused it. */
        return NROS_PLATFORM_CAN_ENOFD;
#endif
    }
    zf.dlc = can_bytes_to_dlc(frame->len);
    memcpy(zf.data, frame->data, frame->len);

    if ((frame->flags & NROS_PLATFORM_CAN_SEND_WAIT_TX) != 0u) {
        err = can_send(s->dev, &zf, zephyr_can_timeout(timeout_us), NULL, NULL);
    } else {
        err = can_send(s->dev, &zf, zephyr_can_timeout(timeout_us), zephyr_can_tx_accepted, NULL);
    }
    return zephyr_can_code(err, fd_frame);
}

uint8_t nros_platform_can_fd_active(const nros_platform_can_t* can) {
    const struct zephyr_can* s = zephyr_can_state(can);
    return s == NULL ? 0u : s->fd_active;
}

static int8_t zephyr_can_install_filters(struct zephyr_can* s) {
    const struct can_filter std_all = {.id = 0u, .mask = 0u, .flags = 0u};
    const struct can_filter ext_all = {.id = 0u, .mask = 0u, .flags = CAN_FILTER_IDE};
    int std_id;
    int ext_id;

    if (nros_can_rx_owner != NULL && nros_can_rx_owner != s) {
        return NROS_PLATFORM_CAN_ENOTSUP;
    }
    if (s->filter_std != ZEPHYR_CAN_NO_FILTER) {
        return NROS_PLATFORM_CAN_OK;
    }
    std_id = can_add_rx_filter_msgq(s->dev, &nros_can_rx_msgq, &std_all);
    if (std_id < 0) {
        return std_id == -ENOSPC ? NROS_PLATFORM_CAN_ENOTSUP : NROS_PLATFORM_CAN_EIO;
    }
    ext_id = can_add_rx_filter_msgq(s->dev, &nros_can_rx_msgq, &ext_all);
    if (ext_id < 0) {
        can_remove_rx_filter(s->dev, std_id);
        return ext_id == -ENOSPC ? NROS_PLATFORM_CAN_ENOTSUP : NROS_PLATFORM_CAN_EIO;
    }
    s->filter_std = (int16_t)std_id;
    s->filter_ext = (int16_t)ext_id;
    nros_can_rx_owner = s;
    return NROS_PLATFORM_CAN_OK;
}

int8_t nros_platform_can_recv(nros_platform_can_t* can, nros_platform_can_frame_t* frame,
                              uint32_t timeout_us) {
    struct zephyr_can* s = zephyr_can_state(can);
    struct can_frame zf;
    int8_t rc;

    if (s == NULL) {
        return NROS_PLATFORM_CAN_ENOTOPEN;
    }
    if (frame == NULL) {
        return NROS_PLATFORM_CAN_EINVAL;
    }
    rc = zephyr_can_install_filters(s);
    if (rc != NROS_PLATFORM_CAN_OK) {
        return rc;
    }
    for (;;) {
        if (k_msgq_get(&nros_can_rx_msgq, &zf, zephyr_can_timeout(timeout_us)) != 0) {
            return NROS_PLATFORM_CAN_EAGAIN;
        }
        /* Remote frames have no place in the frame ABI. */
        if ((zf.flags & CAN_FRAME_RTR) == 0u) {
            break;
        }
    }
    memset(frame, 0, sizeof(*frame));
    frame->id = zf.id;
    if ((zf.flags & CAN_FRAME_IDE) != 0u) {
        frame->flags |= NROS_PLATFORM_CAN_FLAG_EXT;
    }
#ifdef CONFIG_CAN_FD_MODE
    if ((zf.flags & CAN_FRAME_FDF) != 0u) {
        frame->flags |= NROS_PLATFORM_CAN_FLAG_FD;
        if ((zf.flags & CAN_FRAME_BRS) != 0u) {
            frame->flags |= NROS_PLATFORM_CAN_FLAG_BRS;
        }
    }
#endif
    frame->len = can_dlc_to_bytes(zf.dlc);
    memcpy(frame->data, zf.data, frame->len);
    return NROS_PLATFORM_CAN_OK;
}

void nros_platform_can_close(nros_platform_can_t* can) {
    struct zephyr_can* s = zephyr_can_state(can);
    if (s == NULL) {
        return;
    }
    if (s->filter_std != ZEPHYR_CAN_NO_FILTER) {
        can_remove_rx_filter(s->dev, s->filter_std);
        can_remove_rx_filter(s->dev, s->filter_ext);
        k_msgq_purge(&nros_can_rx_msgq);
        nros_can_rx_owner = NULL;
    }
    memset(s, 0, sizeof(*s));
}
