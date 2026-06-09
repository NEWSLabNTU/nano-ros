//! Links the bake-generated cross-language shared-state surface into one binary:
//! the Rust accessors (`nros_shared_state.rs`, consumed as a module) plus the C
//! side (`cross.c`, which includes `nros_shared_context.h` and calls the
//! Rust-exported C-ABI accessors). `NROS_BAKE_DIR` points at the `nros-system`
//! bake dir the test produced with `nros codegen-system`.

use std::{env, fs, path::PathBuf};

fn main() {
    println!("cargo:rerun-if-env-changed=NROS_BAKE_DIR");
    println!("cargo:rerun-if-changed=src/cross.c");
    let bake = PathBuf::from(
        env::var("NROS_BAKE_DIR").expect("NROS_BAKE_DIR must be set by the test to the bake dir"),
    );

    // `#[path]` needs a literal, so copy the generated module next to main.rs
    // under a fixed name. Consuming it as a true `mod` (not `include!`) keeps
    // its inner `#![allow(dead_code)]` valid and the artifact verbatim.
    let generated = bake.join("nros_shared_state.rs");
    println!("cargo:rerun-if-changed={}", generated.display());
    let dst = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap()).join("src/generated.rs");
    fs::copy(&generated, &dst).expect("copy generated shared-state module into src/");

    cc::Build::new()
        .file("src/cross.c")
        .include(&bake)
        .compile("xlang_cross");
}
