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

nros::zephyr_component_main!(crate::AddTwoIntsClient);
