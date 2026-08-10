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

use std::{env, path::PathBuf};

// issue 0491 — the ONE path-input helper, reached through the build-helper
// crate this script already deps.
use nros_board_common::nros_build_paths;

fn main() {
    let Some(threadx_dir) = nros_build_paths::env_path("THREADX_DIR") else {
        println!(
            "cargo:warning=nros-board-threadx: THREADX_DIR not set; \
             skipping kernel + platform-threadx compile. Set it in \
             your overlay/example's `.cargo/config.toml [env]` or via direnv."
        );
        return;
    };

    let port_subpath = env::var("THREADX_PORT").unwrap_or_else(|_| "linux/gnu".to_string());

    // issue 0288 — skip the C build when the selected PORT does not match the
    // target we are building for.
    //
    // The gate below already picks the cross COMPILER by triple, but the
    // sources are compiled either way, and the RISC-V port's C carries RISC-V
    // inline asm. Host-building it hands `csrrci`/`csrrs` to the host
    // assembler:
    //
    //     tx_trace_object_register.c:292: Error: no such instruction:
    //     `csrrci %rax,mstatus,0x08'
    //
    // which makes every package depping this board un-host-buildable — and the
    // source-metadata probe host-compiles exactly such packages to record their
    // callback slots (issue 0288).
    //
    // The triple ALONE cannot decide this: `threadx-linux` is a real ThreadX
    // board whose target IS `x86_64-unknown-linux-gnu`, so "skip on x86_64"
    // would break it. The honest discriminator is the PORT: a cross port
    // selected while building for a host triple means we are host-tooling, not
    // building firmware.
    {
        let target = env::var("TARGET").unwrap_or_default();
        let cross_port_arch = if port_subpath.starts_with("risc-v64") {
            Some("riscv64")
        } else {
            // `linux/gnu` is host-native by definition; any future cross port
            // adds an arm here rather than inheriting a wrong default.
            None
        };
        if let Some(arch) = cross_port_arch
            && !target.starts_with(arch)
        {
            println!(
                "cargo:warning=nros-board-threadx: THREADX_PORT={port_subpath} targets {arch} \
                 but TARGET={target}; skipping the ThreadX C build. The crate still compiles \
                 as a Rust shell so host tooling can build packages that dep it (issue 0288)."
            );
            return;
        }
    }
    let config_dir = nros_build_paths::env_path("THREADX_CONFIG_DIR")
        .expect("nros-board-threadx: THREADX_CONFIG_DIR must be set when THREADX_DIR is");
    let extra_kernel_includes: Vec<PathBuf> = env::var("THREADX_EXTRA_INCLUDES")
        .ok()
        .map(|v| {
            v.split(':')
                .filter(|s| !s.is_empty())
                .map(|s| nros_build_paths::canonical(std::path::Path::new(s)))
                .collect()
        })
        .unwrap_or_default();

    let threadx_port_dir = threadx_dir.join("ports").join(&port_subpath);
    assert!(
        threadx_port_dir.join("inc").exists(),
        "ThreadX port `{}` not found at {}",
        port_subpath,
        threadx_port_dir.display()
    );

    // phase-337 W4.a — an in-tree ARCH-PORT OVERRIDE for this `THREADX_PORT`,
    // if one exists. `risc-v64/gnu` has one
    // (`nros-board-threadx-port-riscv64`) because upstream types `ULONG` as 8
    // bytes there and NetX Duo's packet code needs 4; `linux/gnu` has none
    // because upstream already types it as 4.
    //
    // Its `inc/` MUST precede the upstream port's `inc/`: the forked
    // `tx_port.h` shadows upstream's, and losing that precedence compiles
    // cleanly and corrupts packets at runtime. This is why the override is
    // NOT folded into `THREADX_EXTRA_INCLUDES`, which is appended after.
    let port_override_inc = port_override_inc_dir(&port_subpath);

    // ---- Build ThreadX kernel (C only — port `.S` stays per-overlay) ----
    let mut kernel = cc::Build::new();
    configure(&mut kernel);
    kernel.include(&config_dir);
    if let Some(inc) = &port_override_inc {
        kernel.include(inc);
    }
    kernel
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
    let netx_dir = nros_build_paths::env_path("NETX_DIR")
        .expect("nros-board-threadx: NETX_DIR must be set (platform-threadx uses nx_bsd_*)");
    let extra_netx_includes: Vec<PathBuf> = env::var("NETX_EXTRA_INCLUDES")
        .ok()
        .map(|v| {
            v.split(':')
                .filter(|s| !s.is_empty())
                .map(|s| nros_build_paths::canonical(std::path::Path::new(s)))
                .collect()
        })
        .unwrap_or_default();

    let mut platform = cc::Build::new();
    configure(&mut platform);
    platform.include(&config_dir);
    if let Some(inc) = &port_override_inc {
        platform.include(inc);
    }
    platform
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
    nros_board_common::threadx_sources::add_nros_platform_threadx_build(&mut platform);
    platform.compile("nros_platform_threadx");

    // Link order (reverse dependency): platform → kernel.
    println!("cargo:rustc-link-lib=static=nros_platform_threadx");
    println!("cargo:rustc-link-lib=static=threadx_kernel");

    // Rerun triggers
    println!("cargo:rerun-if-changed=build.rs");
    // The PORT is a NAME (`risc-v64/gnu`) — a different value really is a
    // different compile, so it stays fingerprinted.
    println!("cargo:rerun-if-env-changed=THREADX_PORT");
    // issue 0491 — every other variable here names a DIRECTORY, and a
    // directory has one spelling per example leaf (`relative = true` in the
    // leaf's `.cargo/config.toml`, resolved against THAT leaf), another from
    // `just`, and none at all from a bare build. Cargo compares the value as
    // text, so fingerprinting it made the six ThreadX rows sharing one
    // `--target-dir` invalidate each other forever. The overlay `config/` dirs
    // (`tx_user.h`, `nx_user.h`) are watched by CONTENT instead; the two
    // vendored kernels are read-only sources whose compiled files are declared
    // by `threadx_sources`.
    nros_build_paths::watch_path(&config_dir);
    for p in extra_kernel_includes
        .iter()
        .chain(extra_netx_includes.iter())
    {
        nros_build_paths::watch_path(p);
    }
}

/// phase-337 W4.a — the in-tree arch-port override for a `THREADX_PORT`
/// value, if there is one.
///
/// ONE table, keyed by the port subpath, so the family driver never grows a
/// second spelling of a port path. The path itself comes from the port
/// crate's own manifest dir, not from a relative walk.
fn port_override_inc_dir(port_subpath: &str) -> Option<PathBuf> {
    let dir = match port_subpath {
        "risc-v64/gnu" => nros_board_threadx_port_riscv64::inc_dir(),
        // `linux/gnu` and every other upstream port is used as shipped.
        _ => return None,
    };
    println!("cargo:rerun-if-changed={}", dir.join("tx_port.h").display());
    Some(dir)
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
    // issue 0383 — implicit-function-declaration / int-conversion as errors
    // (`warnings(false)` only omits `-Wall`/`-Wextra`; cc-rs passes no `-w`).
    nros_cc_flags::strict_decls(build);
    // RISC-V cross-compile env. Detect by target triple, NOT by
    // THREADX_PORT, so a host-tooled `cargo check` doesn't
    // accidentally pick up the cross compiler.
    if env::var("TARGET")
        .map(|t| t.starts_with("riscv64"))
        .unwrap_or(false)
    {
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
