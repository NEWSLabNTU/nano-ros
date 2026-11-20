/*
 * RFC-0132 / phase-560 W9.e -- the cmake road's CAN device delivery probe.
 *
 * Opens the image's CAN device (`open(NULL)`) and prints which device the
 * entry handed over and what `open` returned; on success it sends one classic
 * frame (0x263, 8 bytes) that `scripts/test/can-image-device.sh` reads back
 * with `candump`. The same lines as the cargo road's probe.
 */

#include <stdio.h>

#include <nros/platform_can.h>

/* The board's store of the image's device (RFC-0132 D4), which the backend's
 * `open(NULL)` reads. Board-internal; the probe reads it only to report the
 * device it opened. */
extern const char* nros_board_can_image_device(void);

int main(void) {
    static nros_platform_can_t can;
    nros_platform_can_frame_t frame = {
        .id = 0x263u,
        .flags = 0u,
        .len = 8u,
        .data = {0x26, 0x33, 0x0e, 0x00, 0xca, 0xfe, 0x01, 0x02},
    };
    const char* dev = nros_board_can_image_device();
    int8_t rc;

    if (dev == NULL) {
        dev = "-";
    }
    rc = nros_platform_can_open(&can, NULL, 0u);
    printf("CAN-IMAGE: device=%s open(NULL)=%d\n", dev, (int)rc);
    if (rc != NROS_PLATFORM_CAN_OK) {
        fflush(stdout);
        return 0;
    }
    rc = nros_platform_can_send(&can, &frame, 100000u);
    if (rc == NROS_PLATFORM_CAN_OK) {
        printf("CAN-IMAGE: sent 0x263 on %s\n", dev);
    } else {
        printf("CAN-IMAGE: send failed: %d\n", (int)rc);
    }
    nros_platform_can_close(&can);
    fflush(stdout);
    return 0;
}
