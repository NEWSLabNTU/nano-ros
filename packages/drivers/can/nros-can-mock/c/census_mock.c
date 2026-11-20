/*
 * RFC-0132 D6 -- the mock backend for a C/C++ census host build.
 *
 * The census builds an image's own sources for the HOST and runs them in
 * counting mode. An entry that calls the CAN frame driver needs the five
 * `<nros/platform_can.h>` calls defined there, and the host census build is a
 * C/C++ link with one Rust runtime archive already in it, so the Rust mock
 * (`src/imp.rs`) cannot be a second one. This is its C twin, with the same
 * contract a counting run needs and nothing more:
 *
 *  - `open` succeeds on any device (`NULL` included) and never touches an
 *    interface; with `want_fd` the handle carries FD, as the Rust mock's
 *    default bus does;
 *  - `send` applies the header's frame check (`nros_platform_can_frame_check`,
 *    the one statement of which frames are valid) and then drops the frame;
 *  - `recv` has nothing queued (`NROS_PLATFORM_CAN_EAGAIN`).
 *
 * Linked only into a census host build (`cmake/NanoRosCanImage.cmake`), as an
 * archive member pulled only when the entry calls the driver: never on a
 * hardware image (D5).
 */

#include <nros/platform_can.h>

/* "NCCM": an open census-mock handle. A zeroed handle is closed. */
#define CENSUS_MOCK_OPEN_MAGIC 0x4e43434du

struct census_mock_can {
    uint32_t magic;
    uint8_t fd_active;
};

/* RFC-0132 D2a -- the state fits the handle the header sizes. */
NROS_PLATFORM_CAN_STORAGE_FITS(struct census_mock_can);

static struct census_mock_can* census_mock_state(const nros_platform_can_t* can) {
    struct census_mock_can* s;
    if (can == NULL) {
        return NULL;
    }
    s = (struct census_mock_can*)(void*)can;
    return s->magic == CENSUS_MOCK_OPEN_MAGIC ? s : NULL;
}

int8_t nros_platform_can_open(nros_platform_can_t* can, const char* device, uint8_t want_fd) {
    struct census_mock_can* s;
    (void)device;
    if (can == NULL) {
        return NROS_PLATFORM_CAN_EINVAL;
    }
    s = (struct census_mock_can*)(void*)can;
    s->magic = CENSUS_MOCK_OPEN_MAGIC;
    s->fd_active = (uint8_t)(want_fd != 0u);
    return NROS_PLATFORM_CAN_OK;
}

int8_t nros_platform_can_send(nros_platform_can_t* can, const nros_platform_can_frame_t* frame,
                              uint32_t timeout_us) {
    struct census_mock_can* s = census_mock_state(can);
    (void)timeout_us;
    if (s == NULL) {
        return NROS_PLATFORM_CAN_ENOTOPEN;
    }
    return nros_platform_can_frame_check(frame, s->fd_active);
}

uint8_t nros_platform_can_fd_active(const nros_platform_can_t* can) {
    const struct census_mock_can* s = census_mock_state(can);
    return s != NULL ? s->fd_active : 0u;
}

int8_t nros_platform_can_recv(nros_platform_can_t* can, nros_platform_can_frame_t* frame,
                              uint32_t timeout_us) {
    (void)frame;
    (void)timeout_us;
    return census_mock_state(can) != NULL ? NROS_PLATFORM_CAN_EAGAIN : NROS_PLATFORM_CAN_ENOTOPEN;
}

void nros_platform_can_close(nros_platform_can_t* can) {
    struct census_mock_can* s = census_mock_state(can);
    if (s != NULL) {
        s->magic = 0u;
        s->fd_active = 0u;
    }
}
