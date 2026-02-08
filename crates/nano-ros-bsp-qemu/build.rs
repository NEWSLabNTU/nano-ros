//! Build script for nano-ros-bsp-qemu
//!
//! Copies the mps2-an385.x linker script to the output directory.

use std::env;
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;

fn main() {
    // Put the linker script somewhere the linker can find it
    let out = &PathBuf::from(env::var_os("OUT_DIR").unwrap());

    File::create(out.join("memory.x"))
        .unwrap()
        .write_all(include_bytes!("mps2-an385.x"))
        .unwrap();

    println!("cargo:rustc-link-search={}", out.display());

    println!("cargo:rerun-if-changed=mps2-an385.x");
    println!("cargo:rerun-if-changed=build.rs");
}
