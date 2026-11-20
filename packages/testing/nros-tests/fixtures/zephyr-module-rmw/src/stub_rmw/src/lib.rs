//! `stub` -- an out-of-tree nano-ros RMW backend that is its own Zephyr module
//! (RFC-0140 D6, phase-581 W8).
//!
//! The transport is a stub: the table names the backend and leaves every
//! other slot empty, so the runtime refuses to open a session on it. What the
//! fixture exercises is the build, not the wire: the package's own Zephyr
//! module (`zephyr/`) joins the image's build through the nano-ros module hook,
//! its member `NROS_RMW_STUB` joins the module's RMW choice, and the image's
//! `rmw = "stub"` selects it.

#![no_std]

use core::ffi::c_char;

use nros_rmw_cffi::{
    EMPTY_VTABLE, NROS_RMW_RET_OK, NrosRmwRet, NrosRmwVtable, nros_rmw_cffi_register_named,
};

/// The table registered as `stub`.
static STUB_VTABLE: NrosRmwVtable = NrosRmwVtable {
    get_implementation_identifier: Some(stub_identifier),
    ..EMPTY_VTABLE
};

unsafe extern "C" fn stub_identifier() -> *const c_char {
    c"stub".as_ptr()
}

/// Failure to register `stub`: the runtime's return code.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct RegisterError(pub NrosRmwRet);

/// Register the `stub` backend.
///
/// # Errors
///
/// The runtime refused the table (registry full, or a required slot missing).
pub fn register() -> Result<(), RegisterError> {
    // SAFETY: `c"stub"` is NUL-terminated and `STUB_VTABLE` is a `static`.
    let rc = unsafe { nros_rmw_cffi_register_named(c"stub".as_ptr(), &STUB_VTABLE) };
    if rc == NROS_RMW_RET_OK {
        Ok(())
    } else {
        Err(RegisterError(rc))
    }
}
