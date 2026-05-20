//! Build script for nros-board-mps2-an385
//!
//! Copies the mps2-an385.x linker script to the output directory as memory.x
//! so that cortex-m-rt can find it during linking.

use std::{env, fs::File, io::Write, path::PathBuf};

fn main() {
    let out = &PathBuf::from(env::var_os("OUT_DIR").unwrap());

    File::create(out.join("memory.x"))
        .unwrap()
        .write_all(include_bytes!("mps2-an385.x"))
        .unwrap();

    println!("cargo:rustc-link-search={}", out.display());

    println!("cargo:rerun-if-changed=mps2-an385.x");
    println!("cargo:rerun-if-changed=build.rs");
}
