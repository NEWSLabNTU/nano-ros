//! Build script for qemu-bsp-listener
//!
//! This script:
//! 1. Sets up the memory layout linker script
//! 2. Links the pre-built zenoh-pico library (build with scripts/qemu/build-zenoh-pico.sh)

use std::env;
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;

fn main() {
    let out = &PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    // examples/qemu/bsp-listener -> examples/qemu -> examples -> repo_root
    let repo_root = manifest_dir
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap();

    // Use shared linker script from qemu-rs-common
    File::create(out.join("memory.x"))
        .unwrap()
        .write_all(include_bytes!(
            "../../platform-integration/qemu-smoltcp-bridge/mps2-an385.x"
        ))
        .unwrap();

    println!("cargo:rustc-link-search={}", out.display());
    println!("cargo:rerun-if-changed=../../platform-integration/qemu-smoltcp-bridge/mps2-an385.x");
    println!("cargo:rerun-if-changed=build.rs");

    // Link pre-built zenoh-pico library
    let zenoh_pico_lib = repo_root.join("build/qemu-zenoh-pico");
    println!(
        "cargo:rustc-link-search=native={}",
        zenoh_pico_lib.display()
    );
    println!("cargo:rustc-link-lib=static=zenohpico");
    println!(
        "cargo:rerun-if-changed={}",
        zenoh_pico_lib.join("libzenohpico.a").display()
    );
}
