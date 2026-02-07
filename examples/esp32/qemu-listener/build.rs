//! Build script for esp32-qemu-listener
//!
//! Links the pre-built zenoh-pico library (build with scripts/esp32/build-zenoh-pico.sh)

use std::env;
use std::path::PathBuf;

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    // examples/esp32/qemu-listener -> examples/esp32 -> examples -> repo_root
    let repo_root = manifest_dir
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap();

    // Link pre-built zenoh-pico library for ESP32-C3 (RISC-V)
    let zenoh_pico_lib = repo_root.join("build/esp32-zenoh-pico");
    println!(
        "cargo:rustc-link-search=native={}",
        zenoh_pico_lib.display()
    );
    println!("cargo:rustc-link-lib=static=zenohpico");
    println!(
        "cargo:rerun-if-changed={}",
        zenoh_pico_lib.join("libzenohpico.a").display()
    );
    println!("cargo:rerun-if-changed=build.rs");
}
