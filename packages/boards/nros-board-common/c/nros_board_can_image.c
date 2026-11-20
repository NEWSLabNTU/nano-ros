/*
 * RFC-0132 D4 -- the store behind `nros_board_can_image.h`: the image's CAN
 * frame device, set by the board's boot, read by a backend's `open(NULL)`.
 *
 * Written once before the application runs and only read afterwards, so it
 * takes no lock.
 */

#include "nros_board_can_image.h"

static char g_device[NROS_BOARD_CAN_IMAGE_DEVICE_MAX + 1u];

int8_t nros_board_can_set_image_device(const char* name, size_t len) {
    size_t i;

    g_device[0] = '\0';
    if (name == NULL || len == 0u || len > NROS_BOARD_CAN_IMAGE_DEVICE_MAX) {
        return -1;
    }
    for (i = 0u; i < len; i++) {
        if (name[i] == '\0') {
            return -1;
        }
    }
    for (i = 0u; i < len; i++) {
        g_device[i] = name[i];
    }
    g_device[len] = '\0';
    return 0;
}

const char* nros_board_can_image_device(void) {
    return g_device[0] != '\0' ? g_device : NULL;
}
