/*
 * Phase 104.B.6 — weak default of `nros_app_register_backends`.
 *
 * nros-c's `nros_support_init` calls this symbol unconditionally.
 * Two paths resolve it:
 *
 *   (1) POSIX / macOS / Windows host builds: the .init_array ctor
 *       inside each backend's wrapper staticlib has already
 *       registered the backend before main(). The weak default
 *       below fires as a no-op. Idempotent.
 *
 *   (2) Bare-metal targets whose startup doesn't walk .init_array:
 *       CMake's `nano_ros_link_rmw(target NAME <rmw>)` writes a
 *       stub C file into the user's target that provides a STRONG
 *       def of `nros_app_register_backends`, calling each linked
 *       backend's `nros_rmw_<x>_register` fn. The strong def
 *       overrides this weak one.
 *
 * Either path produces a registered backend before
 * `nros_support_init` opens the session.
 */

#if defined(__GNUC__) || defined(__clang__)
__attribute__((weak))
#endif
void nros_app_register_backends(void) {
    /* Intentionally empty — bare-metal stub overrides via strong def. */
}

/*
 * Workspace test / metadata builds can link nros-c without selecting a
 * platform crate. nros-log's default sink still references the platform log
 * ABI, so provide weak no-op fallbacks for that no-platform link path. Real
 * platform crates export strong definitions and override these.
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
