//! Build-script helper for the QEMU RISC-V64 ThreadX board.

use std::{
    env,
    io::Write,
    path::{Path, PathBuf},
    process::Command,
};

/// `port_dir` is the layer-2 arch-port override
/// (`nros_board_threadx_port_riscv64::port_dir()`): `inc/tx_port.h` plus the
/// five forked context-switch `.S` files. Passed in rather than resolved here
/// so the path has exactly one spelling — the port crate's own manifest dir —
/// and so the board's `build.rs` states the layering edge out loud.
pub fn run(linker_script: &[u8], port_dir: &Path) {
    // issue 0288 — do nothing unless we are building FOR riscv64.
    //
    // Everything below cross-compiles ThreadX / NetX / virtio sources with
    // `riscv64-unknown-elf-gcc`. Run on a host build it fails at the first
    // `.S` file:
    //
    //     error occurred in cc-rs: command did not execute successfully:
    //     "riscv64-unknown-elf-gcc" … tx_thread_interrupt_control.S
    //
    // which makes every package that deps this board un-host-buildable. That
    // matters because the source-metadata probe host-compiles a component to
    // record its callback slots, and a standalone example deps its board crate
    // directly — so those packages never got exact executor sizing and fell
    // back to the SystemModel's timer-blind bound.
    //
    // `nros-board-threadx`'s own build.rs already guards this way, for the same
    // stated reason ("so a host-tooled `cargo check` doesn't accidentally pick
    // up the cross compiler"). This helper simply never got the same treatment.
    //
    // Detected by TARGET rather than by a THREADX_* env var: the env vars say
    // where the sources ARE, not what we are building for, and the two differ
    // exactly in the host-tooling case this guards.
    let target = env::var("TARGET").unwrap_or_default();
    if !target.starts_with("riscv64") {
        println!(
            "cargo:warning=nros-board-threadx-qemu-riscv64: TARGET={target} is not riscv64; \
             skipping the ThreadX/NetX cross-compile. The crate still compiles as a Rust \
             shell so host tooling can build packages that dep it (issue 0288)."
        );
        return;
    }

    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let config_dir = manifest_dir.join("config");

    // Resolve workspace root (three levels up from packages/boards/nros-board-threadx-qemu-riscv64/)
    let workspace_root = manifest_dir
        .parent()
        .and_then(|p| p.parent())
        .and_then(|p| p.parent())
        .expect("Could not resolve workspace root");

    let threadx_dir = env_path_or(
        "THREADX_DIR",
        workspace_root.join("third-party/threadx/kernel"),
    );
    let netx_dir = env_path_or(
        "NETX_DIR",
        workspace_root.join("third-party/threadx/netxduo"),
    );
    // 192.3: env-overridable like THREADX_DIR/NETX_DIR above (default in sdk-env.just).
    let virtio_driver_dir = env_path_or(
        "NROS_VIRTIO_NET_NETX_DIR",
        workspace_root.join("packages/drivers/net/virtio-net-netx"),
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

    let threadx_port_dir = threadx_dir.join("ports/risc-v64/gnu");
    assert!(
        threadx_port_dir.join("src").exists(),
        "ThreadX RISC-V 64-bit port not found at {}",
        threadx_port_dir.display()
    );

    let qemu_virt_dir = threadx_port_dir.join("example_build/qemu_virt");
    assert!(
        qemu_virt_dir.join("board.c").exists(),
        "ThreadX QEMU virt board files not found at {}",
        qemu_virt_dir.display()
    );

    let out_dir = PathBuf::from(env::var_os("OUT_DIR").unwrap());

    // ---- Build ThreadX port asm + board ASM overrides + QEMU virt support ----
    //
    // Phase 152.2.B.3 — kernel C + port C compile lifted into the
    // generic `nros-board-threadx` `build.rs`. The RISC-V overlay
    // owns the asm-half of the kernel because:
    //   - 5 port `.S` files are excluded (board ships overrides
    //     with ULONG=4 struct layout fixes).
    //   - `tx_initialize_low_level.S` is overridden by a board-
    //     patched copy (Phase 120.3 16-byte SP alignment).
    //   - QEMU virt support C/asm files (board.c / plic.c /
    //     uart.c / entry.s) live alongside the asm overrides and
    //     also compile with the RISC-V toolchain.
    // Bundled into a single `libthreadx_port_asm.a` archive.
    let mut port_asm = cc::Build::new();
    configure_riscv64(&mut port_asm);
    add_threadx_includes(
        &mut port_asm,
        &threadx_dir,
        &threadx_port_dir,
        &qemu_virt_dir,
        &config_dir,
        port_dir,
    );

    // phase-337 W4.a — the five ULONG=4 overrides now live in the arch-port
    // unit (`nros-board-threadx-port-riscv64`), which owns the exclusion list
    // too. Deriving the excluded names from the files that replace them is
    // what keeps "excluded from upstream" and "compiled from the fork" the
    // same set: the previous hand-written pair of literal lists could drop a
    // name from one and silently compile upstream's 8-byte-ULONG copy.
    //
    // `tx_initialize_low_level.S` is NOT in that set. It is the qemu_virt
    // BSP's low-level init (it includes the board's `csr.h`), patched for the
    // Phase 120.3 16-byte SP alignment, so it stays with the board overlay
    // and is added from `c/` further down.
    let port_asm_dir = port_dir.join("src");
    let overrides = read_port_asm_overrides(&port_asm_dir);
    for entry in std::fs::read_dir(threadx_port_dir.join("src")).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_some_and(|e| e == "S") {
            let name = path.file_name().unwrap().to_str().unwrap_or("").to_string();
            if name == "tx_initialize_low_level.S" || overrides.contains(&name) {
                continue;
            }
            port_asm.file(&path);
        }
    }
    for name in &overrides {
        port_asm.file(port_asm_dir.join(name));
    }

    // QEMU virt board support C files (board.c, plic.c, uart.c).
    // trap.c + hwtimer.c overridden by board crate's c/ versions.
    for entry in std::fs::read_dir(&qemu_virt_dir).unwrap() {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap().to_str().unwrap_or("");
        if name == "trap.c" || name == "hwtimer.c" {
            continue;
        }
        if path.extension().is_some_and(|e| e == "c") {
            port_asm.file(&path);
        }
    }

    // QEMU virt asm — entry.s + tx_initialize_low_level.S come
    // from board crate's c/ dir (entry.s for .init placement;
    // tx_initialize_low_level.S patched for Phase 120.3 SP alignment).
    let qemu_virt_excluded_asm = ["entry.s", "tx_initialize_low_level.S"];
    for entry in std::fs::read_dir(&qemu_virt_dir).unwrap() {
        let path = entry.unwrap().path();
        let ext = path.extension().and_then(|e| e.to_str());
        let name = path.file_name().unwrap().to_str().unwrap_or("");
        if qemu_virt_excluded_asm.contains(&name) {
            continue;
        }
        if ext == Some("S") || ext == Some("s") {
            port_asm.file(&path);
        }
    }
    port_asm.file(manifest_dir.join("c").join("tx_initialize_low_level.S"));

    port_asm.compile("threadx_port_asm");

    // ---- Build NetX Duo ----
    let mut netxduo = cc::Build::new();
    configure_riscv64(&mut netxduo);
    add_threadx_includes(
        &mut netxduo,
        &threadx_dir,
        &threadx_port_dir,
        &qemu_virt_dir,
        &config_dir,
        port_dir,
    );
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

    // ---- Build virtio-net NetX Duo driver ----
    let mut virtio = cc::Build::new();
    configure_riscv64(&mut virtio);
    add_threadx_includes(
        &mut virtio,
        &threadx_dir,
        &threadx_port_dir,
        &qemu_virt_dir,
        &config_dir,
        port_dir,
    );
    add_netx_includes(&mut virtio, &netx_dir, &config_dir);

    virtio
        .include(virtio_driver_dir.join("include"))
        .include(virtio_driver_dir.join("src"));

    virtio.file(virtio_driver_dir.join("src/virtio_mmio.c"));
    virtio.file(virtio_driver_dir.join("src/virtqueue.c"));
    virtio.file(virtio_driver_dir.join("src/virtio_net_nx.c"));

    virtio.compile("virtio_net_netx");

    // ---- Build C glue ----
    // Phase 152.2.B.1 — shared `tx_application_define` stub lives
    // in `nros-board-common`'s `c/threadx_hooks.c`; this overlay
    // ships only the board-specific weak-hook impls (full NetX +
    // virtio-net bring-up in `nros_board_init_eth`, `uart_puts`-
    // backed `nros_board_log`, IP/MAC-derived RNG seed, strong-
    // override of `nros_board_app_stack_size`/`_priority`) and
    // the RISC-V `nros_threadx_set_config` (4-arg, no
    // `interface_name`).
    let mut glue = cc::Build::new();
    configure_riscv64(&mut glue);
    add_threadx_includes(
        &mut glue,
        &threadx_dir,
        &threadx_port_dir,
        &qemu_virt_dir,
        &config_dir,
        port_dir,
    );
    add_netx_includes(&mut glue, &netx_dir, &config_dir);
    glue.include(virtio_driver_dir.join("include"));
    glue.file(manifest_dir.join("c/entry.s"));
    glue.file(manifest_dir.join("c/trap.c"));
    crate::threadx_sources::add_threadx_hooks_source(&mut glue);
    glue.file(manifest_dir.join("c/board_threadx_qemu_riscv64.c"));
    glue.file(manifest_dir.join("c/syscalls.c"));
    glue.file(manifest_dir.join("c/hwtimer.c"));

    glue.compile("glue");

    // ---- Phase 212.M-F.10.3 — NROS_APP_CONFIG source-side emission ----
    //
    // Path C: each board crate emits a one-shot
    // `const nros_app_config_t NROS_APP_CONFIG = { ... };` TU baked into
    // the board's staticlib. `startup.c` reads
    // `NROS_APP_CONFIG.network.{ip,mac,gateway,netmask}` via
    // `<nros/app_config.h>` — the universal extern declaration from
    // the shipped header resolves at link time against this definition.
    //
    // Values transcribe the pre-M.10 `nros.toml` defaults for QEMU
    // SLIRP networking (10.0.2.0/24, host gateway 10.0.2.2, board NIC
    // 10.0.2.40). The example's per-binary `Config` override in
    // `src/lib.rs` controls only the Rust-side runtime values
    // (zenoh locator, domain_id) — the C startup reads the network
    // stack bring-up values directly from `NROS_APP_CONFIG` and
    // happens before Rust user code runs.
    emit_nros_app_config(&out_dir, workspace_root);

    // ---- Link order (reverse dependency) ----
    // `libnros_platform_threadx.a` + `libthreadx_kernel.a` come
    // from the generic `nros-board-threadx` `build.rs` (152.2.B.3
    // Option C lift).
    println!("cargo:rustc-link-lib=static=glue");
    println!("cargo:rustc-link-lib=static=nros_app_config_def");
    println!("cargo:rustc-link-lib=static=virtio_net_netx");
    println!("cargo:rustc-link-lib=static=netxduo");
    println!("cargo:rustc-link-lib=static=threadx_port_asm");

    // Linker script — copy to OUT_DIR so downstream binaries can find it via
    // rustflags = ["-C", "link-arg=-Tlink.lds"] in their .cargo/config.toml.
    // cargo:rustc-link-arg does NOT propagate from library crates to downstream
    // binaries, so we use cargo:rustc-link-search instead.
    std::io::BufWriter::new(std::fs::File::create(out_dir.join("link.lds")).unwrap())
        .write_all(linker_script)
        .unwrap();
    println!("cargo:rustc-link-search={}", out_dir.display());

    // Link picolibc (C standard library for bare-metal RISC-V) and libgcc (compiler builtins)
    if let Some(picolibc_lib_dir) = get_picolibc_lib_dir() {
        println!(
            "cargo:rustc-link-search=native={}",
            picolibc_lib_dir.display()
        );
        println!("cargo:rustc-link-lib=static=c");
    }
    if let Some(libgcc_dir) = get_libgcc_dir() {
        println!("cargo:rustc-link-search=native={}", libgcc_dir.display());
        println!("cargo:rustc-link-lib=static=gcc");
    }

    // ---- Rerun triggers ----
    println!("cargo:rerun-if-changed=c/entry.s");
    println!("cargo:rerun-if-changed=c/trap.c");
    println!("cargo:rerun-if-changed=c/board_threadx_qemu_riscv64.c");
    println!("cargo:rerun-if-changed=c/syscalls.c");
    // phase-337 W4.a — `tx_port.h` and the five `.S` overrides moved to the
    // arch-port unit; their triggers are emitted by that crate's
    // `emit_rerun_directives()`, called from the board's `build.rs` OUTSIDE
    // the riscv64 guard above. They cannot be listed here as bare relative
    // paths: `cargo:rerun-if-changed` resolves those against the BOARD's
    // manifest dir, so they would silently watch nothing.
    println!("cargo:rerun-if-changed=config/tx_user.h");
    println!("cargo:rerun-if-changed=config/nx_port.h");
    println!("cargo:rerun-if-changed=config/nx_user.h");
    println!("cargo:rerun-if-changed=config/link.lds");
    println!("cargo:rerun-if-changed=build.rs");
    // issue 0491 — `THREADX_DIR` / `NETX_DIR` name DIRECTORIES, and cargo
    // compares an env value as TEXT. Fingerprinting the spelling made the
    // ThreadX rows sharing one `--target-dir` invalidate each other forever
    // (one spelling per leaf from `relative = true`, another from `just`).
    // The overlay's own config files are watched above; the two vendored
    // kernels are read-only sources whose compiled files `threadx_sources`
    // declares.
}

/// A path variable with an in-repo default, canonicalised so every consumer
/// spells it the same way (issue 0491). Never fingerprinted as a string.
fn env_path_or(name: &str, default: PathBuf) -> PathBuf {
    let raw = env::var(name).map(PathBuf::from).unwrap_or(default);
    nros_build_paths::canonical(&raw)
}

fn configure_riscv64(build: &mut cc::Build) {
    // Use the RISC-V GCC cross-compiler
    build
        .compiler(nros_build_paths::riscv64::tool_or_legacy("gcc"))
        .archiver(nros_build_paths::riscv64::tool_or_legacy("ar"))
        .opt_level(2)
        .flag("-march=rv64gc")
        .flag("-mabi=lp64d")
        .flag("-mcmodel=medany")
        .flag("-ffunction-sections")
        .flag("-fdata-sections")
        .flag("-fno-builtin")
        .flag("-Wno-unused-parameter")
        .flag("-Wno-sign-compare")
        .define("TX_INCLUDE_USER_DEFINE_FILE", None)
        .define("NX_INCLUDE_USER_DEFINE_FILE", None)
        // Phase 155.E — `NX_BSD_ENABLE_NATIVE_API` must be on for
        // the entire board build, not just at nx_user.h scope.
        // Without it, nxd_bsd.h's "legacy override" block (line
        // 203-...) fires `#define nx_bsd_suseconds_t suseconds_t`,
        // which then expands the unconditional `typedef LONG
        // nx_bsd_suseconds_t;` at line 629 into a SECOND
        // `typedef LONG suseconds_t;` that conflicts with picolibc's
        // own `<sys/types.h>` definition. The nx_user.h define
        // alone doesn't reach the BSD addon when nxd_bsd.h is
        // included from a TU that doesn't pull nx_api.h's user-
        // define chain first; defining it via cc::Build is the
        // canonical override (per nxd_bsd.h:144 comment).
        .define("NX_BSD_ENABLE_NATIVE_API", None);
    build.warnings(false);
    // issue 0383 — implicit-function-declaration / int-conversion as ERRORS.
    // Safe next to `warnings(false)`: cc-rs only omits `-Wall`/`-Wextra` there,
    // it passes no `-w`, so the gate stays live (see nros_cc_flags docs).
    nros_cc_flags::strict_decls(build);

    // picolibc provides C standard library headers (string.h, stdint.h, etc.)
    // picolibc's <machine/endian.h> defines htonl as __bswap32 on LE, which is
    // compatible with our nx_port.h's #ifndef-guarded __builtin_bswap32 definitions.
    // Do NOT use --specs=picolibc.specs (it enables TLS errno which crashes on bare-metal)
    if let Some(sysroot) = get_picolibc_sysroot() {
        build.include(sysroot.join("include"));
    }

    // Phase 152.2.B.2 — `THREADX_CFLAGS` extension point. RISC-V
    // overlay's `.cargo/config.toml` may set additional flags
    // (e.g. `-mtune=…`); appending here keeps overlay-specific
    // tunables out of this build.rs.
    crate::threadx_sources::apply_threadx_cflags(build);
}

/// Get the picolibc sysroot path for RISC-V (provides C standard library headers).
fn get_picolibc_sysroot() -> Option<PathBuf> {
    if let Ok(output) = Command::new(nros_build_paths::riscv64::tool_or_legacy("gcc"))
        .args([
            "-march=rv64gc",
            "-mabi=lp64d",
            "--specs=picolibc.specs",
            "-print-sysroot",
        ])
        .output()
        && output.status.success()
    {
        let sysroot = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !sysroot.is_empty() {
            let path = PathBuf::from(&sysroot);
            if path.join("include").exists() {
                return Some(path);
            }
        }
    }
    // Fallback: known system location
    let fallback = PathBuf::from("/usr/lib/picolibc/riscv64-unknown-elf");
    if fallback.join("include").exists() {
        return Some(fallback);
    }
    None
}

/// Get the picolibc library directory for rv64gc/lp64d (libc.a).
fn get_picolibc_lib_dir() -> Option<PathBuf> {
    // Try gcc -print-sysroot with picolibc specs
    if let Some(sysroot) = get_picolibc_sysroot() {
        // Multilib: sysroot/lib/rv64imafdc/lp64d/ (rv64gc = rv64imafdc)
        let multilib = sysroot.join("lib/rv64imafdc/lp64d");
        if multilib.join("libc.a").exists() {
            return Some(multilib);
        }
        // Single-lib fallback
        let single = sysroot.join("lib");
        if single.join("libc.a").exists() {
            return Some(single);
        }
    }
    // issue 0657 — ask the COMPILER where its libc is, which is the question,
    // and works for either toolchain. The probe above knows only picolibc's
    // sysroot layout, so on the xPack `riscv-none-elf` build that `nros setup`
    // provisions (newlib bundled, no `picolibc.specs`) it found nothing, no
    // `-lc` reached the link, and the image failed on `strcmp`, `snprintf`,
    // `memchr` … — a libc that was sitting in the toolchain all along.
    if let Ok(output) = Command::new(nros_build_paths::riscv64::tool_or_legacy("gcc"))
        .args(["-march=rv64gc", "-mabi=lp64d", "-print-file-name=libc.a"])
        .output()
        && output.status.success()
    {
        let path = PathBuf::from(String::from_utf8_lossy(&output.stdout).trim().to_string());
        // gcc echoes the bare name back when it cannot find the file.
        if path.is_absolute() && path.exists() {
            if let Some(dir) = path.parent() {
                return Some(dir.to_path_buf());
            }
        }
    }
    None
}

/// Get the libgcc directory for rv64gc/lp64d.
fn get_libgcc_dir() -> Option<PathBuf> {
    if let Ok(output) = Command::new(nros_build_paths::riscv64::tool_or_legacy("gcc"))
        .args(["-march=rv64gc", "-mabi=lp64d", "-print-libgcc-file-name"])
        .output()
        && output.status.success()
    {
        let path = PathBuf::from(String::from_utf8_lossy(&output.stdout).trim().to_string());
        return path.parent().map(|p| p.to_path_buf());
    }
    None
}

/// The `.S` files the arch-port unit ships, i.e. the upstream port sources it
/// REPLACES.
///
/// Globbed rather than hard-coded: the files are the list, so a file added or
/// removed there cannot leave a stale literal behind. Empty is a hard error —
/// it would mean silently compiling upstream's whole 8-byte-`ULONG` port.
fn read_port_asm_overrides(port_asm_dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(port_asm_dir)
        .unwrap_or_else(|e| {
            panic!(
                "nros-board-threadx-qemu-riscv64: read_dir({}): {e} — the arch-port unit \
                 (nros-board-threadx-port-riscv64) is missing",
                port_asm_dir.display()
            )
        })
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "S"))
        .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
        .collect();
    names.sort();
    assert!(
        !names.is_empty(),
        "nros-board-threadx-qemu-riscv64: no `.S` overrides in {} — see \
         nros-board-threadx-port-riscv64's crate docs on why upstream's port cannot be used",
        port_asm_dir.display()
    );
    names
}

/// `port_override_dir` is the arch-port unit's `port/` root. Its `inc/` is
/// listed **before** the upstream port's `inc/` on purpose: it carries the
/// forked `tx_port.h`, and losing that precedence compiles cleanly against
/// upstream's 8-byte `ULONG` and corrupts packets at runtime instead
/// (`threadx_hooks.c`'s `_Static_assert` is the backstop).
fn add_threadx_includes(
    build: &mut cc::Build,
    threadx_dir: &Path,
    port_dir: &Path,
    qemu_virt_dir: &Path,
    config_dir: &Path,
    port_override_dir: &Path,
) {
    build
        .include(config_dir)
        .include(port_override_dir.join("inc"))
        .include(threadx_dir.join("common/inc"))
        .include(port_dir.join("inc"))
        .include(qemu_virt_dir);
}

fn add_netx_includes(build: &mut cc::Build, netx_dir: &Path, config_dir: &Path) {
    build
        .include(config_dir)
        .include(netx_dir.join("common/inc"))
        .include(netx_dir.join("addons/BSD"));
}

/// Phase 212.M-F.10.3 — Path C `NROS_APP_CONFIG` source-side emission.
///
/// Writes a one-shot C translation unit that defines the universal
/// `const nros_app_config_t NROS_APP_CONFIG = { ... };` symbol from the
/// board's pre-M.10 default network values (QEMU SLIRP topology). The
/// TU is compiled into `libnros_app_config_def.a` and linked alongside
/// the other board staticlibs; the board's `startup.c` resolves its
/// `extern const nros_app_config_t NROS_APP_CONFIG;` (from
/// `<nros/app_config.h>`) against this definition at link time.
fn emit_nros_app_config(out_dir: &Path, workspace_root: &Path) {
    let src_path = out_dir.join("nros_app_config_def.c");
    let nros_c_include = workspace_root.join("packages/api/nros-c/include");

    let body = r#"/*
 * Auto-generated by nros-board-threadx-qemu-riscv64/build.rs
 * Phase 212.M-F.10.3 Path C — `NROS_APP_CONFIG` source-side emission.
 *
 * Defines the universal `nros_app_config_t NROS_APP_CONFIG` symbol that
 * the shipped `<nros/app_config.h>` declares `extern` on non-Zephyr
 * platforms. Values mirror the board's pre-M.10 `nros.toml` defaults
 * for QEMU SLIRP networking (10.0.2.0/24, host gateway 10.0.2.2,
 * board NIC 10.0.2.40, zenohd at host:7553).
 */

#include <stdint.h>
#include <nros/app_config.h>

const nros_app_config_t NROS_APP_CONFIG = {
    .zenoh = {
        .locator   = "tcp/10.0.2.2:7553",
        .domain_id = 0,
    },
    .network = {
        .ip      = { 10, 0, 2, 40 },
        .mac     = { 0x52, 0x54, 0x00, 0x12, 0x34, 0x56 },
        .gateway = { 10, 0, 2, 2 },
        .netmask = { 255, 255, 255, 0 },
        .prefix  = 24,
    },
    .scheduling = {
        .app_priority            = 0,
        .zenoh_read_priority     = 0,
        .zenoh_lease_priority    = 0,
        .poll_priority           = 0,
        .app_stack_bytes         = 0,
        .zenoh_read_stack_bytes  = 0,
        .zenoh_lease_stack_bytes = 0,
        .poll_interval_ms        = 0,
    },
};
"#;
    std::fs::write(&src_path, body).expect("write nros_app_config_def.c");

    let mut build = cc::Build::new();
    configure_riscv64(&mut build);
    // Reach `<nros/app_config.h>` (the universal extern declaration).
    build.include(&nros_c_include);
    build.file(&src_path);
    build.compile("nros_app_config_def");
}
