//! Issue 1577 — publish the one build-time fact `nros-node` needs about the
//! backends linked into this image: how do they deliver a subscription sample?
//!
//! Cargo exposes a crate's own features only to its OWN build script, and the
//! backend crates enable `in-place-dispatch` / `buffered-dispatch` on THIS
//! crate, not on `nros-node`. `links` metadata is Cargo's mechanism for handing
//! a fact to direct dependents: `nros-node/build.rs` reads it as
//! `DEP_NROS_RMW_IN_PLACE_DISPATCH` / `DEP_NROS_RMW_BUFFERED_DISPATCH`.
fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    // Features unify across the graph, so each is "ANY linked backend does".
    for (feature, key) in [
        ("CARGO_FEATURE_IN_PLACE_DISPATCH", "in_place_dispatch"),
        ("CARGO_FEATURE_BUFFERED_DISPATCH", "buffered_dispatch"),
    ] {
        let declared = std::env::var_os(feature).is_some();
        println!("cargo:{key}={}", u8::from(declared));
    }
}
