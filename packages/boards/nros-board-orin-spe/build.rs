//! Build script for `nros-board-orin-spe`.
//!
//! Resolves NVIDIA's FSP install path from `NV_SPE_FSP_DIR` and emits the
//! link search + library directives so the final firmware can resolve
//! `tegra_ivc_channel_*` (consumed by `nvidia-ivc/fsp`),
//! `pvPortMalloc` / `xTaskCreate` / `vTaskDelay` (consumed by
//! `nros-platform-freertos`), and `printf` (consumed by `println!`).
//!
//! All linkage is `dylib`-flavoured `static` archives — the FSP ships
//! prebuilt `.a` files; we don't recompile FreeRTOS from source on the
//! SPE.
//!
//! # Required env vars
//!
//! - `NV_SPE_FSP_DIR` — directory containing `lib/libtegra_aon_fsp.a`
//!   (FreeRTOS V10.4.3 + IVC + HSP), `lib/libnewlib.a` (newlib stubs)
//!   and `include/` (FSP headers consumed by `nvidia-ivc/fsp`).
//!
//! Without it, `cargo build --features fsp` fails fast with the
//! diagnostic the panic below produces. `--features unix-mock` builds
//! cleanly without the env var (POSIX dev path).

fn main() {
    println!("cargo:rerun-if-env-changed=NV_SPE_FSP_DIR");

    let fsp = std::env::var("CARGO_FEATURE_FSP").is_ok();
    let unix_mock = std::env::var("CARGO_FEATURE_UNIX_MOCK").is_ok();

    if fsp && unix_mock {
        // The Cargo.toml features section makes these mutually exclusive
        // by convention; this is the build-time enforcement.
        panic!(
            "nros-board-orin-spe: features `fsp` and `unix-mock` are \
             mutually exclusive — pick one"
        );
    }

    if !fsp {
        // `unix-mock` (or no backend selected for `cargo doc`): nothing
        // to link against.
        return;
    }

    let dir = std::env::var("NV_SPE_FSP_DIR").unwrap_or_else(|_| {
        panic!(
            "nros-board-orin-spe: feature `fsp` requires NV_SPE_FSP_DIR to \
             point at an installed NVIDIA Orin SPE FSP tree. Typical \
             SDK Manager layout: $NV_SPE_FSP_DIR/lib/libtegra_aon_fsp.a + \
             $NV_SPE_FSP_DIR/include/. See README.md for SDK setup."
        )
    });

    // Compile `c/printf_shim.c` with the same softfp ABI the FSP uses
    // (`-mfloat-abi=softfp -mfpu=vfpv3-d16`) so the resulting object is
    // link-compatible with both the FSP `.a` and the Rust soft-float
    // (`armv7r-none-eabi`) artefacts. The shim provides a `vsnprintf`
    // override that delegates to newlib's integer-only `vsniprintf`,
    // dropping the ~25 KB BTCM cost of the float-aware formatter chain
    // (`_dtoa_r`, `fmaf128`, `__divtf3`, …) that BSP `platform/print.c`
    // would otherwise pull through `printf("%d %s\r\n", …)` call sites.
    println!("cargo:rerun-if-changed=c/printf_shim.c");
    cc::Build::new()
        .file("c/printf_shim.c")
        .flag("-march=armv7-r")
        .flag("-mcpu=cortex-r5")
        .flag("-mfpu=vfpv3-d16")
        .flag("-mfloat-abi=softfp")
        .flag("-Os")
        .flag("-ffunction-sections")
        .flag("-fdata-sections")
        .compile("nros_orin_spe_printf_shim");

    println!("cargo:rustc-link-search=native={}/lib", dir);

    // FSP archive — provides FreeRTOS V10.4.3, tegra_ivc_channel_*, HSP
    // doorbell, and TCU printf.
    println!("cargo:rustc-link-lib=static=tegra_aon_fsp");

    // newlib stubs — sprintf, memcpy, etc. The FSP ships its own copy
    // because the SPE has no system libc. If the user's SDK ships a
    // different name (e.g. picolibc), they can override via
    // `NV_SPE_FSP_LIBC` → `=NV_SPE_FSP_LIBC`-driven extra link.
    if let Ok(libc_name) = std::env::var("NV_SPE_FSP_LIBC") {
        println!("cargo:rustc-link-lib=static={}", libc_name);
    } else {
        // Default to `newlib` (FSP ships `libnewlib.a`). Harmless if the
        // archive doesn't exist — the link will fail with a missing-
        // library error pointing at the right path.
        println!("cargo:rustc-link-lib=static=newlib");
    }

    // Surface the include path for downstream firmware crates that
    // need to `extern "C"` declare FSP functions directly. The board
    // crate itself doesn't include any FSP headers — it only relies
    // on the link-time symbol resolution.
    println!("cargo:fsp-include={}/include", dir);
}
