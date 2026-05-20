//! Build script for nros-board-threadx-linux
//!
//! Phase 152.2.B.3 (Option C) — kernel + `nros-platform-threadx`
//! compile lifted into the generic `nros-board-threadx` crate's
//! own `build.rs`. This overlay's build script now only owns the
//! Linux-specific bits:
//!
//!   - NSOS-netx BSD shim (`libnsos_netx.a`)
//!   - Board-specific glue + shared `threadx_hooks.c`
//!     (`libglue.a`)
//!   - `pthread` link line
//!
//! Environment variables (auto-set by `.envrc` direnv defaults):
//!   `THREADX_DIR`        — ThreadX kernel source root
//!   `NETX_DIR`           — NetX-Duo source root (BSD shim headers)
//!   `NSOS_NETX_DIR`      — nsos-netx shim source
//!   `THREADX_CONFIG_DIR` — overlay's `config/` for `tx_user.h`
//!   `NETX_CONFIG_DIR`    — overlay's `config/` for `nx_user.h`
//!
//! `THREADX_PORT` defaults to `linux/gnu` in the generic crate's
//! `build.rs`, so this overlay does not need to override it.

use std::{
    env,
    path::{Path, PathBuf},
};

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let config_dir = manifest_dir.join("config");

    let workspace_root = manifest_dir
        .parent()
        .and_then(|p| p.parent())
        .and_then(|p| p.parent())
        .expect("Could not resolve workspace root");

    let threadx_dir = env_path_or(
        "THREADX_DIR",
        workspace_root.join("third-party/threadx/kernel"),
    );
    let threadx_port_dir = threadx_dir.join("ports/linux/gnu");
    assert!(
        threadx_port_dir.join("inc").exists(),
        "ThreadX Linux port not found at {}",
        threadx_port_dir.display()
    );
    let netx_dir = env_path_or(
        "NETX_DIR",
        workspace_root.join("third-party/threadx/netxduo"),
    );
    assert!(
        netx_dir.join("common/inc").exists(),
        "NetX Duo common/inc/ not found at {} — run `just threadx_linux setup`",
        netx_dir.display()
    );

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

    // ---- Build C glue (board-specific weak-hook impls + shared threadx_hooks) ----
    let mut glue = cc::Build::new();
    configure_linux(&mut glue);
    add_threadx_includes(&mut glue, &threadx_dir, &threadx_port_dir, &config_dir);
    nros_board_common::threadx_sources::add_threadx_hooks_source(&mut glue);
    glue.file(manifest_dir.join("c/board_threadx_linux.c"));
    glue.compile("glue");

    // ---- Link order (reverse dependency) ----
    // `libnros_platform_threadx.a` + `libthreadx_kernel.a` come from the
    // generic `nros-board-threadx` crate's build.rs (152.2.B.3 lift).
    println!("cargo:rustc-link-lib=static=glue");
    println!("cargo:rustc-link-lib=static=nsos_netx");
    println!("cargo:rustc-link-lib=pthread");

    // ---- Rerun triggers ----
    println!("cargo:rerun-if-changed=c/board_threadx_linux.c");
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
    build.warnings(false);
    nros_board_common::threadx_sources::apply_threadx_cflags(build);
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
