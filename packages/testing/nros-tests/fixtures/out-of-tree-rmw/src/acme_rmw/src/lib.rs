//! `acme` — an out-of-tree nano-ros RMW backend (RFC-0071 D16).
//!
//! It implements the C ABI seam (`nros_rmw_vtable_t`) by delegating to the
//! zenoh backend's Rust implementation, which keeps the fixture small: the
//! table is the generic `RustBackendAdapter` over `nros_rmw_zenoh::ZenohRmw`,
//! registered under the name `acme` with two slots of its own — the
//! implementation identifier (`acme`), and `publish`, wrapped so the first
//! sample published through this backend prints [`PUBLISH_MARKER`]. That is
//! what lets a test tell a sample that went through `acme` from one that went
//! straight to zenoh.
//!
//! It never calls zenoh's own `register()`. Linking zenoh still brings zenoh's
//! hosted self-registration (an `.init_array` ctor), so a hosted image holds
//! two backends; the image names the one it declared — `nros::main!` bakes
//! `rmw = "acme"` as the selector for an image whose entry registers its own
//! backend (RFC-0071 D15) — so no `$NROS_RMW` is needed.

#![no_std]

use core::{
    ffi::c_char,
    sync::atomic::{AtomicBool, Ordering},
};

use nros_rmw_cffi::{
    NROS_RMW_RET_OK, NrosRmwRet, NrosRmwVtable, RustBackendAdapter, nros_rmw_cffi_register_named,
    rmw_byte_span_t, rmw_publisher_t, rmw_ret_t,
};
use nros_rmw_zenoh::ZenohRmw;

/// Printed once, on the first sample published through `acme`.
pub const PUBLISH_MARKER: &str = "acme rmw: publishing through backend acme";

/// The zenoh table this backend delegates to.
const ZENOH: NrosRmwVtable = RustBackendAdapter::<ZenohRmw>::VTABLE;

/// The table registered as `acme`.
static ACME_VTABLE: NrosRmwVtable = NrosRmwVtable {
    get_implementation_identifier: Some(acme_identifier),
    publish: Some(acme_publish),
    ..ZENOH
};

static ANNOUNCED: AtomicBool = AtomicBool::new(false);

unsafe extern "C" fn acme_identifier() -> *const c_char {
    c"acme".as_ptr()
}

unsafe extern "C" fn acme_publish(
    publisher: *const rmw_publisher_t,
    payload: rmw_byte_span_t,
) -> rmw_ret_t {
    if !ANNOUNCED.swap(true, Ordering::AcqRel) {
        nros_log::log_info!(nros_log::get_logger("acme_rmw"), "{}", PUBLISH_MARKER);
    }
    match ZENOH.publish {
        // SAFETY: the runtime's arguments, forwarded unchanged to the slot
        // this table was built from.
        Some(inner) => unsafe { inner(publisher, payload) },
        None => nros_rmw_cffi::NROS_RMW_RET_ERROR,
    }
}

/// Failure to register `acme`: the runtime's return code.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct RegisterError(pub NrosRmwRet);

/// Register the `acme` backend. Idempotent: a second registration under the
/// same name overwrites the first.
///
/// # Errors
///
/// The runtime refused the table (registry full, or a required slot missing).
pub fn register() -> Result<(), RegisterError> {
    // SAFETY: `c"acme"` is NUL-terminated and `ACME_VTABLE` is a `static`.
    let rc = unsafe { nros_rmw_cffi_register_named(c"acme".as_ptr(), &ACME_VTABLE) };
    if rc == NROS_RMW_RET_OK {
        Ok(())
    } else {
        Err(RegisterError(rc))
    }
}
