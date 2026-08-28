//! Phase 88.15.a — emit `memory.x` into OUT_DIR so cortex-m-rt's
//! `link.x` finds it without depending on the board crate's build
//! script.

use std::{env, fs::File, io::Write, path::PathBuf};

fn main() {
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    File::create(out.join("memory.x"))
        .unwrap()
        .write_all(include_bytes!("memory.x"))
        .unwrap();
    println!("cargo:rustc-link-search={}", out.display());
    println!("cargo:rerun-if-changed=memory.x");
    println!("cargo:rerun-if-changed=build.rs");
}
