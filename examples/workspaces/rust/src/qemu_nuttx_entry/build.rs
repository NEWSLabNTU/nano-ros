// Phase 225.O follow-up (known-issue #18) — standalone bootable NuttX
// image via the cargo lane.
//
// NuttX uses a flat-build model: the cargo binary IS the kernel image.
// This build.rs links the prebuilt NuttX kernel libraries (the
// `staging/lib*.a` produced by `scripts/nuttx/build-nuttx.sh`) into the
// final ELF, mirroring the proven
// `packages/testing/nros-tests/bins/logging-smoke-nuttx-qemu-arm/build.rs`.
//
// The board dep crate (`nros-board-nuttx-qemu-arm`) already compiles the
// nano-ros NuttX C platform shim (`platform.c` + `net.c`) into
// `nros_platform_nuttx` and emits its `cargo:rustc-link-lib`, so this
// script does NOT recompile it — doing so would double-define the
// platform symbols. This script only stages the linker script, the
// vector-table head object, and the kernel-library link group.
//
// Required env: `NUTTX_DIR` -> the configured NuttX source tree (with
// `staging/lib*.a` + `include/nuttx/config.h`). Set by the workspace
// fixture lane (`just nuttx build-examples`).

use std::{path::PathBuf, process::Command};

fn main() {
    println!("cargo:rerun-if-env-changed=NUTTX_DIR");
    println!("cargo:rerun-if-changed=build.rs");

    // Resolve the NuttX tree. Without `NUTTX_DIR` we cannot link a
    // bootable image; on a host `cargo check` (no NuttX SDK) we skip
    // the link wiring so the crate still type-checks for the macro
    // expansion. The actual fixture build always sets `NUTTX_DIR`.
    let nuttx_dir = match std::env::var("NUTTX_DIR") {
        Ok(dir) => PathBuf::from(dir),
        Err(_) => return,
    };

    let staging = nuttx_dir.join("staging");
    let linker_script = nuttx_dir.join("boards/arm/qemu/qemu-armv7a/scripts/dramboot.ld");
    let vectortab = nuttx_dir.join("arch/arm/src/arm_vectortab.o");
    println!(
        "cargo:rerun-if-changed={}",
        staging.join("libc.a").display()
    );
    println!("cargo:rerun-if-changed={}", vectortab.display());
    println!("cargo:rerun-if-changed={}", linker_script.display());

    // Skip the link wiring until the NuttX kernel export is provisioned
    // (`just nuttx build`). The crate still compiles; the link only
    // matters when producing the bootable image.
    if !staging.join("libc.a").exists() {
        return;
    }

    // The flat-build linker script (`dramboot.ld`) carries cpp
    // directives that need `<nuttx/config.h>` — preprocess it the same
    // way the NuttX build itself does.
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let processed_ld = out_dir.join("dramboot.ld");
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
        .expect("failed to preprocess NuttX linker script (arm-none-eabi-gcc)");
    assert!(status.success(), "NuttX linker script preprocessing failed");

    // Toolchain libgcc (ARM intrinsics) — the cortex-a7 hard-float
    // multilib path. `-nodefaultlibs` drops gcc's own, so add it back.
    let gcc_out = Command::new("arm-none-eabi-gcc")
        .args([
            "-mcpu=cortex-a7",
            "-mfloat-abi=hard",
            "-mfpu=neon-vfpv4",
            "-print-libgcc-file-name",
        ])
        .output()
        .expect("failed to find libgcc (arm-none-eabi-gcc)");
    let libgcc = String::from_utf8(gcc_out.stdout)
        .unwrap()
        .trim()
        .to_string();

    let board_src = nuttx_dir.join("arch/arm/src/board");

    // --- Empty builtins table (pre-empts contaminated libapps) ---
    // Compile `c/nuttx_builtins_stub.c` to an object and force-link it
    // BEFORE `-lapps` so OUR `g_builtins` / `g_builtin_count` satisfy
    // libc's `lib_builtin_forindex.o` reference. This stops libapps'
    // `builtin_list.o` (which references the staged C/C++ example apps'
    // `*_main` -> undefined `nros_*` FFI) from ever being pulled. See
    // the file header for the full rationale.
    let stub_obj = out_dir.join("nuttx_builtins_stub.o");
    let stub_src = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("c/nuttx_builtins_stub.c");
    println!("cargo:rerun-if-changed={}", stub_src.display());
    let stub_status = Command::new("arm-none-eabi-gcc")
        .args([
            "-c",
            "-mcpu=cortex-a7",
            "-mfloat-abi=hard",
            "-mfpu=neon-vfpv4",
            "-std=c11",
            "-D__NuttX__",
            "-D__KERNEL__",
        ])
        .arg(format!("-isystem{}", nuttx_dir.join("include").display()))
        .arg(format!(
            "-I{}",
            nuttx_dir.join("arch/arm/src/chip").display()
        ))
        .arg(format!(
            "-I{}",
            nuttx_dir.join("arch/arm/src/common").display()
        ))
        .arg(format!(
            "-I{}",
            nuttx_dir.join("arch/arm/src/armv7-a").display()
        ))
        .arg(format!("-I{}", nuttx_dir.join("sched").display()))
        .arg(&stub_src)
        .arg("-o")
        .arg(&stub_obj)
        .status()
        .expect("failed to compile nuttx_builtins_stub.c (arm-none-eabi-gcc)");
    assert!(
        stub_status.success(),
        "nuttx_builtins_stub.c compile failed"
    );

    // --- Link line (order matters) ---
    println!("cargo:rustc-link-arg=-T{}", processed_ld.display());
    println!("cargo:rustc-link-arg=--entry=__start");
    println!("cargo:rustc-link-arg=-nostartfiles");
    println!("cargo:rustc-link-arg=-nodefaultlibs");
    // Vector-table head object — the flat-build reset path.
    println!("cargo:rustc-link-arg={}", vectortab.display());
    // Empty builtins table — force-linked before `-lapps` (below).
    println!("cargo:rustc-link-arg={}", stub_obj.display());
    println!("cargo:rustc-link-arg=-L{}", staging.display());
    println!("cargo:rustc-link-arg=-L{}", board_src.display());
    // The kernel libs are mutually recursive — wrap them in a group.
    // The board crate's `nsh_main` (`nros-board-nuttx-qemu-arm`'s
    // `entry.rs`, `#[no_mangle]` + `#[used]`) comes from the Rust rlibs
    // emitted BEFORE these `-l` flags, so libsched's `nsh_main`
    // reference resolves to it, not `-lapps`' NSH `nsh_main`. (The empty
    // builtins stub above keeps `-lapps`' `builtin_list.o` — and its
    // example-app refs — from being pulled at all.)
    println!("cargo:rustc-link-arg=-Wl,--start-group");
    for lib in [
        "sched", "drivers", "boards", "c", "mm", "arch", "xx", "apps", "net", "crypto", "fs",
        "binfmt", "openamp", "board",
    ] {
        println!("cargo:rustc-link-arg=-l{lib}");
    }
    println!("cargo:rustc-link-arg={libgcc}");
    println!("cargo:rustc-link-arg=-Wl,--end-group");
}
