/*
 * nros_board_can_image.h -- the image's CAN frame device, handed from the
 * board's boot to the CAN backend's `open(NULL)` (RFC-0132 D4).
 *
 * The device is named once per board stack and travels in `NROS_IMAGE_NET` as
 * its `can` row. The board's boot decodes that row (cargo road: the board
 * crate, through `nros_board_common::can_image`; cmake road: the entry's
 * generated hand-off, from the device the CLI decoded) and stores the device
 * here. A backend's `nros_platform_can_open(can, NULL, ...)` reads it back.
 *
 * Board-internal, not an application ABI: an application names the image's
 * device with `open(NULL)` and a second device by its name.
 *
 * One TU (`nros_board_can_image.c`), apart from every backend, so a board that
 * hands a device over pulls only this store into the link, never a backend:
 * the mock may be the image's backend instead.
 */

#ifndef NROS_BOARD_CAN_IMAGE_H
#define NROS_BOARD_CAN_IMAGE_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/** Longest device name the store keeps, in bytes, without the NUL. A SocketCAN
 *  interface name is at most 15 (IFNAMSIZ - 1). */
#define NROS_BOARD_CAN_IMAGE_DEVICE_MAX 31u

/**
 * Store the image's device: `len` bytes at `name`, not NUL-terminated (a Rust
 * `&str` sliced out of the `NROS_IMAGE_NET` text). Called once, at boot, before
 * the application runs.
 *
 * Returns 0, or -1 when `name` is NULL, `len` is 0 or over
 * `NROS_BOARD_CAN_IMAGE_DEVICE_MAX`, or `name` holds a NUL. On -1 the store is
 * left empty, so `open(NULL)` reports no device rather than a truncated one.
 */
int8_t nros_board_can_set_image_device(const char* name, size_t len);

/** The stored device, NUL-terminated, or NULL when the image names none. */
const char* nros_board_can_image_device(void);

#ifdef __cplusplus
}
#endif

#endif /* NROS_BOARD_CAN_IMAGE_H */
