//! Build script for nano-ros-bsp-qemu
//!
//! 1. Copies the mps2-an385.x linker script to the output directory
//! 2. Links the pre-built zenoh-pico ARM library via ZENOH_PICO_LIB_DIR env var

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

    // Link pre-built zenoh-pico library
    if let Ok(lib_dir) = env::var("ZENOH_PICO_LIB_DIR") {
        println!("cargo:rustc-link-search=native={lib_dir}");
        println!("cargo:rustc-link-lib=static=zenohpico");
        let lib_path = PathBuf::from(&lib_dir).join("libzenohpico.a");
        println!("cargo:rerun-if-changed={}", lib_path.display());
    }

    println!("cargo:rerun-if-changed=mps2-an385.x");
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=ZENOH_PICO_LIB_DIR");
}
