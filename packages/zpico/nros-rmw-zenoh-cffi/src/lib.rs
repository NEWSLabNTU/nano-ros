//! Phase 115.L.2 — C-vtable shim for `nros-rmw-zenoh` (zenoh-pico).
//!
//! Exposes a single C entry point `nros_rmw_zenoh_register()` that
//! installs a [`RustBackendAdapter`]-monomorphised vtable for
//! [`ZenohRmw`] into the `nros-rmw-cffi` registry.
//!
//! Distinct from 115.K.3 (deferred): K.3 would re-implement the
//! backend in C. This crate keeps the Rust glue in `nros-rmw-zenoh`
//! and only adds the C-vtable facade.
//!
//! # Use from Rust
//!
//! ```ignore
//! nros_rmw_zenoh_cffi::register().expect("zenoh register failed");
//! // Subsequent Executor::open calls route through zenoh-pico via
//! // the C vtable.
//! ```
//!
//! # Use from C
//!
//! ```c
//! extern int nros_rmw_zenoh_register(void);
//! if (nros_rmw_zenoh_register() != NROS_RMW_RET_OK) { /* handle */ }
//! ```

#![no_std]

use core::ffi::c_int;

use nros_rmw_cffi::{NROS_RMW_RET_OK, NrosRmwRet, RustBackendAdapter};
use nros_rmw_zenoh::ZenohRmw;

/// C entry point — installs the zenoh-pico vtable into the cffi
/// runtime. Returns `NROS_RMW_RET_OK` (0) on success. Idempotent.
#[unsafe(no_mangle)]
pub extern "C" fn nros_rmw_zenoh_register() -> NrosRmwRet {
    RustBackendAdapter::<ZenohRmw>::register()
}

/// Failure mode for the safe Rust wrapper.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct RegisterError(pub c_int);

/// Safe Rust wrapper around [`nros_rmw_zenoh_register`]. Returns
/// `Err(RegisterError(rc))` when the runtime rejects the vtable.
pub fn register() -> Result<(), RegisterError> {
    let rc = nros_rmw_zenoh_register();
    if rc == NROS_RMW_RET_OK {
        Ok(())
    } else {
        Err(RegisterError(rc))
    }
}
