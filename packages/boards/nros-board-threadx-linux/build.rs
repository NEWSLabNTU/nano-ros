//! Build script for nros-board-threadx-linux
//!
//! Compiles the ThreadX kernel (Linux simulation port), the nsos-netx
//! BSD socket shim (forwards `nx_bsd_*` to host POSIX), and board-specific
//! C glue into static libraries linked into the final binary.
//!
//! No NetX Duo TCP/IP stack is built — networking goes through the host
//! kernel via nsos-netx.
//!
//! Environment variables (auto-set by justfile recipes):
//!   THREADX_DIR  — ThreadX kernel source root (default: third-party/threadx/kernel)
//!   NSOS_NETX_DIR — nsos-netx shim source (default: packages/drivers/nsos-netx)

use std::env;
use std::path::{Path, PathBuf};

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let config_dir = manifest_dir.join("config");

    // Resolve workspace root (three levels up from packages/boards/nros-board-threadx-linux/)
    let workspace_root = manifest_dir
        .parent()
        .and_then(|p| p.parent())
        .and_then(|p| p.parent())
        .expect("Could not resolve workspace root");

    let threadx_dir = env_path_or("THREADX_DIR", workspace_root.join("third-party/threadx/kernel"));
    assert!(
        threadx_dir.join("common/inc").exists(),
        "ThreadX common/inc/ not found at {} — run `just setup-threadx`",
        threadx_dir.display()
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

    // ---- Build nsos-netx (NetX BSD compatibility shim over POSIX) ----
    let nsos_netx_dir = env_path_or(
        "NSOS_NETX_DIR",
        workspace_root.join("packages/drivers/nsos-netx"),
    );
    let nsos_src = nsos_netx_dir.join("src/nsos_netx.c");
    assert!(
        nsos_src.exists(),
        "nsos-netx not found at {}",
        nsos_src.display()
    );

    let mut nsos = cc::Build::new();
    configure_linux(&mut nsos);
    nsos.include(nsos_netx_dir.join("include"));
    nsos.file(&nsos_src);
    nsos.compile("nsos_netx");

    println!("cargo:rerun-if-changed={}", nsos_src.display());

    // ---- Build C glue (app_define.c) ----
    let mut glue = cc::Build::new();
    configure_linux(&mut glue);
    add_threadx_includes(&mut glue, &threadx_dir, &threadx_port_dir, &config_dir);
    glue.file(manifest_dir.join("c/app_define.c"));

    glue.compile("glue");

    // ---- Link order (reverse dependency) ----
    println!("cargo:rustc-link-lib=static=glue");
    println!("cargo:rustc-link-lib=static=nsos_netx");
    println!("cargo:rustc-link-lib=static=threadx");
    println!("cargo:rustc-link-lib=pthread");

    // ---- Rerun triggers ----
    println!("cargo:rerun-if-changed=c/app_define.c");
    println!("cargo:rerun-if-changed=config/tx_user.h");
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=THREADX_DIR");
    println!("cargo:rerun-if-env-changed=NSOS_NETX_DIR");
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
        .define("TX_INCLUDE_USER_DEFINE_FILE", None);
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
