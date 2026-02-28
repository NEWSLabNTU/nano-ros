//! Build script for nros-threadx-qemu-riscv64
//!
//! Cross-compiles the ThreadX RISC-V 64-bit port, NetX Duo stack,
//! virtio-net NetX Duo driver, and board-specific C/assembly code
//! into static libraries linked into the final binary.
//!
//! Environment variables (auto-set by justfile recipes):
//!   THREADX_DIR          — ThreadX kernel source root (default: external/threadx)
//!   NETX_DIR             — NetX Duo source root (default: external/netxduo)

use std::env;
use std::path::{Path, PathBuf};

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let config_dir = manifest_dir.join("config");

    // Resolve workspace root (three levels up from packages/boards/nros-threadx-qemu-riscv64/)
    let workspace_root = manifest_dir
        .parent()
        .and_then(|p| p.parent())
        .and_then(|p| p.parent())
        .expect("Could not resolve workspace root");

    let threadx_dir = env_path_or("THREADX_DIR", workspace_root.join("external/threadx"));
    let netx_dir = env_path_or("NETX_DIR", workspace_root.join("external/netxduo"));
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

    // RISC-V port assembly files
    for entry in std::fs::read_dir(threadx_port_dir.join("src")).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_some_and(|e| e == "S") {
            threadx.file(&path);
        }
    }

    // QEMU virt board support C files (board.c, plic.c, uart.c, hwtimer.c, trap.c)
    for entry in std::fs::read_dir(&qemu_virt_dir).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_some_and(|e| e == "c") {
            threadx.file(&path);
        }
    }

    // QEMU virt assembly files (entry.s, tx_initialize_low_level.S)
    for entry in std::fs::read_dir(&qemu_virt_dir).unwrap() {
        let path = entry.unwrap().path();
        let ext = path.extension().and_then(|e| e.to_str());
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
    glue.file(manifest_dir.join("c/app_define.c"));

    glue.compile("glue");

    // ---- Link order (reverse dependency) ----
    println!("cargo:rustc-link-lib=static=glue");
    println!("cargo:rustc-link-lib=static=virtio_net_netx");
    println!("cargo:rustc-link-lib=static=netxduo");
    println!("cargo:rustc-link-lib=static=threadx");

    // Linker script
    println!("cargo:rustc-link-arg=-T{}", config_dir.join("link.lds").display());
    println!("cargo:rustc-link-arg=-nostdlib");

    // ---- Rerun triggers ----
    println!("cargo:rerun-if-changed=c/app_define.c");
    println!("cargo:rerun-if-changed=config/tx_user.h");
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
