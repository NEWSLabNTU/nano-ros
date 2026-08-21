//! Build script for nros-board-s32z270-freertos (phase-372 W2).
//!
//! Per-board wiring only, mirroring nros-board-mps2-an385-freertos: linker
//! scripts into `OUT_DIR`, the board's own C translation unit, the
//! `NROS_APP_CONFIG` symbol, and libc/libgcc discovery. Everything generic
//! (cflag resolution via the `[arch.cortex-r52]` profile, FreeRTOS/lwIP
//! include dirs, the `NROS_APP_CONFIG` emitter) comes from
//! `nros_board_common::freertos_build`; the FreeRTOS kernel, lwIP,
//! `nros-platform-freertos` and the generic C glue are compiled by
//! `nros-board-freertos/build.rs` and propagate transitively.
//!
//! Kernel + port provisioning (phase-372 W3, the licensing seam):
//! `FREERTOS_DIR` / `FREERTOS_PORT` env vars override the defaults. The
//! default port here is the in-tree kernel's `GCC/ARM_CRx_No_GIC` so a clean
//! checkout is LINK-COMPLETE; a hardware consumer points `FREERTOS_DIR` at
//! the NXP FreeRTOS distribution and `FREERTOS_PORT` at `GCC/ARM_CR52_GIC`
//! (with the Thumb-resume CPSR patch applied — see phase-372).

use std::{
    env, fs,
    path::{Path, PathBuf},
};

use nros_board_common::{
    BaseConfig, FreertosScheduling,
    freertos_build::{app_stack_bytes_from_build_env, configure_cflags, emit_app_config_tu},
};

fn main() {
    // issue 0288 — skip the ARM cross-compile when host tooling builds this
    // crate (the source-metadata probe).
    if nros_board_common::host_probe::skip_cross_build(
        "nros-board-s32z270-freertos",
        &["thumb", "arm"],
    ) {
        return;
    }

    let out_dir = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let config_dir = manifest_dir.join("config");
    let shared_config_dir = manifest_dir
        .parent()
        .expect("workspace layout")
        .join("nros-board-freertos/config");

    // --- Linker scripts ---
    // The board script `INCLUDE`s the shared section layout; both land in
    // OUT_DIR on the linker search path. The binary's cargo_config names
    // `-Ts32z270_rtu.ld` in rustflags.
    for (src, name) in [
        (config_dir.join("s32z270_rtu.ld"), "s32z270_rtu.ld"),
        (
            shared_config_dir.join("nros-freertos-cortex-m.ld"),
            "nros-freertos-cortex-m.ld",
        ),
    ] {
        fs::copy(&src, out_dir.join(name))
            .unwrap_or_else(|e| panic!("copying {} into OUT_DIR: {e}", src.display()));
        println!("cargo:rerun-if-changed={}", src.display());
    }
    println!("cargo:rustc-link-search={}", out_dir.display());

    // --- Environment ---
    let freertos_dir = nros_build_paths::freertos_dir();
    let freertos_port =
        env::var("FREERTOS_PORT").unwrap_or_else(|_| "GCC/ARM_CRx_No_GIC".to_string());
    let lwip_dir = nros_build_paths::lwip_dir();
    let freertos_config_dir = env::var("FREERTOS_CONFIG_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| config_dir.clone());
    let port_dir = freertos_dir.join("portable").join(&freertos_port);

    // --- Board C: startup + weak netif/tick hooks ---
    let mut glue = cc::Build::new();
    configure_cflags(&mut glue);
    nros_board_common::freertos_build::add_freertos_includes(
        &mut glue,
        &freertos_dir,
        &port_dir,
        &freertos_config_dir,
    );
    nros_board_common::freertos_build::add_lwip_includes(&mut glue, &lwip_dir);
    let nros_c_include = manifest_dir
        .parent()
        .and_then(|p| p.parent())
        .expect("workspace layout")
        .join("api/nros-c/include");
    assert!(
        nros_c_include.join("nros/app_config.h").exists(),
        "nros-c header not at {} — did nros-c move again? (issue 0365)",
        nros_c_include.display()
    );
    glue.include(nros_c_include);
    glue.file(manifest_dir.join("c/board_s32z270.c"));

    let sched = FreertosScheduling {
        app_stack_bytes: app_stack_bytes_from_build_env(),
        ..FreertosScheduling::default()
    };
    glue.file(emit_app_config_tu(&out_dir, &BaseConfig::default(), &sched));

    // issue 0478 — cc-rs would hand arm-none-eabi-gcc the clang-only
    // `-mno-omit-leaf-frame-pointer`, which gcc REJECTS.
    nros_cc_flags::gcc_safe_frame_pointer(&mut glue);
    glue.compile("startup");

    println!("cargo:rustc-link-lib=static=startup");

    // --- Newlib (libc + nosys stubs) — multilib-correct discovery ---
    let libc_path = gcc_print_file("libc.a");
    let libc_dir = Path::new(&libc_path).parent().unwrap();
    println!("cargo:rustc-link-search={}", libc_dir.display());
    let libgcc_path = gcc_print_file("libgcc.a");
    let libgcc_dir = Path::new(&libgcc_path).parent().unwrap();
    println!("cargo:rustc-link-search={}", libgcc_dir.display());
    println!("cargo:rustc-link-lib=static=c");
    println!("cargo:rustc-link-lib=static=nosys");
    println!("cargo:rustc-link-lib=static=gcc");

    // --- Rerun triggers ---
    println!("cargo:rerun-if-changed=config/FreeRTOSConfig.h");
    println!("cargo:rerun-if-changed=config/lwipopts.h");
    println!("cargo:rerun-if-changed=config/arch/cc.h");
    println!("cargo:rerun-if-changed=c/board_s32z270.c");
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=FREERTOS_PORT");
    println!("cargo:rerun-if-env-changed=FREERTOS_CFLAGS");
    nros_build_paths::watch_path(&freertos_config_dir);
}

fn gcc_print_file(name: &str) -> String {
    let out = std::process::Command::new("arm-none-eabi-gcc")
        .args([
            "-mcpu=cortex-r52",
            "-mfpu=neon-fp-armv8",
            "-mfloat-abi=hard",
            &format!("--print-file-name={name}"),
        ])
        .output()
        .expect("arm-none-eabi-gcc not found");
    let path = String::from_utf8(out.stdout).unwrap();
    let path = path.trim().to_string();
    assert!(
        Path::new(&path).is_absolute(),
        "arm-none-eabi-gcc could not locate {name} for the R52 multilib"
    );
    path
}
