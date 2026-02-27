//! Build script for nros-threadx-linux
//!
//! Compiles the ThreadX kernel (Linux simulation port), NetX Duo stack,
//! the Linux TAP network driver, and board-specific C glue into static
//! libraries linked into the final binary.
//!
//! Environment variables (auto-set by justfile recipes):
//!   THREADX_DIR          — ThreadX kernel source root (default: external/threadx)
//!   NETX_DIR             — NetX Duo source root (default: external/netxduo)
//!   THREADX_SAMPLES_DIR  — ThreadX learn samples (default: external/threadx-learn-samples)

use std::env;
use std::path::{Path, PathBuf};

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let config_dir = manifest_dir.join("config");

    // Resolve workspace root (three levels up from packages/boards/nros-threadx-linux/)
    let workspace_root = manifest_dir
        .parent()
        .and_then(|p| p.parent())
        .and_then(|p| p.parent())
        .expect("Could not resolve workspace root");

    let threadx_dir = env_path_or("THREADX_DIR", workspace_root.join("external/threadx"));
    let netx_dir = env_path_or("NETX_DIR", workspace_root.join("external/netxduo"));
    let samples_dir = env_path_or(
        "THREADX_SAMPLES_DIR",
        workspace_root.join("external/threadx-learn-samples"),
    );

    // Validate directories
    assert!(
        threadx_dir.join("common/inc").exists(),
        "ThreadX common/inc/ not found at {} — run `just setup-threadx`",
        threadx_dir.display()
    );
    assert!(
        netx_dir.join("common/inc").exists(),
        "NetX Duo common/inc/ not found at {} — run `just setup-threadx`",
        netx_dir.display()
    );

    let threadx_port_dir = threadx_dir.join("ports/linux/gnu");
    assert!(
        threadx_port_dir.join("src").exists(),
        "ThreadX Linux port not found at {}",
        threadx_port_dir.display()
    );

    // ---- Build ThreadX kernel ----
    let mut threadx = cc::Build::new();
    configure_linux(&mut threadx);
    add_threadx_includes(&mut threadx, &threadx_dir, &threadx_port_dir, &config_dir);

    // All kernel source files
    for entry in std::fs::read_dir(threadx_dir.join("common/src")).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_some_and(|e| e == "c") {
            threadx.file(&path);
        }
    }

    // Linux port files
    for entry in std::fs::read_dir(threadx_port_dir.join("src")).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_some_and(|e| e == "c") {
            threadx.file(&path);
        }
    }

    threadx.compile("threadx");

    // ---- Build NetX Duo ----
    let mut netxduo = cc::Build::new();
    configure_linux(&mut netxduo);
    add_threadx_includes(&mut netxduo, &threadx_dir, &threadx_port_dir, &config_dir);
    add_netx_includes(&mut netxduo, &netx_dir, &config_dir);

    // All NetX Duo common sources
    for entry in std::fs::read_dir(netx_dir.join("common/src")).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_some_and(|e| e == "c") {
            netxduo.file(&path);
        }
    }

    // BSD socket addon
    netxduo.file(netx_dir.join("addons/BSD/nxd_bsd.c"));

    netxduo.compile("netxduo");

    // ---- Build Linux network driver ----
    let mut driver = cc::Build::new();
    configure_linux(&mut driver);
    add_threadx_includes(&mut driver, &threadx_dir, &threadx_port_dir, &config_dir);
    add_netx_includes(&mut driver, &netx_dir, &config_dir);

    let driver_src = samples_dir.join("courses/netxduo/Driver/nx_linux_network_driver.c");
    assert!(
        driver_src.exists(),
        "Linux network driver not found at {}",
        driver_src.display()
    );
    driver.file(&driver_src);

    driver.compile("netxdriver");

    // ---- Build C glue (app_define.c) ----
    let mut glue = cc::Build::new();
    configure_linux(&mut glue);
    add_threadx_includes(&mut glue, &threadx_dir, &threadx_port_dir, &config_dir);
    add_netx_includes(&mut glue, &netx_dir, &config_dir);
    glue.file(manifest_dir.join("c/app_define.c"));

    glue.compile("glue");

    // ---- Link order (reverse dependency) ----
    println!("cargo:rustc-link-lib=static=glue");
    println!("cargo:rustc-link-lib=static=netxdriver");
    println!("cargo:rustc-link-lib=static=netxduo");
    println!("cargo:rustc-link-lib=static=threadx");
    println!("cargo:rustc-link-lib=pthread");

    // ---- Rerun triggers ----
    println!("cargo:rerun-if-changed=c/app_define.c");
    println!("cargo:rerun-if-changed=config/tx_user.h");
    println!("cargo:rerun-if-changed=config/nx_user.h");
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=THREADX_DIR");
    println!("cargo:rerun-if-env-changed=NETX_DIR");
    println!("cargo:rerun-if-env-changed=THREADX_SAMPLES_DIR");
}

fn env_path_or(name: &str, default: PathBuf) -> PathBuf {
    env::var(name).map(PathBuf::from).unwrap_or(default)
}

fn configure_linux(build: &mut cc::Build) {
    build
        .opt_level(2)
        .flag("-ffunction-sections")
        .flag("-fdata-sections")
        .flag("-Wno-unused-parameter")
        .flag("-Wno-sign-compare")
        .define("TX_INCLUDE_USER_DEFINE_FILE", None)
        .define("NX_INCLUDE_USER_DEFINE_FILE", None);
    // Suppress common warnings in third-party code
    build.warnings(false);
}

fn add_threadx_includes(
    build: &mut cc::Build,
    threadx_dir: &Path,
    port_dir: &Path,
    config_dir: &Path,
) {
    build
        .include(config_dir)
        .include(threadx_dir.join("common/inc"))
        .include(port_dir.join("inc"));
}

fn add_netx_includes(build: &mut cc::Build, netx_dir: &Path, config_dir: &Path) {
    build
        .include(config_dir)
        .include(netx_dir.join("common/inc"))
        .include(netx_dir.join("ports/linux/gnu/inc"))
        .include(netx_dir.join("addons/BSD"));
}
