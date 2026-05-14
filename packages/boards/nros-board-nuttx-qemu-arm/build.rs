use std::{env, path::PathBuf};

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let workspace_root = manifest_dir
        .parent()
        .and_then(|p| p.parent())
        .and_then(|p| p.parent())
        .expect("could not resolve workspace root");

    let nuttx_dir = env::var("NUTTX_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| workspace_root.join("third-party/nuttx/nuttx"));
    if !nuttx_dir.join("include").exists() {
        return;
    }

    let cffi_include = workspace_root.join("packages/core/nros-platform-cffi/include");
    let platform_src = workspace_root.join("packages/core/nros-platform-posix/src");

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
    platform.compile("nros_platform_nuttx");

    println!("cargo:rustc-link-lib=static=nros_platform_nuttx");
    println!("cargo:rerun-if-changed={}", platform_src.display());
    println!("cargo:rerun-if-env-changed=NUTTX_DIR");
}
