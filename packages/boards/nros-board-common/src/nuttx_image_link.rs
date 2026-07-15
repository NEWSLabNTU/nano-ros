//! #127 — shared NuttX flat-build image link (the board-centric entry-link
//! convention, RFC-0032 "third leg").
//!
//! NuttX uses a flat-build model: the cargo binary IS the kernel image. This
//! helper stages the *dynamic* link inputs in the BOARD crate's build script
//! and emits the PROPAGATING directives (`cargo:rustc-link-search` /
//! `cargo:rustc-link-lib` — which, unlike `cargo:rustc-link-arg`, propagate
//! from a dependency's build script to the final `[[bin]]` link), so a
//! dependent Entry pkg links a bootable image with ZERO build.rs of its own:
//!
//! 1. cpp-preprocess the board linker script (it carries directives needing
//!    `<nuttx/config.h>`) → `OUT_DIR/<script name>` — found by GNU ld's
//!    `-T<name>` by-name lookup through the propagated `-L OUT_DIR`;
//! 2. compile the board's builtins stub (empty `g_builtins` table — blocks
//!    libapps' contaminated `builtin_list.o`; see the stub's header in
//!    `nros-board-nuttx-qemu-arm/c/nuttx_builtins_stub.c`);
//! 3. archive the arch vector-table object + the stub into
//!    `libnros_nuttx_boot.a` and link it `-bundle,+whole-archive` (both
//!    members always pulled; the ld script's section placement pins the
//!    vector table, not object order);
//! 4. emit link-search for `OUT_DIR` + `$NUTTX_DIR/staging` + the board lib
//!    dir.
//!
//! The *static* args (`-T<script>`, `--entry=__start`, `-nostartfiles`,
//! `-nodefaultlibs`, the kernel-lib `--start-group` list, `-lgcc`) stay in
//! the Entry pkg's `.cargo/config.toml` rustflags, rendered from the board
//! descriptor's `cargo_config` (nros-board.toml). The cpu link-args there
//! select the gcc driver's multilib, so the trailing `-lgcc` resolves the
//! ARM intrinsics without an absolute `-print-libgcc-file-name` path.
//!
//! Arch-specifics come from the `NUTTX_*` env family (the board overlay sets
//! them); defaults are the qemu-arm cortex-a7 hardfloat values, mirroring
//! [`crate::nuttx_platform_build::run_platform`].
//!
//! Env (all optional, arm defaults):
//! - `NUTTX_CROSS` — cross C compiler driver (default `arm-none-eabi-gcc`);
//!   the archiver is derived by swapping the trailing `gcc` for `ar`.
//! - `NUTTX_PLATFORM_CFLAGS` — arch flags for the stub compile (default
//!   `-mcpu=cortex-a7 -mfloat-abi=hard -mfpu=neon-vfpv4`).
//! - `NUTTX_ARCH_INCLUDES` — space-separated include dirs relative to
//!   `NUTTX_DIR` (default `arch/arm/src/{chip,common,armv7-a}`). Same var
//!   the platform/FFI helpers read, so a board sets it once.
//! - `NUTTX_LD_SCRIPT` — board linker script relative to `NUTTX_DIR`
//!   (default `boards/arm/qemu/qemu-armv7a/scripts/dramboot.ld`).
//! - `NUTTX_VECTORTAB` — arch vector-table object relative to `NUTTX_DIR`
//!   (default `arch/arm/src/arm_vectortab.o`).
//! - `NUTTX_BOARD_LIB_DIR` — `libboard.a` dir relative to `NUTTX_DIR`
//!   (default `arch/arm/src/board`).
//!
//! Gated on `NUTTX_DIR` (env absent → plain host `cargo check` still works)
//! and on `staging/libc.a` (tree provisioned by `just nuttx build`).

use std::{
    env,
    path::{Path, PathBuf},
    process::Command,
};

/// Stage the NuttX image-link inputs and emit the propagating directives.
///
/// `builtins_stub` is the calling board crate's empty-builtins C stub (kept
/// in the board crate's `c/`, next to the board that owns the entry link).
pub fn run_image_link(builtins_stub: &Path) {
    println!("cargo:rerun-if-env-changed=NUTTX_DIR");
    println!("cargo:rerun-if-env-changed=NUTTX_CROSS");
    println!("cargo:rerun-if-env-changed=NUTTX_PLATFORM_CFLAGS");
    println!("cargo:rerun-if-env-changed=NUTTX_ARCH_INCLUDES");
    println!("cargo:rerun-if-env-changed=NUTTX_LD_SCRIPT");
    println!("cargo:rerun-if-env-changed=NUTTX_VECTORTAB");
    println!("cargo:rerun-if-env-changed=NUTTX_BOARD_LIB_DIR");

    // Strictly env-gated (NOT the nros-build-paths repo fallback): the image
    // link only makes sense inside a provisioned fixture/example build, which
    // always exports NUTTX_DIR. A host `cargo check` of a dependent Entry pkg
    // must stay link-directive-free.
    let nuttx_dir = match env::var("NUTTX_DIR") {
        Ok(dir) => PathBuf::from(dir),
        Err(_) => return,
    };

    let nuttx_cross = env::var("NUTTX_CROSS").unwrap_or_else(|_| "arm-none-eabi-gcc".to_string());
    let nuttx_ar = nuttx_cross
        .strip_suffix("gcc")
        .map(|prefix| format!("{prefix}ar"))
        .unwrap_or_else(|| "ar".to_string());
    let cflags: Vec<String> = env::var("NUTTX_PLATFORM_CFLAGS")
        .unwrap_or_else(|_| "-mcpu=cortex-a7 -mfloat-abi=hard -mfpu=neon-vfpv4".to_string())
        .split_whitespace()
        .map(String::from)
        .collect();
    let arch_includes: Vec<String> = env::var("NUTTX_ARCH_INCLUDES")
        .unwrap_or_else(|_| {
            "arch/arm/src/chip arch/arm/src/common arch/arm/src/armv7-a".to_string()
        })
        .split_whitespace()
        .map(String::from)
        .collect();
    let ld_script_rel = env::var("NUTTX_LD_SCRIPT")
        .unwrap_or_else(|_| "boards/arm/qemu/qemu-armv7a/scripts/dramboot.ld".to_string());
    // Phase-285 W4 — an EMPTY `NUTTX_VECTORTAB` means "this arch has no
    // vector-table head object" (riscv rv-virt: the reset path lives in the
    // kernel libs; only arm needs `arm_vectortab.o` at the archive head).
    let vectortab_rel =
        env::var("NUTTX_VECTORTAB").unwrap_or_else(|_| "arch/arm/src/arm_vectortab.o".to_string());
    let board_lib_rel =
        env::var("NUTTX_BOARD_LIB_DIR").unwrap_or_else(|_| "arch/arm/src/board".to_string());

    let staging = nuttx_dir.join("staging");
    let linker_script = nuttx_dir.join(&ld_script_rel);
    let vectortab = (!vectortab_rel.is_empty()).then(|| nuttx_dir.join(&vectortab_rel));
    // Phase-285 W5 — a CONFIGURED vectortab that does not exist means this
    // build is not the image-link lane for this arch (e.g. the riscv C lane
    // compiles the riscv board crate with the helper's arm DEFAULT path
    // against an rv-virt tree, and the C kernel link never consumes the boot
    // archive). Skip the image link gracefully instead of failing `ar` on a
    // missing member; the rust Entry lane sets the arch-correct env.
    if let Some(vt) = &vectortab {
        if !vt.exists() {
            println!(
                "cargo:warning=nuttx_image_link: vectortab {} absent — skipping image-link staging",
                vt.display()
            );
            return;
        }
    }
    let board_lib_dir = nuttx_dir.join(&board_lib_rel);
    println!(
        "cargo:rerun-if-changed={}",
        staging.join("libc.a").display()
    );
    if let Some(vt) = &vectortab {
        println!("cargo:rerun-if-changed={}", vt.display());
    }
    println!("cargo:rerun-if-changed={}", linker_script.display());
    println!("cargo:rerun-if-changed={}", builtins_stub.display());
    if !staging.join("libc.a").exists() {
        return;
    }

    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR set by cargo for build.rs"));

    // Shared -isystem/-I set for the preprocess + stub compile (both need
    // <nuttx/config.h> and the arch headers, like the NuttX build itself).
    let include_args = |cmd: &mut Command| {
        cmd.arg(format!("-isystem{}", nuttx_dir.join("include").display()));
        for inc in &arch_includes {
            cmd.arg(format!("-I{}", nuttx_dir.join(inc).display()));
        }
        cmd.arg(format!("-I{}", nuttx_dir.join("sched").display()));
    };

    // (1) Preprocess the flat-build linker script → OUT_DIR/<name>. The
    // Entry config's `-T<name>` resolves it by name through the propagated
    // `-L OUT_DIR` (GNU ld searches -L dirs for -T scripts).
    let script_name = linker_script
        .file_name()
        .expect("linker script path has a file name");
    let processed_ld = out_dir.join(script_name);
    let mut preprocess = Command::new(&nuttx_cross);
    preprocess.args(["-E", "-P", "-x", "c", "-D__NuttX__", "-D__KERNEL__"]);
    include_args(&mut preprocess);
    preprocess.arg(&linker_script).arg("-o").arg(&processed_ld);
    let status = preprocess.status().unwrap_or_else(|e| {
        panic!("failed to preprocess NuttX linker script ({nuttx_cross}): {e}")
    });
    assert!(status.success(), "NuttX linker script preprocessing failed");

    // (2) Empty builtins table — see the stub's header.
    let stub_obj = out_dir.join("nuttx_builtins_stub.o");
    let mut stub_cc = Command::new(&nuttx_cross);
    stub_cc.arg("-c");
    for f in &cflags {
        stub_cc.arg(f);
    }
    stub_cc.args(["-std=c11", "-D__NuttX__", "-D__KERNEL__"]);
    include_args(&mut stub_cc);
    stub_cc.arg(builtins_stub).arg("-o").arg(&stub_obj);
    let stub_status = stub_cc
        .status()
        .unwrap_or_else(|e| panic!("failed to compile NuttX builtins stub ({nuttx_cross}): {e}"));
    assert!(stub_status.success(), "NuttX builtins stub compile failed");

    // (3) Boot archive: vectortab (reset path head object) + builtins stub.
    let boot_lib = out_dir.join("libnros_nuttx_boot.a");
    let _ = std::fs::remove_file(&boot_lib);
    let mut ar_cmd = Command::new(&nuttx_ar);
    ar_cmd.arg("crs").arg(&boot_lib);
    if let Some(vt) = &vectortab {
        ar_cmd.arg(vt);
    }
    let ar_status = ar_cmd
        .arg(&stub_obj)
        .status()
        .unwrap_or_else(|e| panic!("failed to archive libnros_nuttx_boot.a ({nuttx_ar}): {e}"));
    assert!(ar_status.success(), "libnros_nuttx_boot.a archive failed");

    // (4) The propagating directives. `-bundle` keeps the archive standalone
    // in OUT_DIR (bundling into the rlib is incompatible with
    // `+whole-archive` — same constraint `run_platform` documents,
    // issue-0048) and puts the `-l` at the FINAL binary link, BEFORE the
    // `.cargo/config.toml` trailing `-C link-arg` kernel group — so the
    // builtins stub preempts `-lapps`' contaminated `builtin_list.o`, and
    // the board rlib's `nsh_main` (earlier still, in the rlib list) preempts
    // NSH's.
    println!("cargo:rustc-link-search=native={}", out_dir.display());
    println!("cargo:rustc-link-search=native={}", staging.display());
    println!("cargo:rustc-link-search=native={}", board_lib_dir.display());
    println!("cargo:rustc-link-lib=static:-bundle,+whole-archive=nros_nuttx_boot");
}
