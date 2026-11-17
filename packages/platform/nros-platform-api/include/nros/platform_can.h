#ifndef NROS_PLATFORM_CAN_H
#define NROS_PLATFORM_CAN_H

#include <stdint.h>
#include <stddef.h>

#include "nros/platform.h"

#ifdef __cplusplus
extern "C" {
#endif

/**
 * @file platform_can.h
 * @brief Canonical C ABI for the nros platform CAN frame driver.
 *
 * Companion to `<nros/platform.h>`. An application that sends its own CAN or
 * CAN FD frames (an actuator command, a heartbeat) sends them through these
 * five calls. Carved into a separate header because a board with no CAN never
 * includes it.
 *
 * # What the driver does and does not do
 *
 *  The driver opens a device that the board already brought up and moves
 *  frames on it. It sets no bitrate, starts no controller and states no
 *  layer-1 fact. Frame ids, payload layout, classic or FD, and any end-to-end
 *  protection belong to the application.
 *
 * # The backend
 *
 *  Exactly one backend defines these symbols in an image, and it follows from
 *  the board: Zephyr's CAN API on a Zephyr board, SocketCAN on a Linux-process
 *  board, the board package's own driver on a `"board"`-stack board, or the
 *  host-only mock where the image selects it.
 *
 * # Handle storage
 *
 *  `nros_platform_can_t` is caller storage of a size this header fixes. A
 *  backend keeps its state inside it and proves at compile time that the
 *  state fits (`NROS_PLATFORM_CAN_STORAGE_FITS`). Zero-initialise a handle
 *  before its first `open`; `close` returns it to the closed state. An open
 *  handle must not move: a backend may keep a kernel object (a semaphore, a
 *  message queue) inside it.
 *
 * # Return values
 *
 *  `0` (`NROS_PLATFORM_CAN_OK`) on success, a negative
 *  `NROS_PLATFORM_CAN_E*` code otherwise. `timeout_us = 0` never blocks.
 *  `send` returning `0` means the controller accepted the frame within
 *  `timeout_us`; with `NROS_PLATFORM_CAN_SEND_WAIT_TX` set in the frame's
 *  flags it means the frame left the controller. A backend that cannot report
 *  completion returns `NROS_PLATFORM_CAN_ENOTSUP` for that flag and never
 *  downgrades it to acceptance.
 */

/* ---- Frame flags ---- */

/** 29-bit (extended) identifier; clear for an 11-bit identifier. */
#define NROS_PLATFORM_CAN_FLAG_EXT 0x01u
/** A CAN FD frame. Legal only on a handle whose `fd_active` is true. */
#define NROS_PLATFORM_CAN_FLAG_FD 0x02u
/** FD bit-rate switch. Legal only together with `NROS_PLATFORM_CAN_FLAG_FD`. */
#define NROS_PLATFORM_CAN_FLAG_BRS 0x04u
/** `send` only: return once the frame left the controller, not when it was
 *  accepted. `NROS_PLATFORM_CAN_ENOTSUP` where the backend cannot tell. */
#define NROS_PLATFORM_CAN_SEND_WAIT_TX 0x80u

/** Every flag bit this ABI defines; any other bit makes a frame invalid. */
#define NROS_PLATFORM_CAN_FLAGS_KNOWN 0x87u

/** Largest 11-bit and 29-bit identifiers. */
#define NROS_PLATFORM_CAN_STD_ID_MAX 0x7FFu
#define NROS_PLATFORM_CAN_EXT_ID_MAX 0x1FFFFFFFu

/** Largest payloads, in bytes. */
#define NROS_PLATFORM_CAN_CLASSIC_MAX_LEN 8u
#define NROS_PLATFORM_CAN_FD_MAX_LEN 64u

/* ---- Return codes ---- */

/** Sent, received, opened. */
#define NROS_PLATFORM_CAN_OK 0
/** Invalid frame: identifier out of range for its width, a length that is not
 *  a valid classic or FD size, an unknown flag, BRS without FD, or an FD flag
 *  on a handle whose FD is not active. Also an invalid argument (NULL). */
#define NROS_PLATFORM_CAN_EINVAL (-1)
/** The handle is not open. */
#define NROS_PLATFORM_CAN_ENOTOPEN (-2)
/** `open` with `want_fd` on a device that does not carry CAN FD. */
#define NROS_PLATFORM_CAN_ENOFD (-3)
/** The transmit queue stayed full, or nothing arrived, within `timeout_us`. */
#define NROS_PLATFORM_CAN_EAGAIN (-4)
/** The controller is bus-off. */
#define NROS_PLATFORM_CAN_EBUSOFF (-5)
/** The backend does not provide this call or flag (`recv`,
 *  `NROS_PLATFORM_CAN_SEND_WAIT_TX`, or a board with no CAN backend). */
#define NROS_PLATFORM_CAN_ENOTSUP (-6)
/** `open`: the named device does not exist or is not ready. */
#define NROS_PLATFORM_CAN_ENODEV (-7)
/** Any other backend failure. */
#define NROS_PLATFORM_CAN_EIO (-8)

/* ---- Types ---- */

/** One CAN or CAN FD frame. `len` is the payload in BYTES, never a DLC: the
 *  backend maps bytes to the wire DLC, and a length that is not a valid size
 *  is refused, never padded (padding is payload layout, the application's). */
typedef struct {
    /** 11- or 29-bit identifier, per `NROS_PLATFORM_CAN_FLAG_EXT`. */
    uint32_t id;
    /** `NROS_PLATFORM_CAN_FLAG_*` / `NROS_PLATFORM_CAN_SEND_WAIT_TX`. */
    uint8_t flags;
    /** Payload bytes: 0..8 classic; 0..8, 12, 16, 20, 24, 32, 48, 64 FD. */
    uint8_t len;
    /** Payload; bytes past `len` are ignored on send and unspecified on recv. */
    uint8_t data[64];
} nros_platform_can_frame_t;

/** Handle storage in `uint64_t` words. ABI: changing it is a header edit, a
 *  `scripts/gen-abi-bindings.sh` run and a commit of both. Measured state per
 *  backend (bytes, x86_64): mock 8. */
#define NROS_PLATFORM_CAN_OPAQUE_U64S 16u

/** A CAN handle: caller storage, the backend's state lives inside it. */
typedef struct {
    uint64_t opaque[NROS_PLATFORM_CAN_OPAQUE_U64S];
} nros_platform_can_t;

/** One line in a backend (in-tree or a board package's) that fails the build
 *  when its state outgrows the handle, naming the constant to raise. */
#ifdef __cplusplus
#define NROS_PLATFORM_CAN_STORAGE_FITS(type)                                     \
    static_assert(sizeof(type) <= sizeof(nros_platform_can_t) &&                 \
                      alignof(type) <= alignof(nros_platform_can_t),             \
                  "NROS_PLATFORM_CAN_OPAQUE_U64S too small for " #type)
#else
#define NROS_PLATFORM_CAN_STORAGE_FITS(type)                                     \
    _Static_assert(sizeof(type) <= sizeof(nros_platform_can_t) &&                \
                       _Alignof(type) <= _Alignof(nros_platform_can_t),          \
                   "NROS_PLATFORM_CAN_OPAQUE_U64S too small for " #type)
#endif

/* ---- Calls ---- */

/** Open `device` into the caller's handle storage. `device` NULL opens the
 *  image's device (the one its board stack names); a non-NULL name opens that
 *  device instead. `want_fd` non-zero asks for CAN FD and fails with
 *  `NROS_PLATFORM_CAN_ENOFD` when the device does not carry it, so an image
 *  learns at open rather than at its first FD send. */
int8_t nros_platform_can_open(nros_platform_can_t *can, const char *device, uint8_t want_fd);

/** Send one frame, waiting at most `timeout_us` for the controller to take
 *  it (`0`: do not wait). See the file comment for what `0` means. */
int8_t nros_platform_can_send(nros_platform_can_t *can, const nros_platform_can_frame_t *frame,
                              uint32_t timeout_us);

/** Non-zero when the handle was opened with `want_fd` and the device carries
 *  CAN FD; zero otherwise, a closed handle included. */
uint8_t nros_platform_can_fd_active(const nros_platform_can_t *can);

/** Receive one frame, waiting at most `timeout_us` (`0`: poll).
 *  `NROS_PLATFORM_CAN_EAGAIN` when nothing arrived; `NROS_PLATFORM_CAN_ENOTSUP`
 *  on a backend without receive. */
int8_t nros_platform_can_recv(nros_platform_can_t *can, nros_platform_can_frame_t *frame,
                              uint32_t timeout_us);

/** Close the handle. Idempotent; a closed or never-opened handle is a no-op. */
void nros_platform_can_close(nros_platform_can_t *can);

/* ---- The one frame check every backend applies ---- */

/** `NROS_PLATFORM_CAN_OK` when `frame` is valid to send on a handle whose FD
 *  is active (`fd_active` non-zero) or not, `NROS_PLATFORM_CAN_EINVAL`
 *  otherwise. One definition, so every backend refuses the same frames. */
static inline int8_t nros_platform_can_frame_check(const nros_platform_can_frame_t *frame, uint8_t fd_active) {
    uint8_t flags;
    uint8_t len;
    if (frame == NULL) {
        return NROS_PLATFORM_CAN_EINVAL;
    }
    flags = frame->flags;
    len = frame->len;
    if ((flags & (uint8_t) ~NROS_PLATFORM_CAN_FLAGS_KNOWN) != 0u) {
        return NROS_PLATFORM_CAN_EINVAL;
    }
    if (frame->id > ((flags & NROS_PLATFORM_CAN_FLAG_EXT) ? NROS_PLATFORM_CAN_EXT_ID_MAX
                                                          : NROS_PLATFORM_CAN_STD_ID_MAX)) {
        return NROS_PLATFORM_CAN_EINVAL;
    }
    if ((flags & NROS_PLATFORM_CAN_FLAG_FD) == 0u) {
        if ((flags & NROS_PLATFORM_CAN_FLAG_BRS) != 0u || len > NROS_PLATFORM_CAN_CLASSIC_MAX_LEN) {
            return NROS_PLATFORM_CAN_EINVAL;
        }
        return NROS_PLATFORM_CAN_OK;
    }
    if (fd_active == 0u) {
        return NROS_PLATFORM_CAN_EINVAL;
    }
    if (len <= 8u || len == 12u || len == 16u || len == 20u || len == 24u || len == 32u ||
        len == 48u || len == 64u) {
        return NROS_PLATFORM_CAN_OK;
    }
    return NROS_PLATFORM_CAN_EINVAL;
}

#ifdef __cplusplus
}
#endif

#endif /* NROS_PLATFORM_CAN_H */
