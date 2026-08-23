/*
 * Weak `nros_rmw_cffi_register` fallback for the uORB backend (issue 0335).
 *
 * The PX4-SITL build path links the uORB backend's C++ sources but NOT the Rust
 * `nros-rmw-cffi` staticlib that ships the real (strong) `nros_rmw_cffi_register`,
 * so `vtable.cpp`'s registration call would be an unresolved symbol. This weak
 * definition satisfies the link with a no-op registry; a real (cargo-linked)
 * build overrides it with the Rust strong symbol. Weak = fallback, so this is
 * harmless when the staticlib IS linked.
 *
 * This belongs to the backend, not any example: it was previously
 * `sitl_register_stub.c` inside `packages/testing/nros-px4-register-check/`
 * (RFC-0026 J1 — no framework glue in examples). C linkage matches the Rust
 * `#[unsafe(no_mangle)] extern "C"` symbol it stands in for.
 */

#include "nros/rmw_vtable.h"

__attribute__((weak)) rmw_ret_t nros_rmw_cffi_register(const nros_rmw_vtable_t* vtable) {
    if (vtable == NULL) {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    return NROS_RMW_RET_OK;
}
