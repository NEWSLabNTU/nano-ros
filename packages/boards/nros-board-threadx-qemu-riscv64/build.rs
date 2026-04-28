//! Build script for nros-board-threadx-qemu-riscv64
//!
//! Cross-compiles the ThreadX RISC-V 64-bit port, NetX Duo stack,
//! virtio-net NetX Duo driver, and board-specific C/assembly code
//! into static libraries linked into the final binary.
//!
//! Environment variables (auto-set by justfile recipes):
//!   THREADX_DIR          — ThreadX kernel source root (default: third-party/threadx/kernel)
//!   NETX_DIR             — NetX Duo source root (default: third-party/threadx/netxduo)

use std::env;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let config_dir = manifest_dir.join("config");

    // Resolve workspace root (three levels up from packages/boards/nros-board-threadx-qemu-riscv64/)
    let workspace_root = manifest_dir
        .parent()
        .and_then(|p| p.parent())
        .and_then(|p| p.parent())
        .expect("Could not resolve workspace root");

    let threadx_dir = env_path_or("THREADX_DIR", workspace_root.join("third-party/threadx/kernel"));
    let netx_dir = env_path_or("NETX_DIR", workspace_root.join("third-party/threadx/netxduo"));
    let virtio_driver_dir = workspace_root.join("packages/drivers/virtio-net-netx");

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

    // ---- Build ThreadX kernel ----
    let mut threadx = cc::Build::new();
    configure_riscv64(&mut threadx);
    add_threadx_includes(&mut threadx, &threadx_dir, &threadx_port_dir, &qemu_virt_dir, &config_dir);

    // All kernel common source files
    for entry in std::fs::read_dir(threadx_dir.join("common/src")).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_some_and(|e| e == "c") {
            threadx.file(&path);
        }
    }

    // RISC-V port C files
    for entry in std::fs::read_dir(threadx_port_dir.join("src")).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_some_and(|e| e == "c") {
            threadx.file(&path);
        }
    }

    // RISC-V port assembly files — exclude files that the board crate
    // overrides with ULONG=4 struct layout fixes (see c/tx_thread_*.S).
    // Also exclude tx_initialize_low_level.S (QEMU virt board provides its own).
    let excluded_asm: &[&str] = &[
        "tx_initialize_low_level.S",
        "tx_thread_schedule.S",
        "tx_thread_context_save.S",
        "tx_thread_context_restore.S",
        "tx_thread_stack_build.S",
        "tx_thread_system_return.S",
    ];
    for entry in std::fs::read_dir(threadx_port_dir.join("src")).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_some_and(|e| e == "S") {
            let name = path.file_name().unwrap().to_str().unwrap_or("");
            if excluded_asm.contains(&name) {
                continue;
            }
            threadx.file(&path);
        }
    }

    // Board-local assembly overrides (ULONG=4 struct offset fixes)
    for asm_name in &[
        "tx_thread_schedule.S",
        "tx_thread_context_save.S",
        "tx_thread_context_restore.S",
        "tx_thread_stack_build.S",
        "tx_thread_system_return.S",
    ] {
        threadx.file(manifest_dir.join("c").join(asm_name));
    }

    // QEMU virt board support C files (board.c, plic.c, uart.c)
    // trap.c and hwtimer.c are excluded — board crate provides its own versions
    for entry in std::fs::read_dir(&qemu_virt_dir).unwrap() {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap().to_str().unwrap_or("");
        if name == "trap.c" || name == "hwtimer.c" {
            continue; // use board crate's c/ versions instead
        }
        if path.extension().is_some_and(|e| e == "c") {
            threadx.file(&path);
        }
    }

    // QEMU virt assembly files (tx_initialize_low_level.S only — entry.s is
    // provided by the board crate with .init section placement to guarantee
    // _start appears at 0x80000000 before any Rust .text sections)
    for entry in std::fs::read_dir(&qemu_virt_dir).unwrap() {
        let path = entry.unwrap().path();
        let ext = path.extension().and_then(|e| e.to_str());
        let name = path.file_name().unwrap().to_str().unwrap_or("");
        if name == "entry.s" {
            continue; // use board crate's c/entry.s instead
        }
        if ext == Some("S") || ext == Some("s") {
            threadx.file(&path);
        }
    }

    threadx.compile("threadx");

    // ---- Build NetX Duo ----
    let mut netxduo = cc::Build::new();
    configure_riscv64(&mut netxduo);
    add_threadx_includes(&mut netxduo, &threadx_dir, &threadx_port_dir, &qemu_virt_dir, &config_dir);
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
    add_threadx_includes(&mut virtio, &threadx_dir, &threadx_port_dir, &qemu_virt_dir, &config_dir);
    add_netx_includes(&mut virtio, &netx_dir, &config_dir);

    virtio
        .include(virtio_driver_dir.join("include"))
        .include(virtio_driver_dir.join("src"));

    virtio.file(virtio_driver_dir.join("src/virtio_mmio.c"));
    virtio.file(virtio_driver_dir.join("src/virtqueue.c"));
    virtio.file(virtio_driver_dir.join("src/virtio_net_nx.c"));

    virtio.compile("virtio_net_netx");

    // ---- Build C glue (app_define.c) ----
    let mut glue = cc::Build::new();
    configure_riscv64(&mut glue);
    add_threadx_includes(&mut glue, &threadx_dir, &threadx_port_dir, &qemu_virt_dir, &config_dir);
    add_netx_includes(&mut glue, &netx_dir, &config_dir);
    glue.include(virtio_driver_dir.join("include"));
    glue.file(manifest_dir.join("c/entry.s"));
    glue.file(manifest_dir.join("c/trap.c"));
    glue.file(manifest_dir.join("c/app_define.c"));
    glue.file(manifest_dir.join("c/syscalls.c"));
    glue.file(manifest_dir.join("c/hwtimer.c"));

    glue.compile("glue");

    // ---- Link order (reverse dependency) ----
    println!("cargo:rustc-link-lib=static=glue");
    println!("cargo:rustc-link-lib=static=virtio_net_netx");
    println!("cargo:rustc-link-lib=static=netxduo");
    println!("cargo:rustc-link-lib=static=threadx");

    // Linker script — copy to OUT_DIR so downstream binaries can find it via
    // rustflags = ["-C", "link-arg=-Tlink.lds"] in their .cargo/config.toml.
    // cargo:rustc-link-arg does NOT propagate from library crates to downstream
    // binaries, so we use cargo:rustc-link-search instead.
    std::io::BufWriter::new(std::fs::File::create(out_dir.join("link.lds")).unwrap())
        .write_all(include_bytes!("config/link.lds"))
        .unwrap();
    println!("cargo:rustc-link-search={}", out_dir.display());

    // Link picolibc (C standard library for bare-metal RISC-V) and libgcc (compiler builtins)
    if let Some(picolibc_lib_dir) = get_picolibc_lib_dir() {
        println!("cargo:rustc-link-search=native={}", picolibc_lib_dir.display());
        println!("cargo:rustc-link-lib=static=c");
    }
    if let Some(libgcc_dir) = get_libgcc_dir() {
        println!("cargo:rustc-link-search=native={}", libgcc_dir.display());
        println!("cargo:rustc-link-lib=static=gcc");
    }

    // ---- Rerun triggers ----
    println!("cargo:rerun-if-changed=c/entry.s");
    println!("cargo:rerun-if-changed=c/trap.c");
    println!("cargo:rerun-if-changed=c/app_define.c");
    println!("cargo:rerun-if-changed=c/syscalls.c");
    println!("cargo:rerun-if-changed=config/tx_port.h");
    println!("cargo:rerun-if-changed=config/tx_user.h");
    println!("cargo:rerun-if-changed=config/nx_port.h");
    println!("cargo:rerun-if-changed=config/nx_user.h");
    println!("cargo:rerun-if-changed=config/link.lds");
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=THREADX_DIR");
    println!("cargo:rerun-if-env-changed=NETX_DIR");
}

fn env_path_or(name: &str, default: PathBuf) -> PathBuf {
    env::var(name).map(PathBuf::from).unwrap_or(default)
}

fn configure_riscv64(build: &mut cc::Build) {
    // Use the RISC-V GCC cross-compiler
    build
        .compiler("riscv64-unknown-elf-gcc")
        .archiver("riscv64-unknown-elf-ar")
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
        .define("NX_INCLUDE_USER_DEFINE_FILE", None);
    build.warnings(false);

    // picolibc provides C standard library headers (string.h, stdint.h, etc.)
    // picolibc's <machine/endian.h> defines htonl as __bswap32 on LE, which is
    // compatible with our nx_port.h's #ifndef-guarded __builtin_bswap32 definitions.
    // Do NOT use --specs=picolibc.specs (it enables TLS errno which crashes on bare-metal)
    if let Some(sysroot) = get_picolibc_sysroot() {
        build.include(sysroot.join("include"));
    }
}

/// Get the picolibc sysroot path for RISC-V (provides C standard library headers).
fn get_picolibc_sysroot() -> Option<PathBuf> {
    if let Ok(output) = Command::new("riscv64-unknown-elf-gcc")
        .args([
            "-march=rv64gc",
            "-mabi=lp64d",
            "--specs=picolibc.specs",
            "-print-sysroot",
        ])
        .output()
    {
        if output.status.success() {
            let sysroot = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !sysroot.is_empty() {
                let path = PathBuf::from(&sysroot);
                if path.join("include").exists() {
                    return Some(path);
                }
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
    None
}

/// Get the libgcc directory for rv64gc/lp64d.
fn get_libgcc_dir() -> Option<PathBuf> {
    if let Ok(output) = Command::new("riscv64-unknown-elf-gcc")
        .args(["-march=rv64gc", "-mabi=lp64d", "-print-libgcc-file-name"])
        .output()
    {
        if output.status.success() {
            let path = PathBuf::from(String::from_utf8_lossy(&output.stdout).trim().to_string());
            return path.parent().map(|p| p.to_path_buf());
        }
    }
    None
}

fn add_threadx_includes(
    build: &mut cc::Build,
    threadx_dir: &Path,
    port_dir: &Path,
    qemu_virt_dir: &Path,
    config_dir: &Path,
) {
    build
        .include(config_dir)
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
