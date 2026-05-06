//! Phase 115.D — C++ FFI for the runtime-pluggable custom transport.
//!
//! Mirrors the C-side wrappers in `nros-c/src/transport.rs` so the
//! C++ surface (`nros::TransportOps`, `nros::set_custom_transport`,
//! `nros::clear_custom_transport`, `nros::has_custom_transport`)
//! lands without depending on `nros-c` directly.

use core::ffi::c_void;

use crate::{NROS_CPP_RET_OK, nros_cpp_ret_t};

/// Phase 115.D — C++-side mirror of
/// `nros_rmw::custom_transport::NrosTransportOps`. Same `#[repr(C)]`
/// layout — single ABI, no parallel definitions.
#[repr(C)]
#[derive(Copy, Clone)]
pub struct nros_cpp_transport_ops_t {
    pub user_data: *mut c_void,
    pub open: unsafe extern "C" fn(user_data: *mut c_void, params: *const c_void) -> nros_cpp_ret_t,
    pub close: unsafe extern "C" fn(user_data: *mut c_void),
    pub write:
        unsafe extern "C" fn(user_data: *mut c_void, buf: *const u8, len: usize) -> nros_cpp_ret_t,
    pub read: unsafe extern "C" fn(
        user_data: *mut c_void,
        buf: *mut u8,
        len: usize,
        timeout_ms: u32,
    ) -> i32,
}

/// Phase 115.D — register a custom transport vtable. C++-side entry
/// for `nros::set_custom_transport`.
///
/// # Safety
///
/// `ops`, when non-NULL, must point to a valid `nros_cpp_transport_ops_t`.
/// The four function pointers must follow the threading contract
/// documented in `<nros/transport.hpp>`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_set_custom_transport(
    ops: *const nros_cpp_transport_ops_t,
) -> nros_cpp_ret_t {
    if ops.is_null() {
        unsafe { nros_rmw::set_custom_transport(None) };
        return NROS_CPP_RET_OK;
    }
    let ops_copy = unsafe { *ops };
    let nros_ops = nros_rmw::NrosTransportOps {
        user_data: ops_copy.user_data,
        open: ops_copy.open,
        close: ops_copy.close,
        write: ops_copy.write,
        read: ops_copy.read,
    };
    unsafe { nros_rmw::set_custom_transport(Some(nros_ops)) };
    NROS_CPP_RET_OK
}

/// Phase 115.D — clear any previously-registered transport.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_clear_custom_transport() -> nros_cpp_ret_t {
    unsafe { nros_rmw::set_custom_transport(None) };
    NROS_CPP_RET_OK
}

/// Phase 115.D — return `1` if a transport is registered, `0`
/// otherwise. Returned as `nros_cpp_ret_t` for ABI parity with the
/// other entries.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_has_custom_transport() -> nros_cpp_ret_t {
    if nros_rmw::peek_custom_transport().is_some() {
        1
    } else {
        0
    }
}
