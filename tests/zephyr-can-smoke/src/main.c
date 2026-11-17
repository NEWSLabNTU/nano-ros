/*
 * RFC-0132 / phase-560 W9.b -- the `zephyr` backend of the CAN frame driver.
 *
 * Opens the image's device (`open(NULL)`) with `want_fd`, and:
 *   - FD mode (fd.overlay): sends the classic frame 0x123 and the FD frame
 *     0x1ABCDEF0 (EXT, BRS, 64 bytes 0x00..0x3F), then 0x124 with
 *     NROS_PLATFORM_CAN_SEND_WAIT_TX;
 *   - classic mode (classic.overlay): `fd_active` must be false, the classic
 *     frame goes out, and the FD frame is refused with NROS_PLATFORM_CAN_ENOFD.
 * scripts/test/can-frame-zephyr.sh reads the bus with candump.
 */

#include <stdlib.h>
#include <string.h>

#include <zephyr/kernel.h>
#include <zephyr/sys/printk.h>

#include <nros/platform_can.h>

static int s_fail;

#define CHECK(cond, what)                                                      \
    do {                                                                       \
        if (!(cond)) {                                                         \
            printk("CAN-SMOKE: FAIL: %s\n", what);                             \
            s_fail = 1;                                                        \
        }                                                                      \
    } while (0)

int main(void) {
    nros_platform_can_t can;
    nros_platform_can_frame_t classic;
    nros_platform_can_frame_t fd;
    nros_platform_can_frame_t waited;
    int8_t rc;
    uint8_t fd_active;

    memset(&can, 0, sizeof(can));
    rc = nros_platform_can_open(&can, NULL, 1u);
    printk("CAN-SMOKE: open(NULL, want_fd) = %d\n", rc);
    if (rc != NROS_PLATFORM_CAN_OK) {
        printk("CAN-SMOKE: FAIL: open\n");
        exit(1);
    }
    fd_active = nros_platform_can_fd_active(&can);
    printk("CAN-SMOKE: fd_active = %u\n", fd_active);

    memset(&classic, 0, sizeof(classic));
    classic.id = 0x123u;
    classic.len = 8u;
    for (uint8_t i = 0; i < 8u; i++) {
        classic.data[i] = (uint8_t)(0x11u * (i + 1u));
    }
    memset(&fd, 0, sizeof(fd));
    fd.id = 0x1ABCDEF0u;
    fd.flags = NROS_PLATFORM_CAN_FLAG_EXT | NROS_PLATFORM_CAN_FLAG_FD | NROS_PLATFORM_CAN_FLAG_BRS;
    fd.len = 64u;
    for (uint8_t i = 0; i < 64u; i++) {
        fd.data[i] = i;
    }

    rc = nros_platform_can_send(&can, &classic, 100000u);
    CHECK(rc == NROS_PLATFORM_CAN_OK, "classic send");
    rc = nros_platform_can_send(&can, &fd, 100000u);
    if (fd_active != 0u) {
        CHECK(rc == NROS_PLATFORM_CAN_OK, "FD send on an FD controller");
        memset(&waited, 0, sizeof(waited));
        waited.id = 0x124u;
        waited.flags = NROS_PLATFORM_CAN_SEND_WAIT_TX;
        waited.len = 1u;
        waited.data[0] = 0x5Au;
        rc = nros_platform_can_send(&can, &waited, 100000u);
        CHECK(rc == NROS_PLATFORM_CAN_OK, "SEND_WAIT_TX send");
        printk("CAN-SMOKE: mode FD: classic, FD and WAIT_TX frames sent\n");
    } else {
        CHECK(rc == NROS_PLATFORM_CAN_ENOFD, "FD send on a classic controller returns ENOFD");
        printk("CAN-SMOKE: mode classic: classic sent, FD refused with %d\n", rc);
    }
    /* Refusals that never reach the driver. */
    classic.len = 9u;
    CHECK(nros_platform_can_send(&can, &classic, 0u) == NROS_PLATFORM_CAN_EINVAL, "9-byte classic");

    nros_platform_can_close(&can);
    CHECK(nros_platform_can_send(&can, &fd, 0u) == NROS_PLATFORM_CAN_ENOTOPEN, "send after close");

    printk("CAN-SMOKE: %s\n", s_fail ? "FAIL" : "PASS");
    exit(s_fail);
}
