/*
 * SITL build scaffold (phase-244 D5) — NOT application logic.
 *
 * Hoisted out of the agnostic example source `nros_register_check.cpp` so that
 * file carries only the register-check itself. The PX4-SITL build path does not
 * link the Rust `nros-rmw-cffi` staticlib that ships the real (strong)
 * `nros_rmw_cffi_register`, so this weak stub satisfies the link and lets the
 * uORB backend's `vtable.cpp` register against a no-op registry. A real
 * (cargo-linked) build overrides it with the Rust strong symbol.
 *
 * Compiled into the module by the sibling `CMakeLists.txt`. C linkage (matches
 * the Rust `#[unsafe(no_mangle)] extern "C"` symbol it stands in for).
 */

#include "nros/rmw_vtable.h"

__attribute__((weak)) nros_rmw_ret_t nros_rmw_cffi_register(const nros_rmw_vtable_t *vtable) {
    if (vtable == NULL) {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    return NROS_RMW_RET_OK;
}
