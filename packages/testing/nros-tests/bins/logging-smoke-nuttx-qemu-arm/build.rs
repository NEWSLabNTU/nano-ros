// Phase 88.15.c — copy of `examples/qemu-arm-nuttx/rust/talker/build.rs`.
// NuttX uses a flat-build model: the Rust binary IS the kernel image.
// Resolves NuttX `staging/lib*.a` via $NUTTX_DIR, preprocesses the
// kernel linker script, and adds the kernel archives to the link line.
//
// Keep this in lockstep with the example tree's build.rs — they share
// the same NuttX target + linker invariants.

use std::path::PathBuf;
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-env-changed=NUTTX_DIR");

    let nuttx_dir = match std::env::var("NUTTX_DIR") {
        Ok(dir) => PathBuf::from(dir),
        Err(_) => return,
    };

    let staging = nuttx_dir.join("staging");
    println!(
        "cargo:rerun-if-changed={}",
        nuttx_dir.join("staging/libc.a").display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        nuttx_dir.join("arch/arm/src/arm_vectortab.o").display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        nuttx_dir
            .join("boards/arm/qemu/qemu-armv7a/scripts/dramboot.ld")
            .display()
    );
    if !staging.join("libc.a").exists() {
        return;
    }

    let out_dir = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let processed_ld = out_dir.join("dramboot.ld");
    let linker_script = nuttx_dir.join("boards/arm/qemu/qemu-armv7a/scripts/dramboot.ld");

    let status = Command::new("arm-none-eabi-gcc")
        .args([
            "-E",
            "-P",
            "-x",
            "c",
            &format!("-isystem{}", nuttx_dir.join("include").display()),
            "-D__NuttX__",
            "-D__KERNEL__",
            &format!("-I{}", nuttx_dir.join("arch/arm/src/chip").display()),
            &format!("-I{}", nuttx_dir.join("arch/arm/src/common").display()),
            &format!("-I{}", nuttx_dir.join("arch/arm/src/armv7-a").display()),
            &format!("-I{}", nuttx_dir.join("sched").display()),
        ])
        .arg(&linker_script)
        .arg("-o")
        .arg(&processed_ld)
        .status()
        .expect("failed to preprocess linker script");
    assert!(status.success(), "linker script preprocessing failed");

    // Phase 208.B Track A — paths come from `nros-build-paths`
    // (walks up to `nros-sdk-index.toml`); env vars stay as overrides.
    let cffi_include = nros_build_paths::nros_platform_cffi_include();
    let platform_src = nros_build_paths::nros_platform_posix_src();
    let mut platform = cc::Build::new();
    platform.compiler("arm-none-eabi-gcc");
    platform.flag("-mcpu=cortex-a7");
    platform.flag("-mfloat-abi=hard");
    platform.flag("-mfpu=neon-vfpv4");
    platform.flag("-std=c11");
    platform.define("__NuttX__", None);
    platform.include(&cffi_include);
    platform.include(nuttx_dir.join("include"));
    platform.include(nuttx_dir.join("arch/arm/src/chip"));
    platform.include(nuttx_dir.join("arch/arm/src/common"));
    platform.include(nuttx_dir.join("arch/arm/src/armv7-a"));
    platform.include(nuttx_dir.join("sched"));
    platform.file(platform_src.join("platform.c"));
    platform.file(platform_src.join("net.c"));
    platform.compile("nros_logging_smoke_nuttx_platform");

    let board_src = nuttx_dir.join("arch/arm/src/board");
    let vectortab = nuttx_dir.join("arch/arm/src/arm_vectortab.o");

    let gcc_out = Command::new("arm-none-eabi-gcc")
        .args([
            "-mcpu=cortex-a7",
            "-mfloat-abi=hard",
            "-mfpu=neon-vfpv4",
            "-print-libgcc-file-name",
        ])
        .output()
        .expect("failed to find libgcc");
    let libgcc = String::from_utf8(gcc_out.stdout)
        .unwrap()
        .trim()
        .to_string();

    println!("cargo:rustc-link-arg=-T{}", processed_ld.display());
    println!("cargo:rustc-link-arg=--entry=__start");
    println!("cargo:rustc-link-arg=-nostartfiles");
    println!("cargo:rustc-link-arg=-nodefaultlibs");
    println!("cargo:rustc-link-arg={}", vectortab.display());
    println!("cargo:rustc-link-arg=-L{}", staging.display());
    println!("cargo:rustc-link-arg=-L{}", board_src.display());
    println!("cargo:rustc-link-arg=-Wl,--start-group");
    println!(
        "cargo:rustc-link-arg={}",
        out_dir
            .join("libnros_logging_smoke_nuttx_platform.a")
            .display()
    );
    for lib in [
        "sched", "drivers", "boards", "c", "mm", "arch", "xx", "apps", "net", "crypto", "fs",
        "binfmt", "openamp", "board",
    ] {
        println!("cargo:rustc-link-arg=-l{lib}");
    }
    println!("cargo:rustc-link-arg={libgcc}");
    println!("cargo:rustc-link-arg=-Wl,--end-group");

    println!("cargo:rerun-if-changed={}", linker_script.display());
    println!("cargo:rerun-if-changed={}", platform_src.display());
}
