//! Build script for nros-stm32f4
//!
//! Copies the stm32f4.x linker script to the output directory as memory.x
//! so that cortex-m-rt can find it during linking.

use std::env;
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;

fn main() {
    let out = &PathBuf::from(env::var_os("OUT_DIR").unwrap());

    File::create(out.join("memory.x"))
        .unwrap()
        .write_all(include_bytes!("stm32f4.x"))
        .unwrap();

    println!("cargo:rustc-link-search={}", out.display());

    println!("cargo:rerun-if-changed=stm32f4.x");
    println!("cargo:rerun-if-changed=build.rs");
}
