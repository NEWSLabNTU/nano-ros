//! phase-338 W2 — platform boot glue for the Zephyr staticlib path.
//!
//! Isolated from `lib.rs` so the node logic there is byte-identical to every
//! other Zephyr copy (the `example_portability` gate compares logic files and
//! ignores glue).
//!
//! It cannot live in a `src/main.rs`: Zephyr's build system links this crate as
//! a **staticlib** and calls in, so there is no bin target. The glue is a
//! module of the lib.
//!
//! `force_link_backend!` is a DCE anchor, not a registration: rustc drops a
//! dependency's `#[no_mangle]` exports from the `.a` without a direct
//! reference (issues 0155 / 0163). Registration is still
//! `nros_app_register_backends`.

extern crate zephyr;

#[cfg(feature = "rmw-zenoh")]
nros::force_link_backend!(nros_rmw_zenoh);
#[cfg(feature = "rmw-xrce")]
nros::force_link_backend!(nros_rmw_xrce_cffi);

nros::zephyr_component_main!(crate::FibonacciServer);

/// issue 0902 / phase-455 W4 — this image's zenoh reply-slot refusal total, or
/// `None` where the build links no zenoh shim.
///
/// The counter lives in the C shim of the process that owns the queryable, so
/// nothing outside this image can read it: a completion rate measured without
/// it is the inference issue 0902 says is unfalsifiable ("did results arrive"
/// instead of "did a slot run out"). Reported from `tick()`, on a cadence.
///
/// `None` rather than `0`, exactly as `bins/action-server-concurrent` does it
/// on the native lane: the cyclonedds and xrce builds of this leaf have no
/// reply-slot table at all, and a probe that reports the safe value when it
/// cannot measure is a probe that cannot fail. The caller prints the
/// distinction, so a consumer asserting `refusals=0` is RED on the build that
/// could never have answered.
///
/// It lives in the glue rather than in `lib.rs` because naming a backend is
/// what this file already does (`force_link_backend!` above); the node body
/// stays RMW-agnostic and keeps comparing clean against its portability group.
#[cfg(feature = "rmw-zenoh")]
pub fn reply_slot_refusals() -> Option<u32> {
    Some(nros_rmw_zenoh::reply_slot_refusals_total())
}

#[cfg(not(feature = "rmw-zenoh"))]
pub fn reply_slot_refusals() -> Option<u32> {
    None
}
