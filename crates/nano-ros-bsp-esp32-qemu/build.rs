//! Build script for nano-ros-bsp-esp32-qemu
//!
//! Links the pre-built zenoh-pico RISC-V library via ZENOH_PICO_LIB_DIR env var.

use std::env;
use std::path::PathBuf;

fn main() {
    // Link pre-built zenoh-pico library
    if let Ok(lib_dir) = env::var("ZENOH_PICO_LIB_DIR") {
        println!("cargo:rustc-link-search=native={lib_dir}");
        println!("cargo:rustc-link-lib=static=zenohpico");
        let lib_path = PathBuf::from(&lib_dir).join("libzenohpico.a");
        println!("cargo:rerun-if-changed={}", lib_path.display());
    }

    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=ZENOH_PICO_LIB_DIR");
}
