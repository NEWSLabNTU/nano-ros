//! `/rosout` from C — issue 1589.
//!
//! The Rust bridge (`nros_node::rosout`) in three calls, plus the predicate
//! rcl has. Same shape, same reasons: the application creates the publisher
//! (it is an ENTITY, so it reaches the inventory like any other — issue
//! 1341's shape is a publisher the runtime conjures below the declaration),
//! turns the queue on AFTER it, and pumps from its spin loop.
//!
//! ```c
//! nros_publisher_t rosout = rcl_get_zero_initialized_publisher();
//! nros_rosout_publisher_init(&rosout, &node, NULL);  // NULL = bounded QoS
//! nros_rosout_enable();
//! for (;;) {
//!     rclc_executor_spin_some(&executor, 10000000);
//!     nros_rosout_pump(&rosout, NULL);
//! }
//! ```
//!
//! An image built without the `rosout` capability still LINKS every symbol
//! here: the predicate answers `false` (upstream's contract for a build with
//! the bridge off) and the other three answer `NROS_RET_UNSUPPORTED`, so a
//! ported program compiles and says why at runtime, the way the C++
//! parameter forwarders do (phase-426 W4).
//!
//! The declarations are hand-authored in `<nros/log.h>` beside the rest of
//! the log surface, so cbindgen skips these names (`cbindgen.toml`).

use crate::{error::*, node::nros_node_t, publisher::nros_publisher_t, qos::nros_qos_t};

/// `rcl_logging_rosout_enabled()`: whether this image can publish `/rosout`.
///
/// `true` iff the image was built with the `rosout` capability. It says
/// nothing about whether [`nros_rosout_enable`] has been called — upstream's
/// predicate answers the same build-time question.
#[unsafe(no_mangle)]
pub extern "C" fn nros_logging_rosout_enabled() -> bool {
    cfg!(feature = "rosout")
}

/// Create the `/rosout` publisher on `node`: topic `/rosout`, type
/// `rcl_interfaces/msg/Log`.
///
/// `qos` NULL takes the bounded profile (`nros_node::rosout::qos_bounded`:
/// KEEP_LAST(queue depth), RELIABLE, VOLATILE, 10 s lifespan), which costs no
/// transient-local slot. Pass upstream's `rcl_qos_profile_rosout_default`
/// (TRANSIENT_LOCAL, KEEP_LAST(1000)) explicitly on a target that has budgeted
/// for it — `nros_rosout_qos_default()` returns it.
///
/// # Returns
/// Whatever `nros_publisher_init_with_qos` returns, or
/// `NROS_RET_UNSUPPORTED` in an image without the `rosout` capability.
///
/// # Safety
/// `publisher` points to a zero-initialised publisher, `node` to an
/// initialised node; `qos` is NULL or valid.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_rosout_publisher_init(
    publisher: *mut nros_publisher_t,
    node: *const nros_node_t,
    qos: *const nros_qos_t,
) -> nros_ret_t {
    #[cfg(feature = "rosout")]
    {
        use core::ffi::c_char;
        use nros_node::rosout;

        // The identity crosses as C strings, so it is NUL-terminated here
        // once; the generated constants carry no terminator.
        const NAME_CAP: usize = rosout::TYPE_NAME.len() + 1;
        const HASH_CAP: usize = rosout::TYPE_HASH.len() + 1;
        let mut name = [0u8; NAME_CAP];
        let mut hash = [0u8; HASH_CAP];
        name[..rosout::TYPE_NAME.len()].copy_from_slice(rosout::TYPE_NAME.as_bytes());
        hash[..rosout::TYPE_HASH.len()].copy_from_slice(rosout::TYPE_HASH.as_bytes());
        let type_info = crate::publisher::nros_message_type_t {
            type_name: name.as_ptr() as *const c_char,
            type_hash: hash.as_ptr() as *const c_char,
            serialized_size_max: rosout::TX_BUF,
        };

        let bounded;
        let qos = if qos.is_null() {
            bounded = nros_qos_t::from_qos_settings(rosout::qos_bounded());
            &bounded as *const nros_qos_t
        } else {
            qos
        };
        // "/rosout\0" — absolute, so no namespace or remap moves it.
        crate::publisher::nros_publisher_init_with_qos(
            publisher,
            node,
            &type_info,
            c"/rosout".as_ptr(),
            qos,
        )
    }
    #[cfg(not(feature = "rosout"))]
    {
        let _ = (publisher, node, qos);
        NROS_RET_UNSUPPORTED
    }
}

/// Upstream's `rcl_qos_profile_rosout_default`: KEEP_LAST(1000), RELIABLE,
/// TRANSIENT_LOCAL, 10 s lifespan. Correct, and expensive on an embedded
/// target — see `nros_rosout_publisher_init`.
#[unsafe(no_mangle)]
pub extern "C" fn nros_rosout_qos_default() -> nros_qos_t {
    #[cfg(feature = "rosout")]
    {
        nros_qos_t::from_qos_settings(nros_node::rosout::qos())
    }
    #[cfg(not(feature = "rosout"))]
    {
        crate::qos::NROS_QOS_DEFAULT
    }
}

/// Start queueing log records for `/rosout`, scoped to what this image's ROS
/// release publishes (RFC-0102 D4). Call it AFTER
/// `nros_rosout_publisher_init`: a queue with nowhere to drain only counts
/// drops.
///
/// # Returns
/// `NROS_RET_OK`; `NROS_RET_ERROR` if the queueing sink could not be
/// registered (the sink table is full); `NROS_RET_UNSUPPORTED` without the
/// `rosout` capability.
#[unsafe(no_mangle)]
pub extern "C" fn nros_rosout_enable() -> nros_ret_t {
    #[cfg(feature = "rosout")]
    {
        if nros_node::rosout::enable() {
            NROS_RET_OK
        } else {
            NROS_RET_ERROR
        }
    }
    #[cfg(not(feature = "rosout"))]
    {
        NROS_RET_UNSUPPORTED
    }
}

/// Publish every queued record on `publisher`. Call from the spin loop. Never
/// blocks on the queue and never allocates.
///
/// `out_sent`, if non-NULL, receives how many messages reached the transport
/// — also on failure, where it counts those before the first refusal.
///
/// # Returns
/// `NROS_RET_OK`; `NROS_RET_PUBLISH_FAILED` on the first transport refusal
/// (the rest of the queue is still drained, so it never wedges);
/// `NROS_RET_UNSUPPORTED` without the `rosout` capability.
///
/// # Safety
/// `publisher` was initialised by `nros_rosout_publisher_init`; `out_sent`
/// is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_rosout_pump(
    publisher: *const nros_publisher_t,
    out_sent: *mut usize,
) -> nros_ret_t {
    #[cfg(feature = "rosout")]
    {
        validate_not_null!(publisher);
        let (sent, ret) = match nros_node::rosout::pump_raw(|bytes| {
            match crate::publisher::nros_publish_raw(publisher, bytes.as_ptr(), bytes.len()) {
                NROS_RET_OK => Ok(()),
                _ => Err(()),
            }
        }) {
            Ok(n) => (n, NROS_RET_OK),
            Err((n, _)) => (n, NROS_RET_PUBLISH_FAILED),
        };
        if !out_sent.is_null() {
            *out_sent = sent;
        }
        ret
    }
    #[cfg(not(feature = "rosout"))]
    {
        let _ = publisher;
        if !out_sent.is_null() {
            *out_sent = 0;
        }
        NROS_RET_UNSUPPORTED
    }
}
