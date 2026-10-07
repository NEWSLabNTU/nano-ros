/*
 * Weak `nros_rmw_cffi_register_named` fallback for the uORB backend (issue 0335).
 *
 * The PX4-SITL build path links the uORB backend's C++ sources but NOT the Rust
 * `nros-rmw-cffi` staticlib that ships the real (strong)
 * `nros_rmw_cffi_register_named`, so `vtable.cpp`'s registration call would be
 * an unresolved symbol.
 *
 * This used to define the UNNAMED `nros_rmw_cffi_register`, which `vtable.cpp`
 * stopped calling when the named registry landed (phase 104.B.2), so the
 * fallback satisfied nothing. phase-482 W6 deleted the unnamed entry point and
 * moved the fallback to the symbol that is actually called. This weak
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

__attribute__((weak)) rmw_ret_t nros_rmw_cffi_register_named(const char* name,
                                                             const nros_rmw_vtable_t* vtable) {
    if (name == NULL || vtable == NULL) {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    return NROS_RMW_RET_OK;
}
