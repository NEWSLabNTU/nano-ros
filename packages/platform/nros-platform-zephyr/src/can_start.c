/*
 * RFC-0109 D10 (phase-514 W5) — the board starts its CAN controller.
 *
 * Bringing a controller up is a layer-1 fact, owned by whoever owns the device,
 * never by the middleware that rides it. On Zephyr, devicetree states the
 * controller and its `bus-speed`, and the driver applies both at init — but
 * there is no configuration-only way to START it: a Zephyr CAN controller sits
 * in CAN_STATE_STOPPED until somebody calls `can_start()`. Our zenoh-pico fork's
 * CAN and ISO-TP links used to make that call themselves, refcounted per
 * device; they now assume a started controller and refuse a stopped one at
 * session open, so the call lives here, before any session opens.
 *
 * WHICH controller: the one `zephyr,canbus` chooses — Zephyr's own name for
 * "this board's CAN bus", and the node `cmake/zephyr/native-sim-can-host.overlay`
 * points at the host's vcan0. An image that rides a second controller starts it
 * itself, as it would any other device the board does not name.
 *
 * RFC-0132 (revision of 2026-11-17) adds two things, both layer 1:
 *
 *   - FD MODE. A Zephyr controller takes `can_set_mode(CAN_MODE_FD)` only while
 *     stopped, so it is set here, before `can_start()`, when the node states a
 *     data-phase bitrate (`bitrate-data`, or its deprecated spelling
 *     `bus-speed-data`), the image has CONFIG_CAN_FD_MODE, and the controller
 *     reports CAN_MODE_FD among its capabilities. That is the one place FD is
 *     decided; the CAN frame driver (`can.c`) reads it back through
 *     `can_get_mode()` and never changes it.
 *   - THE `nros,can` CHOSEN. The frame driver's device is `nros,can` when an
 *     image's overlay names one (else `zephyr,canbus`), so that controller is
 *     started too. When both name one node it is started once.
 *
 * RMW-blind, like net_wait.c: nothing here knows which middleware is linked.
 * An APPLICATION-level SYS_INIT runs after every driver's POST_KERNEL init and
 * before main(), so the controller is up before the first session.
 */

#include <zephyr/device.h>
#include <zephyr/devicetree.h>
#include <zephyr/drivers/can.h>
#include <zephyr/init.h>
#include <zephyr/logging/log.h>

LOG_MODULE_REGISTER(nros_can_start, LOG_LEVEL_INF);

#if DT_HAS_CHOSEN(zephyr_canbus) || DT_HAS_CHOSEN(nros_can)

/* Does the node state a data-phase bitrate? Either spelling: Zephyr renamed
 * `bus-speed-data` to `bitrate-data` and keeps both in the binding. */
#define NROS_CAN_NODE_STATES_FD(node)                                          \
    (DT_NODE_HAS_PROP(node, bitrate_data) || DT_NODE_HAS_PROP(node, bus_speed_data))

static void nros_can_start_one(const struct device* dev, bool states_fd) {
    int rc;
    if (!device_is_ready(dev)) {
        /* Not fatal at boot: an image with no CAN session never notices, and
         * one with a CAN session is refused at open naming the controller. */
        LOG_ERR("CAN controller %s is not ready; not started", dev->name);
        return;
    }
#ifdef CONFIG_CAN_FD_MODE
    if (states_fd) {
        can_mode_t cap = 0;
        if (can_get_capabilities(dev, &cap) == 0 && (cap & CAN_MODE_FD) != 0u) {
            /* -EBUSY: the application started it first, in the mode of its
             * choosing, which is its right. */
            rc = can_set_mode(dev, CAN_MODE_FD);
            if (rc < 0 && rc != -EBUSY) {
                LOG_ERR("can_set_mode(%s, FD) failed: %d", dev->name, rc);
            }
        } else {
            LOG_ERR("CAN controller %s states a data bitrate but cannot run CAN FD", dev->name);
        }
    }
#else
    if (states_fd) {
        LOG_WRN("CAN controller %s states a data bitrate, but CONFIG_CAN_FD_MODE is off: "
                "it runs classic CAN", dev->name);
    }
#endif
    rc = can_start(dev);
    /* -EALREADY: the application started it first, which is its right. */
    if (rc < 0 && rc != -EALREADY) {
        LOG_ERR("can_start(%s) failed: %d", dev->name, rc);
        return;
    }
    LOG_INF("CAN controller %s started (%s)", dev->name,
            (can_get_mode(dev) & CAN_MODE_FD) != 0u ? "CAN FD" : "classic");
}

static int nros_can_start(void) {
#if DT_HAS_CHOSEN(zephyr_canbus)
    nros_can_start_one(DEVICE_DT_GET(DT_CHOSEN(zephyr_canbus)),
                       NROS_CAN_NODE_STATES_FD(DT_CHOSEN(zephyr_canbus)));
#endif
#if DT_HAS_CHOSEN(nros_can)
#if DT_HAS_CHOSEN(zephyr_canbus)
    if (DEVICE_DT_GET(DT_CHOSEN(nros_can)) != DEVICE_DT_GET(DT_CHOSEN(zephyr_canbus)))
#endif
    {
        nros_can_start_one(DEVICE_DT_GET(DT_CHOSEN(nros_can)),
                           NROS_CAN_NODE_STATES_FD(DT_CHOSEN(nros_can)));
    }
#endif
    return 0;
}

SYS_INIT(nros_can_start, APPLICATION, CONFIG_APPLICATION_INIT_PRIORITY);

#endif /* DT_HAS_CHOSEN(zephyr_canbus) || DT_HAS_CHOSEN(nros_can) */
