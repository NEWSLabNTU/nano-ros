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
//!   - Phase 212.M-F.10.3 — `NROS_APP_CONFIG` symbol emission. The
//!     shipped `<nros/app_config.h>` non-Zephyr branch declares the
//!     symbol `extern`; we emit the matching `const` definition into
//!     this board's staticlib so any TU (notably the board's
//!     `startup.c`) that pulls the header resolves it at link time.
//!     Values transcribed from the board's pre-M.10 default
//!     `nros.toml` (matches `src/config.rs`'s `Config::default()` for
//!     IP/MAC/gateway/interface/domain_id; locator stays at the
//!     retired toml's `tcp/127.0.0.1:7555`).
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
    fs,
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

    // Phase 212.M-F.10.3 — emit `const nros_app_config_t NROS_APP_CONFIG`
    // into a board-side TU. Must happen before the `glue` cc::Build below
    // so the emitted .c rides along in the same staticlib (single archive
    // search per link pass). See `emit_app_config_def` for the value
    // contract.
    let nros_c_include = workspace_root.join("packages/core/nros-c/include");
    assert!(
        nros_c_include.join("nros/zephyr/app_config.h").exists(),
        "nros-c shipped header not found at {} — expected the M-F.10.1 \
         `extern const` declaration. Did the workspace_root walk-up break?",
        nros_c_include.display()
    );
    let app_config_def_c = emit_app_config_def();

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
    // Phase 212.M-F.10.3 — pull the emitted NROS_APP_CONFIG TU into the
    // same `glue` staticlib. The emitted .c includes the shipped header
    // via `<nros/zephyr/app_config.h>` to get the `nros_app_config_t`
    // type definition (the include-root path resolves the same struct
    // shipped at the canonical location; the Zephyr Kconfig branch is
    // skipped because `__ZEPHYR__` is unset on a host build).
    glue.include(&nros_c_include);
    glue.file(&app_config_def_c);
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

/// Phase 212.M-F.10.3 — Path C: emit the `NROS_APP_CONFIG` definition
/// matching the `extern` declaration shipped in
/// `<nros/zephyr/app_config.h>`'s non-Zephyr branch (the canonical
/// `nros_app_config_t` storage path on threadx-linux).
///
/// Field values come from the board's pre-M.10 default `nros.toml`
/// (`tcp/127.0.0.1:7555` locator, `02:00:00:00:00:00` MAC,
/// `192.0.3.10/24` IP, `192.0.3.1` gateway, `veth-tx0` interface,
/// domain `0`); they cross-check against `src/config.rs`'s
/// `Config::default()` for everything except the locator (the Rust
/// default points at the bridge gateway `192.0.3.1:7447` — the
/// nros.toml-era examples ran zenohd on the loopback host instead, so
/// we transcribe `127.0.0.1:7555` to keep the prior C/C++ E2E
/// fixture's locator stable). Scheduling fields stay zero — the board
/// has its own ThreadX-thread plumbing and doesn't expose scheduler
/// knobs through `NROS_APP_CONFIG`.
///
/// Returns the absolute path of the emitted .c, ready to feed into a
/// `cc::Build`.
fn emit_app_config_def() -> PathBuf {
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let dst = out_dir.join("nros_app_config_def.c");

    let body = r#"/* Auto-generated by packages/boards/nros-board-threadx-linux/build.rs.
 * Phase 212.M-F.10.3 Path C — board-side NROS_APP_CONFIG symbol
 * definition (matches the `extern const` declaration in the shipped
 * <nros/app_config.h> non-Zephyr branch).
 *
 * Edits must follow `Config::default()` in src/config.rs + the
 * retired examples' nros.toml. Do NOT edit by hand — patch
 * build.rs::emit_app_config_def instead.
 */

#include <stdint.h>
/* Canonical path — wrapper at nros-c/include/nros/app_config.h
 * re-includes nros/zephyr/app_config.h for the struct type. */
#include <nros/app_config.h>

const nros_app_config_t NROS_APP_CONFIG = {
    .zenoh =
        {
            .locator = "tcp/127.0.0.1:7555",
            .domain_id = 0,
        },
    .network =
        {
            .ip = {192, 0, 3, 10},
            .mac = {0x02, 0x00, 0x00, 0x00, 0x00, 0x00},
            .gateway = {192, 0, 3, 1},
            .netmask = {255, 255, 255, 0},
            .prefix = 24,
        },
    .scheduling = {0, 0, 0, 0, 0, 0, 0, 0},
};
"#;

    fs::write(&dst, body).expect("failed to write nros_app_config_def.c into OUT_DIR");
    dst
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
