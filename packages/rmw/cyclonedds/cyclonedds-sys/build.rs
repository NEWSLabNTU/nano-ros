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
            "cyclonedds-sys: source dir {} has no CMakeLists.txt — source not \
             provisioned. Run `nros setup --source cyclonedds-src` (or \
             `git submodule update --init third-party/dds/cyclonedds`, or set \
             CYCLONEDDS_SOURCE_DIR). — #0390",
            src.display(),
        );
    }

    // issue 0400 — CMake caches `check_symbol_exists` results as INTERNAL
    // entries and a reconfigure never re-tests them, so a build dir configured
    // under one C library keeps its answers under another. The `cmake` crate
    // reuses `$OUT_DIR/build` across runs, and a fixture built with an explicit
    // `--target-dir` puts that OUT_DIR inside the CHECKOUT — shared between the
    // host and the ROS distrobox. glibc grew `strlcpy`/`strlcat` in 2.38: Arch
    // has them, Ubuntu 22.04 (2.35) does not, so the host's
    // `idlpp_have_strlcat=1` made idlc skip its own fallback and the box link
    // died with `undefined reference to \`strlcpy'` in vendored code that
    // builds fine on both. Same rule as `scripts/build/cmake-cache-guard.sh`,
    // applied where the `cmake` crate owns the dir.
    wipe_build_dir_on_compiler_change(&PathBuf::from(env::var("OUT_DIR").unwrap()).join("build"));

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
    // Cyclone's DDSRT pulls in pthread + dl + rt on a hosted glibc/Linux host.
    // Linux, not POSIX: `librt` and `libdl` are separate libraries only on
    // glibc — macOS folds both into libSystem and ships no `-lrt`, so this
    // ungated link line does not resolve there.
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

/// Drop a CMake build dir whose cached configure results describe a DIFFERENT
/// compiler (issue 0400). The compiler version separates host from container
/// and also catches a host toolchain upgrade, where stale capability probes are
/// the same hazard. No-op when the versions match or either is unknown.
fn wipe_build_dir_on_compiler_change(build_dir: &std::path::Path) {
    if !build_dir.join("CMakeCache.txt").is_file() {
        return;
    }
    let Some(cached) = cmake_cached_cc_version(build_dir) else {
        return;
    };
    let cc = env::var("CC").unwrap_or_else(|_| "cc".to_string());
    let Some(now) = std::process::Command::new(&cc)
        .arg("-dumpfullversion")
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|v| !v.is_empty())
    else {
        return;
    };
    if cached != now {
        println!(
            "cargo:warning=cyclonedds-sys: CMake dir was configured by cc {cached}, now {now} — \
             wiping (cached check_symbol_exists results describe the other environment, issue 0400)"
        );
        let _ = std::fs::remove_dir_all(build_dir);
    }
}

/// `CMAKE_C_COMPILER_VERSION` out of `CMakeFiles/<ver>/CMakeCCompiler.cmake`.
fn cmake_cached_cc_version(build_dir: &std::path::Path) -> Option<String> {
    let entries = std::fs::read_dir(build_dir.join("CMakeFiles")).ok()?;
    for entry in entries.flatten() {
        let info = entry.path().join("CMakeCCompiler.cmake");
        let Ok(text) = std::fs::read_to_string(&info) else {
            continue;
        };
        for line in text.lines() {
            if let Some(rest) = line.trim().strip_prefix("set(CMAKE_C_COMPILER_VERSION ") {
                return Some(rest.trim_matches(|c| c == '"' || c == ')').to_string());
            }
        }
    }
    None
}
