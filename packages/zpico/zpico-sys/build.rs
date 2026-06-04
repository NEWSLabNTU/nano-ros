//! Build script entrypoint for zpico-sys.
//!
//! The implementation lives in `nros-zpico-build` so the parsing,
//! generation, and source-selection helpers can be unit-tested outside
//! Cargo's build-script harness.

fn main() {
    nros_zpico_build::runner::run();
}
