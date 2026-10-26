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

#if DT_HAS_CHOSEN(zephyr_canbus)

static int nros_can_start(void) {
    const struct device* dev = DEVICE_DT_GET(DT_CHOSEN(zephyr_canbus));
    if (!device_is_ready(dev)) {
        /* Not fatal at boot: an image with no CAN session never notices, and
         * one with a CAN session is refused at open naming the controller. */
        LOG_ERR("CAN controller %s is not ready; not started", dev->name);
        return 0;
    }
    int rc = can_start(dev);
    /* -EALREADY: the application started it first, which is its right. */
    if (rc < 0 && rc != -EALREADY) {
        LOG_ERR("can_start(%s) failed: %d", dev->name, rc);
        return 0;
    }
    LOG_INF("CAN controller %s started", dev->name);
    return 0;
}

SYS_INIT(nros_can_start, APPLICATION, CONFIG_APPLICATION_INIT_PRIORITY);

#endif /* DT_HAS_CHOSEN(zephyr_canbus) */
