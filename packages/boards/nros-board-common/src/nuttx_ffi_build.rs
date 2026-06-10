use std::{env, path::PathBuf, process::Command};

/// 194.3c.1 — back-compat shim. The arm board's FFI crate calls
/// `run_qemu_arm()`; it now forwards to the arch-generic `run_nuttx()`,
/// whose arm defaults reproduce the pre-194.3c behaviour byte-for-byte.
pub fn run_qemu_arm() {
    run_nuttx();
}

/// Arch-generic NuttX FFI build (194.3c.1). All arch-specifics come from
/// `NUTTX_*` env (the board overlay sets them); the defaults are the
/// qemu-arm cortex-a7 hardfloat values, so a build with no overrides is
/// identical to the old `run_qemu_arm`. A new-arch NuttX board (e.g. riscv)
/// supplies its own `NUTTX_CROSS` / `NUTTX_ARCH` / `NUTTX_ARCH_CFLAGS` /
/// `NUTTX_LIBGCC_FLAGS` / `NUTTX_VECTORTAB_OBJ` / `NUTTX_LINKER_SCRIPT` /
/// `NUTTX_ARCH_INCLUDES`.
pub fn run_nuttx() {
    // APP_MAIN_CPP: path to the C or C++ source file to compile (set by CMake)
    // APP_INCLUDE_DIRS: semicolon-separated include directories (set by CMake)
    // Phase 208.B Track A — paths come from `nros-build-paths`
    // (walks up from CARGO_MANIFEST_DIR to `nros-sdk-index.toml`);
    // env vars stay valid as out-of-tree overrides. The helper also
    // emits the matching `cargo:rerun-if-env-changed` directives.
    let nros_c_include = nros_build_paths::nros_c_include();
    let nros_cpp_include = nros_build_paths::nros_cpp_include();

    // 194.2: cross-compiler + arch cflags are per-board (the board overlay / env
    // sets them); defaults = the qemu-arm cortex-a7 hardfloat values so the
    // existing board is unchanged. A new-arch NuttX board overrides these.
    let nuttx_cross = env::var("NUTTX_CROSS").unwrap_or_else(|_| "arm-none-eabi-gcc".to_string());
    let arch_cflags: Vec<String> = env::var("NUTTX_ARCH_CFLAGS")
        .unwrap_or_else(|_| "-mcpu=cortex-a7 -mfloat-abi=hard -mfpu=vfpv3-d16".to_string())
        .split_whitespace()
        .map(String::from)
        .collect();
    // The libgcc multilib probe keeps its own flag set: on qemu-arm it
    // deliberately differs from the compile flags (neon-vfpv4 selects
    // `v7ve+simd/hard`, vfpv3-d16 selects `v7-a+fp/hard` — different libgcc.a),
    // and that is the variant the linked closure expects. Per-board override.
    let libgcc_flags: Vec<String> = env::var("NUTTX_LIBGCC_FLAGS")
        .unwrap_or_else(|_| "-mcpu=cortex-a7 -mfloat-abi=hard -mfpu=neon-vfpv4".to_string())
        .split_whitespace()
        .map(String::from)
        .collect();
    // 194.3: NuttX flat-build link internals live under `arch/<arch>/src`; the
    // vector-table object is arch-specific (ARM's `arm_vectortab.o`; arches
    // without one set NUTTX_VECTORTAB_OBJ=""). Defaults = qemu-arm.
    let nuttx_arch = env::var("NUTTX_ARCH").unwrap_or_else(|_| "arm".to_string());
    let vectortab_obj =
        env::var("NUTTX_VECTORTAB_OBJ").unwrap_or_else(|_| "arm_vectortab.o".to_string());
    // 194.3c.1: the flat-build linker script lives under the board's NuttX
    // tree (`boards/<arch>/<chip>/<board>/scripts/<name>.ld`) and the
    // linker-script preprocessor needs the arch's source include dirs. Both
    // were arm-hardcoded before 194.3c; now per-board via env (defaults =
    // qemu-arm). `NUTTX_ARCH_INCLUDES` is a space-separated list of dirs
    // relative to `NUTTX_DIR` (the arm default carries the armv7-a family dir
    // that has no `arch/<arch>/{chip,common}` analogue).
    let linker_script_rel = env::var("NUTTX_LINKER_SCRIPT")
        .unwrap_or_else(|_| "boards/arm/qemu/qemu-armv7a/scripts/dramboot.ld".to_string());
    let arch_includes: Vec<String> = env::var("NUTTX_ARCH_INCLUDES")
        .unwrap_or_else(|_| "arch/arm/src/chip arch/arm/src/common arch/arm/src/armv7-a".to_string())
        .split_whitespace()
        .map(String::from)
        .collect();
    println!("cargo:rerun-if-env-changed=NUTTX_CROSS");
    println!("cargo:rerun-if-env-changed=NUTTX_ARCH_CFLAGS");
    println!("cargo:rerun-if-env-changed=NUTTX_LIBGCC_FLAGS");
    println!("cargo:rerun-if-env-changed=NUTTX_ARCH");
    println!("cargo:rerun-if-env-changed=NUTTX_VECTORTAB_OBJ");
    println!("cargo:rerun-if-env-changed=NUTTX_LINKER_SCRIPT");
    println!("cargo:rerun-if-env-changed=NUTTX_ARCH_INCLUDES");

    let main_src = env::var("APP_MAIN_CPP").unwrap_or_else(|_| {
        panic!(
            "APP_MAIN_CPP not set. Set it to the path of the C/C++ source file.\n\
             Example: APP_MAIN_CPP=examples/qemu-arm-nuttx/c/zenoh/talker/src/main.c"
        )
    });

    let is_cpp =
        main_src.ends_with(".cpp") || main_src.ends_with(".cxx") || main_src.ends_with(".cc");

    let mut build = cc::Build::new();
    build
        .cpp(is_cpp)
        .file(&main_src)
        .flag("-ffunction-sections")
        .flag("-fdata-sections")
        .define("NROS_PLATFORM_NUTTX", None)
        .warnings(false);
    // The arch ABI flags must match the NuttX export + Rust closure (e.g. the
    // qemu-arm default `-mcpu=cortex-a7 -mfloat-abi=hard -mfpu=vfpv3-d16` is
    // hardfloat — cc-rs's softfloat default for `-march=armv7-a` would trip ld
    // with `uses VFP register arguments`). Per-board via NUTTX_ARCH_CFLAGS.
    for f in &arch_cflags {
        build.flag(f);
    }

    if is_cpp {
        // The NuttX flat-build kernel ELF is statically linked and has no
        // dynamic linker / GOT-init startup. cc-rs defaults to `-fPIC`,
        // which causes g++ to emit `R_ARM_GOT_BREL` relocations for
        // COMDAT statics (e.g. `Node::GlobalStorageHolder<>::storage`).
        // The static linker leaves those GOT slots zero in a `-static`
        // binary, so accessors return 0 at runtime and `nros::init`
        // fails with INVALID_ARGUMENT. Disable PIC for C++ only — the
        // C examples don't have COMDAT statics and rely on cc-rs's
        // default PIC for their NuttX kernel-symbol references.
        build.pic(false);
    }

    // Phase 156 (NuttX, supersedes 155.B.5) — generated per-build header paths MUST come
    // first so they shadow the source-tree `#error` stubs at
    // `packages/core/nros-{c,cpp}/include/nros/nros_{,cpp_}config_generated.h`.
    //
    // nros-c / nros-cpp `build.rs` each emit their
    // `nros_{,cpp_}config_generated.h` under
    // `$CARGO_TARGET_DIR/nros-{c,cpp}-generated/nros/`. Cmake also
    // mirrors the C header into
    // `<build_dir>/nano_ros/packages/core/nros-c/include/nros/...`
    // (passed via APP_INCLUDE_DIRS_FILE), but the cpp variant is
    // not mirrored — the cargo-target path is the only place it
    // lives. Add both up front; the APP_INCLUDE_DIRS_FILE and the
    // source-tree fallback come after so they cannot win.
    if let Ok(target_dir) = env::var("CARGO_TARGET_DIR") {
        let td = PathBuf::from(target_dir);
        build.include(td.join("nros-c-generated"));
        if is_cpp {
            build.include(td.join("nros-cpp-generated"));
        }
    }

    // APP_INCLUDE_DIRS_FILE from cmake lists per-example codegen
    // paths (`build/nano_ros_c/std_msgs`, `build/nano_ros_cpp/...`),
    // the per-build mirror of nros-c/nros-cpp headers
    // (`build/nano_ros/packages/core/nros-{c,cpp}/include`), AND
    // the in-tree source-tree fallbacks
    // (`packages/core/nros-{c,cpp}/include`). cmake puts the
    // source-tree path first in the list — but those source-tree
    // paths hold `#error` stubs of nros_{,cpp_}config_generated.h.
    // If applied verbatim, the source-tree stub wins over the
    // per-build mirror that has the real header.
    //
    // Filter the source-tree nros-{c,cpp}/include paths out of
    // the first pass and re-add them at the end as a last-resort
    // fallback. Other entries (codegen + per-build mirrors) keep
    // their original order.
    let nros_c_src = nros_c_include.clone();
    let nros_cpp_src = nros_cpp_include.clone();
    let is_src_tree_stub = |dir: &str| -> bool {
        let p = PathBuf::from(dir);
        let canon = p.canonicalize().unwrap_or(p);
        canon == nros_c_src.canonicalize().unwrap_or(nros_c_src.clone())
            || canon == nros_cpp_src.canonicalize().unwrap_or(nros_cpp_src.clone())
    };
    let mut deferred_src_tree: Vec<String> = Vec::new();
    if let Ok(includes_file) = env::var("APP_INCLUDE_DIRS_FILE") {
        match std::fs::read_to_string(&includes_file) {
            Ok(contents) => {
                for line in contents.lines() {
                    let dir = line.trim();
                    if dir.is_empty() {
                        continue;
                    }
                    if is_src_tree_stub(dir) {
                        // Defer to the end so the per-build mirror at
                        // `build/nano_ros/packages/core/nros-{c,cpp}/include`
                        // (which has the real `nros_{,cpp_}config_generated.h`)
                        // is searched first, but source-tree headers like
                        // `nros/app_main.h` are still found.
                        deferred_src_tree.push(dir.to_string());
                    } else {
                        build.include(dir);
                    }
                }
                for dir in &deferred_src_tree {
                    build.include(dir);
                }
            }
            Err(e) => panic!("APP_INCLUDE_DIRS_FILE={includes_file} not readable: {e}"),
        }
    }

    if is_cpp {
        build.include(&nros_cpp_include);
        build.flag("-std=c++14");
    } else {
        build.include(&nros_c_include);
    }

    // Additional include directories. Two paths:
    //   * `APP_INCLUDE_DIRS` (semicolon-separated env var) — legacy
    //     callers that build the include list directly in cmake.
    //   * `APP_INCLUDE_DIRS_FILE` (newline-separated file) — the
    //     `nuttx_build_example(LINK_INTERFACES …)` path, which
    //     materialises the cmake link-graph closure via
    //     `file(GENERATE)`. File-based passing avoids the
    //     `cmake -E env` ambiguity around `;` (it's both list-sep
    //     and a valid path char).
    if let Ok(include_dirs) = env::var("APP_INCLUDE_DIRS") {
        for dir in include_dirs.split(';') {
            if !dir.is_empty() {
                build.include(dir);
            }
        }
    }
    // (Note: APP_INCLUDE_DIRS_FILE applied above before the source-tree
    // includes, to make the per-build generated header win.)

    // Source-tree fallbacks (lowest priority — stub headers may
    // live here, must come AFTER any caller mirror dirs).
    if is_cpp {
        build.include(&nros_cpp_include);
        build.flag("-std=c++14");
    } else {
        build.include(&nros_c_include);
    }

    // Extra source files from CMake (semicolon-separated, e.g. generated interface .c files)
    if let Ok(extra_sources) = env::var("APP_EXTRA_SOURCES") {
        for src in extra_sources.split(';') {
            if !src.is_empty() {
                build.file(src);
            }
        }
    }

    // Compile definitions from CMake (semicolon-separated, e.g. from config.toml)
    if let Ok(compile_defs) = env::var("APP_COMPILE_DEFS") {
        for def in compile_defs.split(';') {
            if !def.is_empty() {
                build.define(def.split('=').next().unwrap_or(def), def.split('=').nth(1));
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

    // Preprocess linker script (194.3c.1: script path + arch include dirs +
    // the preprocessor itself are per-board via env; the cross-compiler is
    // `nuttx_cross`, not a hardcoded `arm-none-eabi-gcc`).
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let processed_ld = out_dir.join("dramboot.ld");
    let linker_script = nuttx_dir.join(&linker_script_rel);

    let mut pp_args: Vec<String> = vec![
        "-E".into(),
        "-P".into(),
        "-x".into(),
        "c".into(),
        format!("-isystem{}", nuttx_dir.join("include").display()),
        "-D__NuttX__".into(),
        "-D__KERNEL__".into(),
    ];
    for inc in &arch_includes {
        pp_args.push(format!("-I{}", nuttx_dir.join(inc).display()));
    }
    pp_args.push(format!("-I{}", nuttx_dir.join("sched").display()));

    let status = Command::new(&nuttx_cross)
        .args(&pp_args)
        .arg(&linker_script)
        .arg("-o")
        .arg(&processed_ld)
        .status()
        .expect("failed to preprocess linker script");
    assert!(status.success(), "linker script preprocessing failed");

    let arch_src = nuttx_dir.join("arch").join(&nuttx_arch).join("src");
    let board_src = arch_src.join("board");

    // Find libgcc.a — 194.2: per-board cross-compiler + libgcc-probe flags
    // (defaults = qemu-arm's neon-vfpv4 → v7ve+simd/hard, unchanged).
    let gcc_out = Command::new(&nuttx_cross)
        .args(&libgcc_flags)
        .arg("-print-libgcc-file-name")
        .output()
        .expect("failed to find libgcc");
    let libgcc = String::from_utf8(gcc_out.stdout)
        .unwrap()
        .trim()
        .to_string();

    // NuttX flat-build: the binary IS the kernel
    println!("cargo:rustc-link-arg=-T{}", processed_ld.display());
    println!("cargo:rustc-link-arg=--entry=__start");
    println!("cargo:rustc-link-arg=-nostartfiles");
    println!("cargo:rustc-link-arg=-nodefaultlibs");
    if !vectortab_obj.is_empty() {
        println!(
            "cargo:rustc-link-arg={}",
            arch_src.join(&vectortab_obj).display()
        );
    }
    println!("cargo:rustc-link-arg=-L{}", staging.display());
    println!("cargo:rustc-link-arg=-L{}", board_src.display());
    println!("cargo:rustc-link-arg=-Wl,--start-group");
    for lib in [
        "sched", "drivers", "boards", "c", "mm", "arch", "xx", "apps", "net", "crypto", "fs",
        "binfmt", "openamp", "board",
    ] {
        println!("cargo:rustc-link-arg=-l{lib}");
    }
    println!("cargo:rustc-link-arg={libgcc}");
    println!("cargo:rustc-link-arg=-Wl,--end-group");

    println!("cargo:rerun-if-changed={}", main_src);
    println!("cargo:rerun-if-changed={}", linker_script.display());
    println!("cargo:rerun-if-env-changed=APP_MAIN_CPP");
    println!("cargo:rerun-if-env-changed=APP_INCLUDE_DIRS");
    println!("cargo:rerun-if-env-changed=APP_INCLUDE_DIRS_FILE");
    if let Ok(includes_file) = env::var("APP_INCLUDE_DIRS_FILE") {
        println!("cargo:rerun-if-changed={includes_file}");
    }
    println!("cargo:rerun-if-env-changed=APP_FFI_LIBS_FILE");
    if let Ok(ffi_libs_file) = env::var("APP_FFI_LIBS_FILE") {
        println!("cargo:rerun-if-changed={ffi_libs_file}");
        // Each line is an absolute path to a `lib<name>.a` static lib.
        // Forward to rustc as a link search dir + a -l static link.
        // Avoids `undefined reference to nros_cpp_serialize_…` from the
        // Rust FFI glue that the `<pkg>__nano_ros_cpp` interface library
        // would normally drag in via cmake's regular link graph.
        match std::fs::read_to_string(&ffi_libs_file) {
            Ok(contents) => {
                for line in contents.lines() {
                    let path = line.trim();
                    if path.is_empty() {
                        continue;
                    }
                    let lib_path = std::path::Path::new(path);
                    let dir = lib_path
                        .parent()
                        .unwrap_or_else(|| std::path::Path::new("."));
                    let stem = lib_path
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .and_then(|s| s.strip_prefix("lib"))
                        .unwrap_or_else(|| {
                            panic!("FFI lib path {path} has no `lib<name>.a` shape")
                        });
                    println!("cargo:rustc-link-search=native={}", dir.display());
                    println!("cargo:rustc-link-lib=static={stem}");
                }
            }
            Err(e) => panic!("APP_FFI_LIBS_FILE={ffi_libs_file} not readable: {e}"),
        }
    }
    println!("cargo:rerun-if-env-changed=APP_EXTRA_SOURCES");
    println!("cargo:rerun-if-env-changed=APP_COMPILE_DEFS");
}
