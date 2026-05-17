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
//!   NETX_DIR — NetX Duo source root for BSD compatibility headers
//!              (default: third-party/threadx/netxduo)
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
    let netx_dir = env_path_or("NETX_DIR", workspace_root.join("third-party/threadx/netxduo"));
    assert!(
        netx_dir.join("common/inc").exists(),
        "NetX Duo common/inc/ not found at {} — run `just threadx_linux setup`",
        netx_dir.display()
    );

    // ---- Build ThreadX kernel ----
    // Phase 152.2.B — kernel + port source enumeration moved to
    // `nros_board_common::threadx_sources` so both ThreadX
    // overlays (Linux sim + RISC-V QEMU) + the future generic
    // crate share one canonical list. ThreadX-kernel submodule
    // bumps that add new files pick up automatically here.
    let mut threadx = cc::Build::new();
    configure_linux(&mut threadx);
    add_threadx_includes(&mut threadx, &threadx_dir, &threadx_port_dir, &config_dir);
    nros_board_common::threadx_sources::add_threadx_kernel_sources(&mut threadx, &threadx_dir);
    nros_board_common::threadx_sources::add_threadx_port_sources(
        &mut threadx,
        &threadx_dir,
        "linux/gnu",
    );
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

    // ---- Phase 121.3 — Build nros-platform-threadx C port ----
    // The native C port provides the canonical `nros_platform_*`
    // symbols against the ThreadX kernel + nsos-netx BSD shim. Built
    // in-tree by the board because the includes (`tx_api.h`,
    // `nx_bsd_*.h`) are already configured here.
    let nros_platform_threadx_dir =
        workspace_root.join("packages/core/nros-platform-threadx/src");
    let nros_platform_cffi_include =
        workspace_root.join("packages/core/nros-platform-cffi/include");
    let mut platform = cc::Build::new();
    configure_linux(&mut platform);
    add_threadx_includes(&mut platform, &threadx_dir, &threadx_port_dir, &config_dir);
    platform.include(netx_dir.join("common/inc"));
    platform.include(netx_dir.join("ports/linux/gnu/inc"));
    platform.include(netx_dir.join("addons/BSD"));
    platform.include(&nros_platform_cffi_include);
    platform.file(nros_platform_threadx_dir.join("platform.c"));
    platform.file(nros_platform_threadx_dir.join("net.c"));
    platform.file(nros_platform_threadx_dir.join("timer.c"));
    platform.compile("nros_platform_threadx");
    println!("cargo:rerun-if-changed={}", nros_platform_threadx_dir.display());

    // ---- Build C glue (app_define.c) ----
    let mut glue = cc::Build::new();
    configure_linux(&mut glue);
    add_threadx_includes(&mut glue, &threadx_dir, &threadx_port_dir, &config_dir);
    glue.file(manifest_dir.join("c/app_define.c"));

    glue.compile("glue");

    // ---- Link order (reverse dependency) ----
    println!("cargo:rustc-link-lib=static=nros_platform_threadx");
    println!("cargo:rustc-link-lib=static=glue");
    println!("cargo:rustc-link-lib=static=nsos_netx");
    println!("cargo:rustc-link-lib=static=threadx");
    println!("cargo:rustc-link-lib=pthread");

    // ---- Rerun triggers ----
    println!("cargo:rerun-if-changed=c/app_define.c");
    println!("cargo:rerun-if-changed=config/tx_user.h");
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=THREADX_DIR");
    println!("cargo:rerun-if-env-changed=NETX_DIR");
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
