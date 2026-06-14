/*
 * Phase 249 P4a (issue 0050 W3.1) — the weak default of
 * `nros_app_register_backends` was REMOVED. C/C++ registration is now the single
 * generated STRONG def emitted by `nano_ros_link_rmw()` — universal for every
 * C/C++ app via `nros_platform_link_app` (phase-249 P2b) — which calls each
 * linked backend's `nros_rmw_<x>_register`. With no weak fallback, a target that
 * fails to emit the strong def is a LINK ERROR (undefined `nros_app_register_
 * backends`), not a silent no-op that opens the session with no backend (the
 * #48-class hazard). `nros_support_init` still calls the symbol unconditionally.
 *
 * This file now only carries the no-platform log-ABI weak fallbacks below:
 * workspace test / metadata builds can link nros-c without selecting a platform
 * crate; nros-log's default sink references the platform log ABI, so these weak
 * no-ops satisfy that no-platform link path. Real platform crates export strong
 * definitions and override them.
 */

#include <stdint.h>

#if defined(__GNUC__) || defined(__clang__)
__attribute__((weak))
#endif
void nros_platform_log_write(uint8_t severity,
                             const uint8_t *name_ptr, uintptr_t name_len,
                             const uint8_t *msg_ptr, uintptr_t msg_len) {
    (void) severity;
    (void) name_ptr;
    (void) name_len;
    (void) msg_ptr;
    (void) msg_len;
}

#if defined(__GNUC__) || defined(__clang__)
__attribute__((weak))
#endif
void nros_platform_log_flush(void) {
}
