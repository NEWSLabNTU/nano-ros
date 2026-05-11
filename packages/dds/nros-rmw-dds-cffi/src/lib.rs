//! Phase 115.L.1 — C-vtable shim for `nros-rmw-dds` (dust-DDS).
//!
//! Exposes a single C entry point `nros_rmw_dds_register()` that
//! installs a [`RustBackendAdapter`]-monomorphised vtable for
//! [`DdsRmw`] into the `nros-rmw-cffi` registry. After this call, the
//! cffi runtime's `CffiSession` / `CffiPublisher` / etc. route all
//! operations through dust-dds.
//!
//! # Use from Rust
//!
//! ```ignore
//! nros_rmw_dds_cffi::register().expect("nros-rmw-dds register failed");
//! // Subsequent Executor::open calls will route through dust-dds via
//! // the C vtable.
//! ```
//!
//! # Use from C
//!
//! Link against this crate's static archive and call:
//! ```c
//! extern int nros_rmw_dds_register(void);
//! if (nros_rmw_dds_register() != NROS_RMW_RET_OK) { /* handle */ }
//! ```

#![no_std]

use core::ffi::c_int;

use nros_rmw_cffi::{NROS_RMW_RET_OK, NrosRmwRet, RustBackendAdapter};
use nros_rmw_dds::DdsRmw;

/// C entry point — installs the dust-DDS vtable into the cffi runtime.
/// Returns `NROS_RMW_RET_OK` (0) on success.
///
/// Idempotent: re-registering the same vtable is a no-op from the
/// runtime's perspective.
#[unsafe(no_mangle)]
pub extern "C" fn nros_rmw_dds_register() -> NrosRmwRet {
    RustBackendAdapter::<DdsRmw>::register()
}

/// Failure mode for the safe Rust wrapper.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct RegisterError(pub c_int);

/// Safe Rust wrapper around [`nros_rmw_dds_register`]. Returns
/// `Err(RegisterError(rc))` when the runtime rejects the vtable
/// (e.g. future ABI-version mismatch).
pub fn register() -> Result<(), RegisterError> {
    let rc = nros_rmw_dds_register();
    if rc == NROS_RMW_RET_OK {
        Ok(())
    } else {
        Err(RegisterError(rc))
    }
}
