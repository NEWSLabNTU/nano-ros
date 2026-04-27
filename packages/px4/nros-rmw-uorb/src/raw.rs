//! Direct typed pub/sub API — the **primary** way to use uORB from nano-ros.
//!
//! Users generate PX4 message types via `px4-msg-codegen` (`#[px4_message(…)]`)
//! and call [`publication`] / [`subscription`] to obtain typed handles
//! straight from `px4-uorb`. No CDR, no type erasure, no trampoline
//! registry — the same `Publication<T>` / `Subscription<T>` instances PX4
//! C++ modules use, addressed by ROS 2 topic name.
//!
//! ```ignore
//! use px4_uorb::UorbTopic;
//! use nros_rmw_uorb::{publication, subscription};
//!
//! #[px4_msg_macros::px4_message("./msg/SensorPing.msg")]
//! pub struct sensor_ping;
//!
//! let pub_  = publication::<sensor_ping>("/fmu/out/sensor_ping", 0)?;
//! let sub   = subscription::<sensor_ping>("/fmu/out/sensor_ping", 0)?;
//!
//! pub_.publish(&SensorPing { seq: 0, .. })?;
//! if let Some(msg) = sub.try_recv() { /* … */ }
//! ```
//!
//! [`crate::register`] + the [`crate::UorbSession`] trampoline registry
//! exist for **nros-node compatibility** — they bridge `publish_raw(&[u8])`
//! onto the same typed instances. Most users won't need them; reach for
//! [`publication`]/[`subscription`] first.

use core::ffi::CStr;

use nros_rmw::TransportError;
use px4_sys::orb_metadata;
use px4_uorb::{Publication, Subscription, UorbTopic};

use crate::topics::lookup_topic;

/// Construct a typed [`Publication`] for `ros_name`.
///
/// Validates that:
/// - `ros_name` is mapped in `topics.toml`.
/// - The mapped uORB name matches `T::metadata().o_name` (catches the case
///   where the user's `[[topic]]` entry points at a different uORB topic
///   from the `T` they passed).
///
/// Returns [`TransportError::InvalidConfig`] on either mismatch. The
/// returned `Publication` lazy-advertises on first publish; nothing
/// observable happens at the call site.
pub fn publication<T: UorbTopic>(
    ros_name: &str,
    instance: u8,
) -> Result<Publication<T>, TransportError> {
    let entry = lookup_topic(ros_name).ok_or(TransportError::InvalidConfig)?;
    verify_meta_matches::<T>(entry.uorb_name)?;
    let _ = instance; // Publication doesn't take instance; it's set via advertise_multi later
    Ok(Publication::<T>::new())
}

/// Construct a typed [`Subscription`] for `ros_name` at `instance`.
///
/// Same validation rules as [`publication`].
pub fn subscription<T: UorbTopic>(
    ros_name: &str,
    instance: u8,
) -> Result<Subscription<T>, TransportError> {
    let entry = lookup_topic(ros_name).ok_or(TransportError::InvalidConfig)?;
    verify_meta_matches::<T>(entry.uorb_name)?;
    // px4_uorb's `Subscription<T>::new()` defaults to instance 0; only call
    // `with_instance` when the caller asked for a non-zero instance so we
    // exercise the same code path as upstream gyro_watch / heartbeat
    // examples for the common single-instance case.
    if instance == 0 {
        Ok(Subscription::<T>::new())
    } else {
        Ok(Subscription::<T>::with_instance(instance))
    }
}

fn verify_meta_matches<T: UorbTopic>(expected_uorb_name: &str) -> Result<(), TransportError> {
    let meta: &orb_metadata = T::metadata();
    // SAFETY: o_name is a 'static C string per uORB convention.
    let actual = unsafe { CStr::from_ptr(meta.o_name) }
        .to_str()
        .map_err(|_| TransportError::InvalidConfig)?;
    if actual == expected_uorb_name {
        Ok(())
    } else {
        Err(TransportError::Backend(
            "uORB: topics.toml uorb name does not match T::metadata().o_name",
        ))
    }
}
