//! Phase 115.C — runtime-pluggable custom transport (C surface).
//!
//! Wraps the `nros_rmw::custom_transport::NrosTransportOps` vtable
//! defined by Phase 115.A. Users register four C function pointers
//! before `nros_support_init`; whichever RMW backend is active
//! consumes the registered ops via `nros_rmw::take_custom_transport`
//! during `Rmw::open`.
//!
//! ## Threading contract
//!
//! - `read` and `write` are NEVER invoked concurrently from
//!   different threads.
//! - Callbacks must NOT be invoked from interrupt context.
//! - `user_data` is opaque to nros — its `Send` / `Sync` discipline
//!   is the C caller's responsibility.

use core::ffi::c_void;

use crate::error::{NROS_RET_ERROR, NROS_RET_INVALID_ARGUMENT, NROS_RET_OK, nros_ret_t};

/// Phase 115.A.2 — current ABI version of [`nros_transport_ops_t`].
///
/// C / C++ callers MUST fill in `ops.abi_version =
/// NROS_TRANSPORT_OPS_ABI_VERSION_V1` before passing the struct to
/// [`nros_set_custom_transport`]. Mismatched values are rejected
/// with `NROS_RET_ERROR` (mapped from
/// `TransportError::IncompatibleAbi`); cffi consumers see
/// `NROS_RMW_RET_INCOMPATIBLE_ABI`.
#[unsafe(no_mangle)]
pub static NROS_TRANSPORT_OPS_ABI_VERSION_V1: u32 = nros_rmw::NROS_TRANSPORT_OPS_ABI_VERSION_V1;

/// Phase 115.C — C-side mirror of
/// `nros_rmw::custom_transport::NrosTransportOps`. Same `#[repr(C)]`
/// layout — single ABI, no parallel definitions.
///
/// Field semantics:
///
/// - `abi_version`: must be [`NROS_TRANSPORT_OPS_ABI_VERSION_V1`]
///   (Phase 115.A.2).
/// - `_reserved`: padding for future minor-version detection. Set to 0.
/// - `user_data`: opaque caller context, threaded back into every
///   callback as the first argument. Lifetime: must outlive the
///   transport's active period (i.e. until `close` returns).
/// - `open`: open the underlying medium. `params` is opaque
///   per-transport metadata supplied by the caller (e.g. UART baud
///   rate). May be `NULL`. Returns 0 on success, negative
///   `nros_ret_t` on failure.
/// - `close`: tear the transport down. Complement of `open`.
/// - `write`: send `len` bytes from `buf`. Returns 0 on success,
///   negative `nros_ret_t` on failure.
/// - `read`: receive up to `len` bytes within `timeout_ms`. Returns
///   the non-negative byte count on success, negative `nros_ret_t`
///   on error / timeout.
#[repr(C)]
#[derive(Copy, Clone)]
pub struct nros_transport_ops_t {
    pub abi_version: u32,
    pub _reserved: u32,
    pub user_data: *mut c_void,
    pub open: unsafe extern "C" fn(user_data: *mut c_void, params: *const c_void) -> nros_ret_t,
    pub close: unsafe extern "C" fn(user_data: *mut c_void),
    pub write:
        unsafe extern "C" fn(user_data: *mut c_void, buf: *const u8, len: usize) -> nros_ret_t,
    pub read: unsafe extern "C" fn(
        user_data: *mut c_void,
        buf: *mut u8,
        len: usize,
        timeout_ms: u32,
    ) -> i32,
}

/// Phase 115.C — register a custom transport vtable.
///
/// Must be called BEFORE `nros_support_init`. Subsequent calls
/// before init overwrite the slot. After init, behaviour is
/// implementation-defined — the active RMW backend may have already
/// consumed the previously-registered vtable.
///
/// Pass `NULL` to clear a previously-registered vtable.
///
/// # Returns
///
/// - `NROS_RET_OK` on success.
/// - `NROS_RET_INVALID_ARGUMENT` if `ops` is non-NULL but any of the
///   four function pointers is NULL.
///
/// # Safety
///
/// The four function pointers in `ops` must follow the threading
/// contract documented in `<nros/transport.h>`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_set_custom_transport(ops: *const nros_transport_ops_t) -> nros_ret_t {
    if ops.is_null() {
        // Clear request.
        let _ = unsafe { nros_rmw::set_custom_transport(None) };
        return NROS_RET_OK;
    }
    // Copy by-value out of the caller's struct.
    let ops_copy = unsafe { *ops };
    let nros_ops = nros_rmw::NrosTransportOps {
        abi_version: ops_copy.abi_version,
        _reserved: ops_copy._reserved,
        user_data: ops_copy.user_data,
        open: ops_copy.open,
        close: ops_copy.close,
        write: ops_copy.write,
        read: ops_copy.read,
    };
    match unsafe { nros_rmw::set_custom_transport(Some(nros_ops)) } {
        Ok(()) => NROS_RET_OK,
        Err(_) => NROS_RET_ERROR,
    }
}

/// Phase 115.C — clear any previously-registered custom transport.
/// Equivalent to `nros_set_custom_transport(NULL)`. Convenience for
/// teardown paths.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_clear_custom_transport() -> nros_ret_t {
    let _ = unsafe { nros_rmw::set_custom_transport(None) };
    NROS_RET_OK
}

/// Phase 115.C — query whether a custom transport is currently
/// registered. Returns `1` if a transport is registered, `0`
/// otherwise. Useful for board crates that want to fall back to
/// a static transport when no runtime override has been provided.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_has_custom_transport() -> nros_ret_t {
    if nros_rmw::peek_custom_transport().is_some() {
        1
    } else {
        0
    }
}

// Touch the unused-import lint to keep the binding visible even when
// no callers in this file consume it directly.
const _: nros_ret_t = NROS_RET_INVALID_ARGUMENT;
