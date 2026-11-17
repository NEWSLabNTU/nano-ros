/*
 * RFC-0132 -- the mock's frame check is the header's, not a second copy.
 *
 * `nros_platform_can_frame_check` is a `static inline` in <nros/platform_can.h>,
 * the one definition every C backend applies. Rust cannot call an inline, so
 * this file gives it one exported name the mock calls. A Rust re-implementation
 * would be a second statement of which frames are valid, free to drift.
 */

#include <nros/platform_can.h>

int8_t nros_can_mock_frame_check(const nros_platform_can_frame_t *frame, uint8_t fd_active);

int8_t nros_can_mock_frame_check(const nros_platform_can_frame_t *frame, uint8_t fd_active) {
    return nros_platform_can_frame_check(frame, fd_active);
}
