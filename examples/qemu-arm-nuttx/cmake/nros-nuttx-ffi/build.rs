use std::env;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    // APP_MAIN_CPP: path to the C or C++ source file to compile (set by CMake)
    // APP_INCLUDE_DIRS: semicolon-separated include directories (set by CMake)
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let nros_root = manifest_dir.join("../../../..");

    let main_src = env::var("APP_MAIN_CPP").unwrap_or_else(|_| {
        panic!(
            "APP_MAIN_CPP not set. Set it to the path of the C/C++ source file.\n\
             Example: APP_MAIN_CPP=examples/qemu-arm-nuttx/c/zenoh/talker/src/main.c"
        )
    });

    let is_cpp = main_src.ends_with(".cpp") || main_src.ends_with(".cxx") || main_src.ends_with(".cc");

    let mut build = cc::Build::new();
    build
        .cpp(is_cpp)
        .file(&main_src)
        .flag("-ffunction-sections")
        .flag("-fdata-sections")
        .warnings(false);

    if is_cpp {
        let nros_cpp_include = nros_root.join("packages/core/nros-cpp/include");
        build.include(&nros_cpp_include);
        build.flag("-std=c++14");
    } else {
        let nros_c_include = nros_root.join("packages/core/nros-c/include");
        build.include(&nros_c_include);
    }

    // Additional include directories from CMake (semicolon-separated)
    if let Ok(include_dirs) = env::var("APP_INCLUDE_DIRS") {
        for dir in include_dirs.split(';') {
            if !dir.is_empty() {
                build.include(dir);
            }
        }
    }

    // Compile definitions from CMake (semicolon-separated, e.g. from config.toml)
    if let Ok(compile_defs) = env::var("APP_COMPILE_DEFS") {
        for def in compile_defs.split(';') {
            if !def.is_empty() {
                build.define(def.split('=').next().unwrap_or(def),
                    def.split('=').nth(1));
            }
        }
    }

    build.compile("app");

    // ---- NuttX kernel link args ----
    // The binary IS the NuttX kernel. Link against all NuttX staging libraries,
    // linker script, and startup objects.
    println!("cargo:rerun-if-env-changed=NUTTX_DIR");
    let nuttx_dir = match env::var("NUTTX_DIR") {
        Ok(dir) => PathBuf::from(dir),
        Err(_) => return,
    };

    let staging = nuttx_dir.join("staging");
    if !staging.join("libc.a").exists() {
        return;
    }

    // Preprocess linker script
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let processed_ld = out_dir.join("dramboot.ld");
    let linker_script = nuttx_dir.join("boards/arm/qemu/qemu-armv7a/scripts/dramboot.ld");

    let status = Command::new("arm-none-eabi-gcc")
        .args(["-E", "-P", "-x", "c",
            &format!("-isystem{}", nuttx_dir.join("include").display()),
            "-D__NuttX__", "-D__KERNEL__",
            &format!("-I{}", nuttx_dir.join("arch/arm/src/chip").display()),
            &format!("-I{}", nuttx_dir.join("arch/arm/src/common").display()),
            &format!("-I{}", nuttx_dir.join("arch/arm/src/armv7-a").display()),
            &format!("-I{}", nuttx_dir.join("sched").display()),
        ])
        .arg(&linker_script)
        .arg("-o").arg(&processed_ld)
        .status().expect("failed to preprocess linker script");
    assert!(status.success(), "linker script preprocessing failed");

    let board_src = nuttx_dir.join("arch/arm/src/board");
    let vectortab = nuttx_dir.join("arch/arm/src/arm_vectortab.o");

    // Find libgcc.a
    let gcc_out = Command::new("arm-none-eabi-gcc")
        .args(["-mcpu=cortex-a7", "-mfloat-abi=hard", "-mfpu=neon-vfpv4", "-print-libgcc-file-name"])
        .output().expect("failed to find libgcc");
    let libgcc = String::from_utf8(gcc_out.stdout).unwrap().trim().to_string();

    // NuttX flat-build: the binary IS the kernel
    println!("cargo:rustc-link-arg=-T{}", processed_ld.display());
    println!("cargo:rustc-link-arg=--entry=__start");
    println!("cargo:rustc-link-arg=-nostartfiles");
    println!("cargo:rustc-link-arg=-nodefaultlibs");
    println!("cargo:rustc-link-arg={}", vectortab.display());
    println!("cargo:rustc-link-arg=-L{}", staging.display());
    println!("cargo:rustc-link-arg=-L{}", board_src.display());
    println!("cargo:rustc-link-arg=-Wl,--start-group");
    for lib in ["sched", "drivers", "boards", "c", "mm", "arch", "xx",
                "apps", "net", "crypto", "fs", "binfmt", "openamp", "board"] {
        println!("cargo:rustc-link-arg=-l{lib}");
    }
    println!("cargo:rustc-link-arg={libgcc}");
    println!("cargo:rustc-link-arg=-Wl,--end-group");

    println!("cargo:rerun-if-changed={}", main_src);
    println!("cargo:rerun-if-changed={}", linker_script.display());
    println!("cargo:rerun-if-env-changed=APP_MAIN_CPP");
    println!("cargo:rerun-if-env-changed=APP_INCLUDE_DIRS");
    println!("cargo:rerun-if-env-changed=APP_COMPILE_DEFS");
}
