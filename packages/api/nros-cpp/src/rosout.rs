//! `/rosout` from C++ — issue 1589. The runtime half of `<nros/rosout.hpp>`.
//!
//! The publisher is an ordinary C++ publisher (`CppPublisher` in caller
//! storage, created through `nros_cpp_publisher_create`), named by
//! `rcl_interfaces/msg/Log`'s wire identity rather than by a generated C++
//! type — the tree generates that message for Rust only, and the bridge never
//! hands the C++ side a `Log` to fill: [`nros_node::rosout::pump_raw`] encodes
//! each record, so all three languages share one `Log` mapping.
//!
//! Every symbol links without the `rosout` capability and answers
//! `NROS_CPP_RET_UNSUPPORTED` / `false`, the phase-426 W4 rule for a surface
//! an image may not have compiled in.

use core::ffi::c_void;

#[cfg(not(feature = "rosout"))]
use crate::NROS_CPP_RET_UNSUPPORTED;
use crate::{nros_cpp_node_t, nros_cpp_qos_t, nros_cpp_ret_t};

/// `rcl_logging_rosout_enabled()`: true iff this image was built with the
/// `rosout` capability.
#[unsafe(no_mangle)]
pub extern "C" fn nros_cpp_rosout_enabled() -> bool {
    cfg!(feature = "rosout")
}

/// Create the `/rosout` publisher into `storage` (a C++ publisher slot).
/// `qos` NULL takes the bounded profile (`nros_node::rosout::qos_bounded`).
///
/// # Safety
/// `node` is an initialised node handle, `storage` a publisher slot of
/// `NROS_PUBLISHER_SIZE` bytes, `qos` NULL or valid.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_rosout_publisher_create(
    node: *const nros_cpp_node_t,
    qos: *const nros_cpp_qos_t,
    storage: *mut c_void,
) -> nros_cpp_ret_t {
    #[cfg(feature = "rosout")]
    {
        use core::ffi::c_char;
        use nros_node::rosout;

        const NAME_CAP: usize = rosout::TYPE_NAME.len() + 1;
        const HASH_CAP: usize = rosout::TYPE_HASH.len() + 1;
        let mut name = [0u8; NAME_CAP];
        let mut hash = [0u8; HASH_CAP];
        name[..rosout::TYPE_NAME.len()].copy_from_slice(rosout::TYPE_NAME.as_bytes());
        hash[..rosout::TYPE_HASH.len()].copy_from_slice(rosout::TYPE_HASH.as_bytes());
        let qos = if qos.is_null() {
            nros_cpp_qos_t::from_qos_settings(rosout::qos_bounded())
        } else {
            unsafe { *qos }
        };
        unsafe {
            crate::publisher::nros_cpp_publisher_create(
                node,
                c"/rosout".as_ptr(),
                name.as_ptr() as *const c_char,
                hash.as_ptr() as *const c_char,
                qos,
                storage,
            )
        }
    }
    #[cfg(not(feature = "rosout"))]
    {
        let _ = (node, qos, storage);
        NROS_CPP_RET_UNSUPPORTED
    }
}

/// Start queueing records for `/rosout`, scoped by the image's ROS release
/// (RFC-0102 D4). `NROS_CPP_RET_ERROR` if the queueing sink could not be
/// registered.
#[unsafe(no_mangle)]
pub extern "C" fn nros_cpp_rosout_enable() -> nros_cpp_ret_t {
    #[cfg(feature = "rosout")]
    {
        if nros_node::rosout::enable() {
            crate::NROS_CPP_RET_OK
        } else {
            crate::NROS_CPP_RET_ERROR
        }
    }
    #[cfg(not(feature = "rosout"))]
    {
        NROS_CPP_RET_UNSUPPORTED
    }
}

/// Publish every queued record on the publisher in `storage`. `out_sent`, if
/// non-NULL, receives how many reached the transport (on failure: before the
/// first refusal).
///
/// # Safety
/// `storage` was filled by [`nros_cpp_rosout_publisher_create`]; `out_sent`
/// is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_rosout_pump(
    storage: *mut c_void,
    out_sent: *mut usize,
) -> nros_cpp_ret_t {
    #[cfg(feature = "rosout")]
    {
        if storage.is_null() {
            return crate::NROS_CPP_RET_INVALID_ARGUMENT;
        }
        let (sent, ret) = match nros_node::rosout::pump_raw(|bytes| {
            match unsafe {
                crate::publisher::nros_cpp_publish_raw(storage, bytes.as_ptr(), bytes.len())
            } {
                crate::NROS_CPP_RET_OK => Ok(()),
                _ => Err(()),
            }
        }) {
            Ok(n) => (n, crate::NROS_CPP_RET_OK),
            Err((n, _)) => (n, crate::NROS_CPP_RET_PUBLISH_FAILED),
        };
        if !out_sent.is_null() {
            unsafe { *out_sent = sent };
        }
        ret
    }
    #[cfg(not(feature = "rosout"))]
    {
        let _ = storage;
        if !out_sent.is_null() {
            unsafe { *out_sent = 0 };
        }
        NROS_CPP_RET_UNSUPPORTED
    }
}
