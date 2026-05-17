//! Phase 152.2.B.3 (Option C) — generic ThreadX kernel +
//! `nros-platform-threadx` C-port build.
//!
//! Compiles two static archives every ThreadX overlay links
//! transitively:
//!
//! | Archive                       | Contents                            |
//! |-------------------------------|-------------------------------------|
//! | `libthreadx_kernel.a`         | kernel `common/src/*.c` + port C    |
//! | `libnros_platform_threadx.a`  | C port for `nros_platform_*` ABI    |
//!
//! NetX-Duo / NSOS, port `.S` assembly, board-specific drivers,
//! and the `tx_application_define` glue stay per-overlay
//! because the NetX variant choice (NSOS shim vs full
//! NetX-Duo + virtio-net) and arch ASM diverge too much for a
//! shared compile.
//!
//! Required env vars (read from the user's environment; the
//! repo's `.envrc` sets defaults; per-target justfile recipes
//! override for non-Linux ports):
//!
//! | Var                       | Purpose                                          |
//! |---------------------------|--------------------------------------------------|
//! | `THREADX_DIR`             | ThreadX kernel source root                       |
//! | `THREADX_PORT`            | Portable layer (e.g. `linux/gnu`, `risc-v64/gnu`)|
//! | `THREADX_CONFIG_DIR`      | Overlay dir with `tx_user.h`                     |
//! | `THREADX_EXTRA_INCLUDES`  | Colon-sep extra kernel includes (optional)       |
//! | `THREADX_CFLAGS`          | Extra cflags (via `apply_threadx_cflags`)        |
//! | `NETX_DIR`                | NetX-Duo source root (required for platform-threadx)|
//! | `NETX_CONFIG_DIR`         | Overlay dir with `nx_user.h`                     |
//! | `NETX_EXTRA_INCLUDES`     | Colon-sep extra NetX includes (optional)         |
//!
//! Bare `cargo check -p nros-board-threadx` (no env vars) is a
//! no-op + warning so the crate's surface compiles standalone.

use std::env;
use std::path::PathBuf;

fn main() {
    let Some(threadx_dir) = env::var_os("THREADX_DIR").map(PathBuf::from) else {
        println!(
            "cargo:warning=nros-board-threadx: THREADX_DIR not set; \
             skipping kernel + platform-threadx compile. Set it in \
             your overlay/example's `.cargo/config.toml [env]` or via direnv."
        );
        println!("cargo:rerun-if-env-changed=THREADX_DIR");
        return;
    };

    let port_subpath = env::var("THREADX_PORT").unwrap_or_else(|_| "linux/gnu".to_string());
    let config_dir = env::var_os("THREADX_CONFIG_DIR")
        .map(PathBuf::from)
        .expect("nros-board-threadx: THREADX_CONFIG_DIR must be set when THREADX_DIR is");
    let extra_kernel_includes: Vec<PathBuf> = env::var("THREADX_EXTRA_INCLUDES")
        .ok()
        .map(|v| v.split(':').filter(|s| !s.is_empty()).map(PathBuf::from).collect())
        .unwrap_or_default();

    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let workspace_root = manifest_dir
        .parent()
        .and_then(|p| p.parent())
        .and_then(|p| p.parent())
        .expect("Could not resolve workspace root");

    let threadx_port_dir = threadx_dir.join("ports").join(&port_subpath);
    assert!(
        threadx_port_dir.join("inc").exists(),
        "ThreadX port `{}` not found at {}",
        port_subpath,
        threadx_port_dir.display()
    );

    // ---- Build ThreadX kernel (C only — port `.S` stays per-overlay) ----
    let mut kernel = cc::Build::new();
    configure(&mut kernel);
    kernel
        .include(&config_dir)
        .include(threadx_dir.join("common/inc"))
        .include(threadx_port_dir.join("inc"));
    for p in &extra_kernel_includes {
        kernel.include(p);
    }
    nros_board_common::threadx_sources::add_threadx_kernel_sources(&mut kernel, &threadx_dir);
    nros_board_common::threadx_sources::add_threadx_port_sources(
        &mut kernel,
        &threadx_dir,
        &port_subpath,
    );
    kernel.compile("threadx_kernel");

    // ---- Build nros-platform-threadx C port ----
    // Needs threadx + netx includes. NETX_DIR is required because
    // `net.c` includes `nxd_bsd.h`. Bare-metal overlays without
    // NetX-Duo per-port dir simply leave `NETX_EXTRA_INCLUDES` empty.
    let netx_dir = env::var_os("NETX_DIR")
        .map(PathBuf::from)
        .expect("nros-board-threadx: NETX_DIR must be set (platform-threadx uses nx_bsd_*)");
    let extra_netx_includes: Vec<PathBuf> = env::var("NETX_EXTRA_INCLUDES")
        .ok()
        .map(|v| v.split(':').filter(|s| !s.is_empty()).map(PathBuf::from).collect())
        .unwrap_or_default();

    let mut platform = cc::Build::new();
    configure(&mut platform);
    platform
        .include(&config_dir)
        .include(threadx_dir.join("common/inc"))
        .include(threadx_port_dir.join("inc"))
        .include(netx_dir.join("common/inc"))
        .include(netx_dir.join("addons/BSD"));
    // Auto-include the matching NetX port inc dir if it exists
    // (Linux uses `ports/linux/gnu/inc` for `nx_port.h`; bare-
    // metal RISC-V has no NetX port dir and uses its overlay's
    // `config_dir` instead).
    let netx_port_inc = netx_dir.join("ports").join(&port_subpath).join("inc");
    if netx_port_inc.exists() {
        platform.include(&netx_port_inc);
    }
    for p in &extra_kernel_includes {
        platform.include(p);
    }
    for p in &extra_netx_includes {
        platform.include(p);
    }
    nros_board_common::threadx_sources::add_nros_platform_threadx_build(
        &mut platform,
        workspace_root,
    );
    platform.compile("nros_platform_threadx");

    // Link order (reverse dependency): platform → kernel.
    println!("cargo:rustc-link-lib=static=nros_platform_threadx");
    println!("cargo:rustc-link-lib=static=threadx_kernel");

    // Rerun triggers
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=THREADX_DIR");
    println!("cargo:rerun-if-env-changed=THREADX_PORT");
    println!("cargo:rerun-if-env-changed=THREADX_CONFIG_DIR");
    println!("cargo:rerun-if-env-changed=THREADX_EXTRA_INCLUDES");
    println!("cargo:rerun-if-env-changed=NETX_DIR");
    println!("cargo:rerun-if-env-changed=NETX_EXTRA_INCLUDES");
}

fn configure(build: &mut cc::Build) {
    build
        .opt_level(2)
        .flag("-ffunction-sections")
        .flag("-fdata-sections")
        .flag("-Wno-unused-parameter")
        .flag("-Wno-sign-compare")
        .define("TX_INCLUDE_USER_DEFINE_FILE", None)
        .define("NX_INCLUDE_USER_DEFINE_FILE", None)
        .warnings(false);
    // RISC-V cross-compile env. Detect by target triple, NOT by
    // THREADX_PORT, so a host-tooled `cargo check` doesn't
    // accidentally pick up the cross compiler.
    if env::var("TARGET").map(|t| t.starts_with("riscv64")).unwrap_or(false) {
        build
            .compiler("riscv64-unknown-elf-gcc")
            .archiver("riscv64-unknown-elf-ar")
            .flag("-march=rv64gc")
            .flag("-mabi=lp64d")
            .flag("-mcmodel=medany")
            .flag("-fno-builtin");
        if let Some(sysroot) = get_picolibc_sysroot() {
            build.include(sysroot.join("include"));
        }
    }
    // THREADX_CFLAGS hook for further per-overlay extension.
    nros_board_common::threadx_sources::apply_threadx_cflags(build);
}

/// Probe `riscv64-unknown-elf-gcc -print-sysroot` (under
/// picolibc.specs) for the picolibc sysroot. Returns `None` if
/// gcc isn't installed; caller proceeds without picolibc and
/// the kernel compile will fail with a clearer "string.h not
/// found" error.
fn get_picolibc_sysroot() -> Option<PathBuf> {
    use std::process::Command;
    let output = Command::new("riscv64-unknown-elf-gcc")
        .args([
            "-march=rv64gc",
            "-mabi=lp64d",
            "--specs=picolibc.specs",
            "-print-sysroot",
        ])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let s = String::from_utf8(output.stdout).ok()?;
    let trimmed = s.trim();
    if trimmed.is_empty() {
        // Fallback: hard-coded distro path (matches RISC-V overlay's helper).
        let p = PathBuf::from("/usr/lib/picolibc/riscv64-unknown-elf");
        if p.exists() { Some(p) } else { None }
    } else {
        Some(PathBuf::from(trimmed))
    }
}
