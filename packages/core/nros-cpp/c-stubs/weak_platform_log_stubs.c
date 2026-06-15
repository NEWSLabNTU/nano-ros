/*
 * Weak fallbacks for the platform log ABI (`nros_platform_log_{write,flush}`),
 * mirroring nros-c's `weak_platform_log_stubs.c`. nros-log's default sink
 * references these symbols; workspace test / metadata builds can link nros-cpp
 * WITHOUT a platform crate (the normal source of the strong defs), so these weak
 * no-ops satisfy that no-platform link path. A real platform crate exports strong
 * defs and overrides them (weak defs from nros-c + nros-cpp coexist — the linker
 * keeps one; a strong platform def wins).
 *
 * History: this TU once carried the weak default of `nros_app_register_backends`
 * — REMOVED in phase-249 P4a (issue 0050 W3.1). `nros_cpp_init` calls the symbol
 * before opening the CFFI RMW session; it is now the single generated STRONG def
 * from `nano_ros_link_rmw()` (universal per `nros_platform_link_app`, phase-249
 * P2b), so a missing registration is a LINK ERROR, not a silent no-op.
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
