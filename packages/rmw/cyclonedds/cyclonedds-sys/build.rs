//! Build script for `cyclonedds-sys` (Phase 212.K.1).
//!
//! Drives `third-party/dds/cyclonedds` (pin: 0.10.5-14-g12b4af2c) through
//! the `cmake` build-script crate and emits the link metadata + the
//! `DEP_DDSC_*` hand-offs every downstream sys crate (notably
//! `nros-rmw-cyclonedds-sys`, K.2) needs.
//!
//! Flags mirror `just/cyclonedds.just::build-rmw` so the cmake project
//! self-built here matches what the in-tree CMake path produces.
//!
//! Override the Cyclone source location with `CYCLONEDDS_SOURCE_DIR=…`
//! (used by `nros-build-paths::env_or_repo_path`).

use std::{env, path::PathBuf};

fn main() {
    println!("cargo:rerun-if-changed=build.rs");

    // Source dir — default to the pinned submodule; user override wins.
    let src =
        nros_build_paths::env_or_repo_path("CYCLONEDDS_SOURCE_DIR", "third-party/dds/cyclonedds");
    println!("cargo:rerun-if-changed={}", src.display());
    if !src.join("CMakeLists.txt").is_file() {
        panic!(
            "cyclonedds-sys: source dir {} has no CMakeLists.txt — \
             submodule not initialised? Run `git submodule update --init \
             third-party/dds/cyclonedds` or set CYCLONEDDS_SOURCE_DIR.",
            src.display(),
        );
    }

    // Configure + build via the `cmake` crate.
    //
    // - ENABLE_LTO=OFF: rust-lld cannot link slim-LTO objects produced
    //   by Cyclone's default GCC LTO settings (cf. MEMORY: "ThreadX
    //   Cyclone LTO vs rust-lld" — same hazard on native).
    // - BUILD_SHARED_LIBS=OFF: static `libddsc.a` for clean link.
    // - BUILD_IDLC=ON: host `idlc` shipped alongside the lib.
    // - ENABLE_{SSL,SECURITY,SHM}=OFF + BUILD_{TESTING,DOCS,EXAMPLES}=OFF:
    //   trim build time (matches just/cyclonedds.just).
    let dst = cmake::Config::new(&src)
        .define("CMAKE_BUILD_TYPE", "Release")
        .define("ENABLE_LTO", "OFF")
        .define("BUILD_SHARED_LIBS", "OFF")
        .define("BUILD_IDLC", "ON")
        .define("ENABLE_SSL", "OFF")
        .define("ENABLE_SECURITY", "OFF")
        .define("ENABLE_SHM", "OFF")
        .define("BUILD_TESTING", "OFF")
        .define("BUILD_DOCS", "OFF")
        .define("BUILD_EXAMPLES", "OFF")
        .define("CMAKE_POSITION_INDEPENDENT_CODE", "ON")
        // The `cmake` crate's default install layout is
        // `<OUT_DIR>/{include,lib,bin}` — matches Cyclone's defaults.
        .build();

    let install_lib = dst.join("lib");
    let install_include = dst.join("include");
    let install_bin = dst.join("bin");

    // Sanity — fail loudly here rather than at downstream link time.
    let libddsc_a = install_lib.join("libddsc.a");
    if !libddsc_a.is_file() {
        panic!(
            "cyclonedds-sys: expected static libddsc at {} after build. \
             Cyclone may have ignored BUILD_SHARED_LIBS=OFF; check \
             {}/build.log for clues.",
            libddsc_a.display(),
            dst.display(),
        );
    }
    let idlc = install_bin.join("idlc");
    if !idlc.is_file() {
        panic!(
            "cyclonedds-sys: expected host idlc at {} after build. \
             Cyclone may have skipped BUILD_IDLC=ON.",
            idlc.display(),
        );
    }

    // Linker flags.
    println!("cargo:rustc-link-search=native={}", install_lib.display());
    println!("cargo:rustc-link-lib=static=ddsc");
    // Cyclone's DDSRT pulls in pthread + dl + rt on hosted POSIX.
    println!("cargo:rustc-link-lib=dylib=pthread");
    println!("cargo:rustc-link-lib=dylib=dl");
    println!("cargo:rustc-link-lib=dylib=rt");

    // DEP_DDSC_* metadata for downstream crates (via `links = "ddsc"`).
    println!("cargo:include={}", install_include.display());
    println!("cargo:idlc={}", idlc.display());
    println!("cargo:lib={}", install_lib.display());
    println!("cargo:root={}", dst.display());

    // Re-export the install root in the OUT_DIR for ad-hoc inspection.
    let out_dir = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR"));
    println!("cargo:rerun-if-changed={}", out_dir.display());
}
