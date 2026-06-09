//! Build script for nros-board-nuttx.
//!
//! Sole job: make Cargo re-compile this crate when the compile-time
//! baked connection config changes. `run_entry` reads the locator/domain
//! via `option_env!("NROS_LOCATOR")` / `option_env!("NROS_DOMAIN_ID")`
//! because the NuttX QEMU guest has no runtime environment to read from
//! (see the comment at the `ExecutorConfig` site). Cargo does NOT track
//! `option_env!` inputs on its own, so without these directives a changed
//! `NROS_LOCATOR` would silently reuse a stale object with the old baked
//! value.
fn main() {
    println!("cargo:rerun-if-env-changed=NROS_LOCATOR");
    println!("cargo:rerun-if-env-changed=NROS_DOMAIN_ID");
}
