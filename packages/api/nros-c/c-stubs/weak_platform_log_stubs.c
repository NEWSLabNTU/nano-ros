/*
 * Weak fallbacks for the platform log ABI (`nros_platform_log_{write,flush}`).
 *
 * nros-log's default sink references these symbols, but workspace test /
 * metadata builds can link nros-c WITHOUT selecting a platform crate (which is
 * what normally supplies the strong definitions). These weak no-ops satisfy that
 * no-platform link path; a real platform crate exports strong defs and overrides
 * them.
 *
 * History: this TU once also carried the weak default of
 * `nros_app_register_backends` — REMOVED in phase-249 P4a (issue 0050 W3.1).
 * Backend registration is now the single generated STRONG def emitted by
 * `nano_ros_link_rmw()` (universal per `nros_platform_link_app`, phase-249 P2b),
 * so a missing registration is a LINK ERROR, not a silent no-op (the #48-class
 * hazard) — there is no weak fallback to delete here anymore.
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
