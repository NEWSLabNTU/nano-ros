//! Build script for nano-ros-c
//!
//! The C headers in include/nano_ros/ are manually maintained.
//! No code generation is needed at build time.

fn main() {
    // Re-run if source files change (for library rebuild)
    println!("cargo:rerun-if-changed=src/");
}
