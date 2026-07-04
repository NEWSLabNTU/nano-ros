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
        .unwrap_or_else(|_| {
            "arch/arm/src/chip arch/arm/src/common arch/arm/src/armv7-a".to_string()
        })
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

    // Phase 238.C — mixed C/C++ app build. A NuttX C example registers a
    // declarative C node (`Talker.c`, C-linkage `__nros_component_<pkg>_register`)
    // but is driven by the header-only C++ `EntryNodeRuntime` (the generated
    // entry is a `.cpp`). cc-rs compiles every file in one `cc::Build` with a
    // single language, so a `.c` extra under a `.cpp` main would be forced to
    // C++ — mangling the C node's register symbol. Compile `.c` sources in a
    // separate C `cc::Build` (and `.cpp/.cc/.cxx` in a C++ one), so each source
    // keeps its native linkage. The two archives both link into the kernel ELF.
    let is_cxx_ext = |p: &str| p.ends_with(".cpp") || p.ends_with(".cxx") || p.ends_with(".cc");

    // Resolve the source-tree stub dirs so the APP_INCLUDE_DIRS_FILE pass can
    // defer them (they hold `#error` stubs of nros_{,cpp_}config_generated.h;
    // the per-build mirror must win). Same logic as pre-238.C.
    let nros_c_src = nros_c_include.clone();
    let nros_cpp_src = nros_cpp_include.clone();
    let is_src_tree_stub = |dir: &str| -> bool {
        let p = PathBuf::from(dir);
        let canon = p.canonicalize().unwrap_or(p);
        canon == nros_c_src.canonicalize().unwrap_or(nros_c_src.clone())
            || canon == nros_cpp_src.canonicalize().unwrap_or(nros_cpp_src.clone())
    };

    // Materialise the include-dir lists ONCE so both builds share them.
    //   * `file_regular` / `file_deferred` — APP_INCLUDE_DIRS_FILE entries, with
    //     the source-tree stubs deferred to the end (the per-build generated
    //     header wins).
    //   * `app_include_dirs` — the legacy APP_INCLUDE_DIRS semicolon list.
    let mut file_regular: Vec<String> = Vec::new();
    let mut file_deferred: Vec<String> = Vec::new();
    if let Ok(includes_file) = env::var("APP_INCLUDE_DIRS_FILE") {
        match std::fs::read_to_string(&includes_file) {
            Ok(contents) => {
                for line in contents.lines() {
                    let dir = line.trim();
                    if dir.is_empty() {
                        continue;
                    }
                    if is_src_tree_stub(dir) {
                        file_deferred.push(dir.to_string());
                    } else {
                        file_regular.push(dir.to_string());
                    }
                }
            }
            Err(e) => panic!("APP_INCLUDE_DIRS_FILE={includes_file} not readable: {e}"),
        }
    }
    let app_include_dirs: Vec<String> = env::var("APP_INCLUDE_DIRS")
        .unwrap_or_default()
        .split(';')
        .filter(|d| !d.is_empty())
        .map(String::from)
        .collect();
    // Compile defs (APP_COMPILE_DEFS) shared by both builds (incl NROS_PKG_NAME
    // so the C node's NROS_NODE_REGISTER macro emits the right symbol).
    let compile_defs: Vec<(String, Option<String>)> = env::var("APP_COMPILE_DEFS")
        .unwrap_or_default()
        .split(';')
        .filter(|d| !d.is_empty())
        .map(|def| {
            let mut it = def.splitn(2, '=');
            let k = it.next().unwrap_or(def).to_string();
            let v = it.next().map(String::from);
            (k, v)
        })
        .collect();

    // Apply the common (language-agnostic + per-language) config to a build.
    let configure = |build: &mut cc::Build, want_cpp: bool| {
        build
            .cpp(want_cpp)
            .flag("-ffunction-sections")
            .flag("-fdata-sections")
            .define("NROS_PLATFORM_NUTTX", None)
            .warnings(false);
        for f in &arch_cflags {
            build.flag(f);
        }
        if want_cpp {
            // NuttX flat-build kernel ELF is `-static` with no GOT-init startup;
            // cc-rs's default `-fPIC` emits R_ARM_GOT_BREL relocations for COMDAT
            // statics that the static linker leaves zero (→ `nros::init` fails).
            // Disable PIC for C++ only — C TUs rely on the default PIC for their
            // NuttX kernel-symbol references.
            build.pic(false);

            // issue-0036 — NuttX C++ libc header precedence. The toolchain's
            // libstdc++ `<cstdlib>` does `#include_next <stdlib.h>`, which skips
            // the `-I` NuttX include dir and reaches the toolchain's **newlib**
            // `stdlib.h` — whose `div_t`/`ldiv_t`/`lldiv_t` are anonymous-struct
            // typedefs, conflicting with NuttX's named-struct (`struct div_s`)
            // ones that arrive via the direct `<stdlib.h>` (e.g. from nros-c's
            // `platform/posix.h`). Two libc header sets in one TU →
            // "conflicting declaration 'typedef struct div_t div_t'". We link
            // NuttX libc, so NuttX's headers must win. NuttX ships its own C++
            // wrappers under `include/cxx/` (`cstdlib` → NuttX `<stdlib.h>`);
            // putting that dir AHEAD of the cmake-passed `${NUTTX_DIR}/include`
            // makes `<cstdlib>` resolve to NuttX's wrapper, so the newlib
            // `include_next` never fires. `<type_traits>` etc. (not shipped under
            // `include/cxx/`) still fall through to libstdc++. Lighter than
            // `-nostdinc++` (which would also drop `<type_traits>`, needed by
            // nros-cpp's `node.hpp`).
            if let Ok(nuttx_dir) = env::var("NUTTX_DIR") {
                let cxx = PathBuf::from(nuttx_dir).join("include").join("cxx");
                if cxx.is_dir() {
                    build.include(&cxx);
                }
            }
        }
        // Generated per-build header dirs first (shadow the source-tree stubs).
        if let Ok(target_dir) = env::var("CARGO_TARGET_DIR") {
            let td = PathBuf::from(target_dir);
            build.include(td.join("nros-c-generated"));
            if want_cpp {
                build.include(td.join("nros-cpp-generated"));
            }
        }
        for dir in &file_regular {
            build.include(dir);
        }
        for dir in &file_deferred {
            build.include(dir);
        }
        if want_cpp {
            build.include(&nros_cpp_include);
            build.flag("-std=c++14");
        } else {
            build.include(&nros_c_include);
        }
        for dir in &app_include_dirs {
            build.include(dir);
        }
        // Source-tree fallback (lowest priority).
        if want_cpp {
            build.include(&nros_cpp_include);
            build.flag("-std=c++14");
        } else {
            build.include(&nros_c_include);
        }
        for (k, v) in &compile_defs {
            build.define(k, v.as_deref());
        }
    };

    // Partition all sources (main + extras) by language.
    let mut cpp_files: Vec<String> = Vec::new();
    let mut c_files: Vec<String> = Vec::new();
    if is_cpp {
        cpp_files.push(main_src.clone());
    } else {
        c_files.push(main_src.clone());
    }
    if let Ok(extra_sources) = env::var("APP_EXTRA_SOURCES") {
        for src in extra_sources.split(';') {
            if src.is_empty() {
                continue;
            }
            if is_cxx_ext(src) {
                cpp_files.push(src.to_string());
            } else {
                c_files.push(src.to_string());
            }
        }
    }

    // phase-263 C2b — per-component `NROS_PKG_NAME`. A multi-node LAUNCH entry composes
    // several `NROS_C_COMPONENT(...)` nodes, each of which names its `extern "C"` seam
    // `__nros_c_component_<NROS_PKG_NAME>_*` from the `-DNROS_PKG_NAME=<pkg>` define. A
    // single `cc::Build` carries one define for ALL its files, so the cmake side passes
    // `APP_EXTRA_SOURCE_PKGS="<abs-src>=<pkg>;…"` and each mapped source is compiled in its
    // OWN `cc::Build` with that pkg's define (its own archive). Unmapped sources + the main
    // entry keep the shared builds (back-compat with the single-node carrier, where the one
    // `NROS_PKG_NAME` in `APP_COMPILE_DEFS` is correct). This is the NuttX analog of how
    // Zephyr compiles each component as a separate static lib (phase-263 C2d).
    let mut src_pkg: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    if let Ok(map) = env::var("APP_EXTRA_SOURCE_PKGS") {
        for pair in map.split(';') {
            if let Some((s, p)) = pair.split_once('=')
                && !s.is_empty()
                && !p.is_empty()
            {
                src_pkg.insert(s.to_string(), p.to_string());
            }
        }
    }

    // Mapped sources compile solo; the rest stay in the shared per-language archives. The
    // C++ shared archive carries the entry + header-only runtime; the C shared archive any
    // declarative C node(s) without a per-source pkg (single-node carrier path).
    let mut shared_cpp: Vec<String> = Vec::new();
    let mut shared_c: Vec<String> = Vec::new();
    let mut solo: Vec<(String, bool, String)> = Vec::new(); // (path, want_cpp, pkg)
    for f in &cpp_files {
        if let Some(pkg) = src_pkg.get(f) {
            solo.push((f.clone(), true, pkg.clone()));
        } else {
            shared_cpp.push(f.clone());
        }
    }
    for f in &c_files {
        if let Some(pkg) = src_pkg.get(f) {
            solo.push((f.clone(), false, pkg.clone()));
        } else {
            shared_c.push(f.clone());
        }
    }

    // Compile the SHARED archives FIRST, then the solo per-component ones. cc-rs emits the
    // `-l` flags in compile order = link order, and a static archive's objects are pulled
    // only to satisfy references seen EARLIER on the line. The entry TU (in `app_cpp`)
    // references each component's `__nros_c_component_<pkg>_*` seam, so the entry archive
    // must precede the `app_pkg_*` archives that define them — else the seams stay
    // unresolved (the symptom before this ordering).
    if !shared_cpp.is_empty() {
        let mut build_cpp = cc::Build::new();
        configure(&mut build_cpp, true);
        for f in &shared_cpp {
            build_cpp.file(f);
        }
        build_cpp.compile("app_cpp");
    }
    if !shared_c.is_empty() {
        let mut build_c = cc::Build::new();
        configure(&mut build_c, false);
        for f in &shared_c {
            build_c.file(f);
        }
        build_c.compile("app_c");
    }
    // Each mapped source in its OWN archive with a per-component `NROS_PKG_NAME` (last `-D`
    // wins over the shared one from `APP_COMPILE_DEFS`, so the per-source pkg is effective).
    for (idx, (path, want_cpp, pkg)) in solo.iter().enumerate() {
        let mut build = cc::Build::new();
        configure(&mut build, *want_cpp);
        build.define("NROS_PKG_NAME", Some(pkg.as_str()));
        build.file(path);
        build.compile(&format!("app_pkg_{idx}"));
    }

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
    // The vector table now travels inside `libnros_nuttx_boot.a`
    // (nuttx_image_link.rs bundles `arm_vectortab.o` + the builtins stub and
    // links it `+whole-archive`). Emitting the raw object here as well made
    // every ARM C/C++ example link fail with `multiple definition of
    // _vector_start` once the boot archive landed; the riscv board already
    // opted out via `NUTTX_VECTORTAB_OBJ=""`.
    let _ = &vectortab_obj;
    println!("cargo:rustc-link-arg=-L{}", staging.display());
    println!("cargo:rustc-link-arg=-L{}", board_src.display());
    println!("cargo:rustc-link-arg=-Wl,--start-group");
    // #134 follow-up: link every archive the NuttX build actually staged
    // instead of a hardcoded list. Configs differ per board: the arm
    // rv-virt defconfig stages libxx/libcrypto/libboard, the riscv one
    // doesn't but adds libaudio (NXPLAYER/NXRECORDER) — a fixed list either
    // aborts the group ("cannot find -lcrypto") or drops needed archives
    // ("undefined reference to audio_register"). Order inside
    // --start-group is irrelevant.
    let mut staged: Vec<String> = std::fs::read_dir(&staging)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().into_string().ok()?;
            let lib = name.strip_prefix("lib")?.strip_suffix(".a")?;
            Some(lib.to_string())
        })
        .collect();
    staged.sort();
    if board_src.join("libboard.a").exists() {
        staged.push("board".to_string());
    }
    if staged.is_empty() {
        panic!(
            "NuttX staging dir {} contains no lib*.a archives — did the NuttX build run?",
            staging.display()
        );
    }
    for lib in staged {
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
