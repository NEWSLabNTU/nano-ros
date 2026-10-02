use std::{
    env,
    path::{Path, PathBuf},
    process::Command,
};

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
    //
    // issue 1588 — every `APP_*` PATH input (the main source, the include-dir
    // list and list file, the extra / interface source lists, the left half of
    // each source=pkg pair, the FFI lib list file and every line in both files)
    // is resolved through `nros_build_paths`, i.e. under issue 1280's rule: a
    // value naming ANOTHER nano-ros checkout is re-rooted onto this one, with a
    // `cargo::warning`. `APP_COMPILE_DEFS` is not a path and is read as text.
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

    let main_src = nros_build_paths::env_path("APP_MAIN_CPP").unwrap_or_else(|| {
        panic!(
            "APP_MAIN_CPP not set. Set it to the path of the C/C++ source file.\n\
             Example: APP_MAIN_CPP=examples/qemu-armv7a-nuttx/c/zenoh/talker/src/main.c"
        )
    });

    let is_cpp = is_cxx_ext(&main_src);

    let sizes_includes = per_build_sizes_includes();

    // Phase 238.C — mixed C/C++ app build. A NuttX C example registers a
    // declarative C node (`Talker.c`, C-linkage `__nros_component_<pkg>_register`)
    // but is driven by the header-only C++ `EntryNodeRuntime` (the generated
    // entry is a `.cpp`). cc-rs compiles every file in one `cc::Build` with a
    // single language, so a `.c` extra under a `.cpp` main would be forced to
    // C++ — mangling the C node's register symbol. Compile `.c` sources in a
    // separate C `cc::Build` (and `.cpp/.cc/.cxx` in a C++ one), so each source
    // keeps its native linkage. The two archives both link into the kernel ELF.
    // Resolve the source-tree stub dirs so the APP_INCLUDE_DIRS_FILE pass can
    // defer them (they hold `#error` stubs of nros_{,cpp_}config_generated.h;
    // the per-build mirror must win). Same logic as pre-238.C.
    let nros_c_src = nros_c_include.clone();
    let nros_cpp_src = nros_cpp_include.clone();
    let is_src_tree_stub = |dir: &Path| -> bool {
        let canon = dir.canonicalize().unwrap_or_else(|_| dir.to_path_buf());
        canon == nros_c_src.canonicalize().unwrap_or(nros_c_src.clone())
            || canon == nros_cpp_src.canonicalize().unwrap_or(nros_cpp_src.clone())
    };

    // Materialise the include-dir lists ONCE so both builds share them.
    //   * `file_regular` / `file_deferred` — APP_INCLUDE_DIRS_FILE entries, with
    //     the source-tree stubs deferred to the end (the per-build generated
    //     header wins).
    //   * `app_include_dirs` — the legacy APP_INCLUDE_DIRS semicolon list.
    let mut file_regular: Vec<PathBuf> = Vec::new();
    let mut file_deferred: Vec<PathBuf> = Vec::new();
    let includes_file =
        nros_build_paths::env_path_list_file("APP_INCLUDE_DIRS_FILE").map(|(file, dirs)| {
            for dir in dirs {
                if is_src_tree_stub(&dir) {
                    file_deferred.push(dir);
                } else {
                    file_regular.push(dir);
                }
            }
            file
        });
    let app_include_dirs = nros_build_paths::env_path_list_sep("APP_INCLUDE_DIRS", ';');
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
        // Issue 1570 — every TU compiled here writes a depfile; the files it
        // names are declared to cargo after the last compile below.
        nros_cc_flags::header_deps::track_header_deps(build);
        // issue 0383 — C only; a C++ TU rejects both constructs anyway and gcc
        // just warns that the option does not apply to the language.
        if !want_cpp {
            nros_cc_flags::strict_decls(build);
        }
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
            // Issue 0551 — through the accessor, like every other NuttX header
            // input. The snapshot carries `include/cxx`, so this keeps
            // resolving; reaching the shared tree directly would silently
            // depend on it still being configured, which after any
            // `make olddefconfig` it is not.
            // phase-471 W6 — through `env_path`, like the sibling module's
            // `nuttx_dir()`. `None` when unset keeps the "host cargo check"
            // gate this read already had; what it adds is issue 1280's rule,
            // so this include and the kernel libs below name ONE tree.
            if let Some(nuttx_dir) = nros_build_paths::env_path("NUTTX_DIR") {
                let cxx = crate::nuttx_export::include_root(&nuttx_dir).join("cxx");
                if cxx.is_dir() {
                    build.include(&cxx);
                }
            }
        }
        // issue 1569 — the image's OWN per-build sizes headers first (they
        // shadow the source-tree stubs), for C and C++ alike: a C component
        // reads the C++ header too (`component.h` sizes its publisher buffers
        // from `NROS_PUBLISHER_SIZE`).
        for dir in &sizes_includes {
            build.include(dir);
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
    let mut cpp_files: Vec<PathBuf> = Vec::new();
    let mut c_files: Vec<PathBuf> = Vec::new();
    if is_cpp {
        cpp_files.push(main_src.clone());
    } else {
        c_files.push(main_src.clone());
    }
    for src in nros_build_paths::env_path_list_sep("APP_EXTRA_SOURCES", ';') {
        if is_cxx_ext(&src) {
            cpp_files.push(src);
        } else {
            c_files.push(src);
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
    //
    // issue 1588 — the lookup below matches a pair's path against an
    // `APP_EXTRA_SOURCES` element, so both are resolved by the SAME
    // `nros_build_paths` rule; resolving only one side would miss every pair.
    let src_pkg: std::collections::HashMap<PathBuf, String> =
        nros_build_paths::env_path_pairs("APP_EXTRA_SOURCE_PKGS", ';', '=')
            .into_iter()
            .collect();

    // Mapped sources compile solo; the rest stay in the shared per-language archives. The
    // C++ shared archive carries the entry + header-only runtime; the C shared archive any
    // declarative C node(s) without a per-source pkg (single-node carrier path).
    let mut shared_cpp: Vec<PathBuf> = Vec::new();
    let mut shared_c: Vec<PathBuf> = Vec::new();
    let mut solo: Vec<(PathBuf, bool, String)> = Vec::new(); // (path, want_cpp, pkg)
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

    // phase-281 W3-nuttx (C lane) — generated C interface serdes TUs
    // (`std_msgs_msg_int32.c` defining `std_msgs_msg_int32_init/serialize`, etc.),
    // compiled into a SINGLE `app_iface` archive emitted LAST so its `-l` flag lands
    // AFTER every `app_pkg_*` on the link line. The node TUs (in `app_pkg_*`) REFERENCE
    // these serdes, and a static archive's objects are pulled only to satisfy references
    // seen EARLIER on the line — so the archive DEFINING the serdes must come after the
    // ones referencing them. (The C++ lane's serdes ride the trailing Rust `_ffi_lib` via
    // APP_FFI_LIBS, which is already emitted last; the pure-C serdes have no such lib.)
    // These TUs carry no `NROS_C_COMPONENT`, so no `-DNROS_PKG_NAME` is needed. Skipped
    // cleanly when empty (the pure-C pub/sub nuttx entry, whose nodes use no generated
    // serdes, passes nothing here).
    {
        let (iface_cpp, iface_c): (Vec<PathBuf>, Vec<PathBuf>) =
            nros_build_paths::env_path_list_sep("APP_INTERFACE_SOURCES", ';')
                .into_iter()
                .partition(|src| is_cxx_ext(src));
        if !iface_c.is_empty() {
            let mut build = cc::Build::new();
            configure(&mut build, false);
            for f in &iface_c {
                build.file(f);
            }
            build.compile("app_iface_c");
        }
        if !iface_cpp.is_empty() {
            let mut build = cc::Build::new();
            configure(&mut build, true);
            for f in &iface_cpp {
                build.file(f);
            }
            build.compile("app_iface_cpp");
        }
    }

    // Issue 1570 — the rebuild edge for everything compiled above. These TUs are
    // the image's component sources, handed over as env LISTS, and the lists are
    // all `rerun-if-env-changed` can watch: an edit to a component `.c`, to
    // `component.h`, or to the committed NuttX config snapshot left cargo's
    // fingerprint unchanged and the image a museum binary (measured: the ctrl
    // instance at 0x2d8 in the object cmake rebuilt, 0x288 in the image).
    // The compiler knows what each TU opened; declare exactly that. Cargo also
    // copies these into the artifact's dep-info, which the cmake rule consumes
    // as its DEPFILE, so the same list is the edge that re-runs cargo at all.
    nros_cc_flags::header_deps::emit_header_deps(&PathBuf::from(
        env::var("OUT_DIR").expect("OUT_DIR set by cargo for build scripts"),
    ));

    // ---- NuttX kernel link args ----
    // The binary IS the NuttX kernel. Link against all NuttX staging libraries,
    // linker script, and startup objects.
    // issue 0491 — `NUTTX_DIR` names a DIRECTORY, and cargo compares an env
    // value as TEXT, so fingerprinting the spelling lets two consumers that
    // spell one directory differently invalidate each other inside a shared
    // `--target-dir`. Not replaced by a content watch: NuttX is BUILT IN
    // PLACE, so watching its tree would leave this permanently dirty after
    // every kernel build. The specific inputs are declared per file.
    //
    // phase-471 W6 — `env_path`, not a bare `env::var`. Still env-gated (it
    // answers `None` when unset, so a host `cargo check` stays
    // link-directive-free) and still no `rerun-if-env-changed`; what it adds is
    // issue 1280's rule. The measured reason: `nuttx_platform_build` resolves
    // the SAME variable through `nros_build_paths::nuttx_dir()`, so in a linked
    // worktree the platform C port compiled against THIS checkout while the
    // kernel libs and headers came from the one the parent shell had
    // activated. "Built in place" is an argument for naming ONE tree, not for
    // naming whichever tree happens to be built.
    let Some(nuttx_dir) = nros_build_paths::env_path("NUTTX_DIR") else {
        return;
    };

    // phase-339 W2 — link the per-arch export SNAPSHOT, not the shared live
    // tree. `kernel_libs` falls back to `staging/` when this arch has no
    // snapshot, so a tree provisioned by an older `build-nuttx.sh` still links.
    let kernel = crate::nuttx_export::kernel_libs(&nuttx_dir);
    let staging = kernel.libs.clone();
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
        // Issue 0511 — the arch-correct headers, not the shared tree's (this is
        // the same cpp pass over the same linker script as `nuttx_image_link`).
        format!(
            "-isystem{}",
            crate::nuttx_export::include_root(&nuttx_dir).display()
        ),
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
    if !kernel.from_snapshot {
        println!("cargo:rustc-link-arg=-L{}", board_src.display());
    }
    println!("cargo:rustc-link-arg=-Wl,--start-group");
    // #134 follow-up: link every archive the NuttX build actually staged
    // instead of a hardcoded list. Configs differ per board: the arm
    // rv-virt defconfig stages libxx/libcrypto/libboard, the riscv one
    // doesn't but adds libaudio (NXPLAYER/NXRECORDER) — a fixed list either
    // aborts the group ("cannot find -lcrypto") or drops needed archives
    // ("undefined reference to audio_register"). Order inside
    // --start-group is irrelevant.
    //
    // phase-339 W2 — watch the dir so the lib set is re-scanned when archives
    // are added or removed.
    //
    // This used to be the SHARED live `staging/`, which both architectures wrote:
    // the watch was correct (it stopped us linking "a lib list from the other
    // config's kernel") but it made every arm entry read stale the moment riscv
    // built, because the watched path had genuinely changed (issue 0433). With a
    // per-arch snapshot the watch means what it says — this arch's kernel changed
    // — and the other architecture cannot trip it.
    println!("cargo:rerun-if-changed={}", staging.display());
    if !kernel.from_snapshot {
        // Compatibility path only: pre-phase-339 trees still need the separate
        // board-lib dir watched, and still carry the cross-arch hazard above.
        println!("cargo:rerun-if-changed={}", board_src.display());
    }
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
    // phase-339 W2 — the snapshot ships `libs/libboard.a`, so the scan above
    // already found it. Only the live-tree fallback needs the special case,
    // where `libboard.a` sits outside the staging dir.
    if !kernel.from_snapshot && board_src.join("libboard.a").exists() {
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

    // issue 1588 — every content watch names the RESOLVED path. Watching the
    // raw value would, in exactly the case the re-root exists for, watch the
    // OTHER checkout's file while compiling this one's: an edit here would
    // never re-run the script.
    println!("cargo:rerun-if-changed={}", main_src.display());
    println!("cargo:rerun-if-changed={}", linker_script.display());
    if let Some(includes_file) = &includes_file {
        println!("cargo:rerun-if-changed={}", includes_file.display());
    }
    // Each line is an absolute path to a `lib<name>.a` static lib.
    // Forward to rustc as a link search dir + a -l static link.
    // Avoids `undefined reference to nros_cpp_serialize_…` from the
    // Rust FFI glue that the `<pkg>__nano_ros_cpp` interface library
    // would normally drag in via cmake's regular link graph.
    if let Some((ffi_libs_file, libs)) = nros_build_paths::env_path_list_file("APP_FFI_LIBS_FILE") {
        println!("cargo:rerun-if-changed={}", ffi_libs_file.display());
        for lib_path in libs {
            let dir = lib_path.parent().unwrap_or_else(|| Path::new("."));
            let stem = lib_path
                .file_stem()
                .and_then(|s| s.to_str())
                .and_then(|s| s.strip_prefix("lib"))
                .unwrap_or_else(|| {
                    panic!(
                        "FFI lib path {} has no `lib<name>.a` shape",
                        lib_path.display()
                    )
                });
            println!("cargo:rustc-link-search=native={}", dir.display());
            println!("cargo:rustc-link-lib=static={stem}");
            // Issue 1570 — the archive's CONTENT is a link input. Cargo
            // does not fingerprint a `rustc-link-lib`, so without this a
            // rebuilt archive left the image linked against the old one
            // (0475's class: a lib reached with no file-level edge).
            println!("cargo:rerun-if-changed={}", lib_path.display());
        }
    }
    // issue 0491 forbids fingerprinting a PATH variable's spelling, and these
    // stay anyway — deliberately, and not on the "owns its target dir" premise
    // they used to be exempted on (issue 0805 made the NuttX leaves SHARE one).
    // The value is not a respelling of one input here, it SELECTS the inputs:
    // this one unit compiles each leaf's own sources into its own image, so
    // leaf B after leaf A in the shared dir MUST re-run it, and the content
    // watches above cannot say so — they name A's files, which did not change.
    // 0491's cost (siblings invalidating each other) is therefore no cost: two
    // leaves never share this script's output. The content watches are what
    // catch an edit WITHIN one leaf. Argued in check-path-env-fingerprints.py.
    //
    // Spelled as literals, one a line, so that gate can READ them: a loop over
    // an array would interpolate the name and pass it unexamined.
    // `APP_EXTRA_SOURCE_PKGS` had no directive at all before issue 1588 — a
    // changed source=pkg map with an unchanged source list re-ran nothing.
    println!("cargo:rerun-if-env-changed=APP_MAIN_CPP");
    println!("cargo:rerun-if-env-changed=APP_INCLUDE_DIRS");
    println!("cargo:rerun-if-env-changed=APP_INCLUDE_DIRS_FILE");
    println!("cargo:rerun-if-env-changed=APP_FFI_LIBS_FILE");
    println!("cargo:rerun-if-env-changed=APP_EXTRA_SOURCES");
    println!("cargo:rerun-if-env-changed=APP_EXTRA_SOURCE_PKGS");
    println!("cargo:rerun-if-env-changed=APP_INTERFACE_SOURCES");
    println!("cargo:rerun-if-env-changed=APP_COMPILE_DEFS");
}

/// A C++ translation unit, by extension — the split between the C and the C++
/// `cc::Build` (phase 238.C).
fn is_cxx_ext(p: &Path) -> bool {
    matches!(
        p.extension().and_then(|e| e.to_str()),
        Some("cpp" | "cxx" | "cc")
    )
}

/// issue 1569 — the per-build sizes header directories of the `nros-c` and
/// `nros-cpp` units linked into THIS image, from their `links` channels
/// (`DEP_NROS_C_CONFIG_INCLUDE`, `DEP_NROS_CPP_CONFIG_INCLUDE`).
///
/// This used to add `$CARGO_TARGET_DIR/nros-{c,cpp}-generated`, the FLAT shared
/// copies, which had no ordering edge: `nros-cpp` carried no `links` key, so
/// cargo was free to run this script before `nros-cpp`'s had written its
/// header, and every TU then fell through the include path to the dispatching
/// stub — which on NuttX included a COMMITTED, hand-kept snapshot. Measured on
/// realtime-c: the tier executor storage was 2 x 98,312 (the snapshot) against
/// 2 x 89,072 (the build), and seven buffer sizes in the snapshot had fallen
/// BELOW the build (issue 1568). Both keys carry `links` now, so cargo orders
/// this script after both writers, and both paths are the writers' own OUT_DIR
/// copies — never a sibling feature set's header on the shared path.
///
/// A missing header is a hard error that names it. There is no fallback to
/// fall through to any more: the stubs no longer dispatch to a snapshot for a
/// NuttX build, so compiling on regardless would only move this panic to a
/// less legible `#error` inside a component.
fn per_build_sizes_includes() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    for (key, header) in [
        ("DEP_NROS_C_CONFIG_INCLUDE", "nros_config_generated.h"),
        ("DEP_NROS_CPP_CONFIG_INCLUDE", "nros_cpp_config_generated.h"),
    ] {
        let dir = env::var(key).map(PathBuf::from).unwrap_or_else(|_| {
            panic!(
                "{key} is not set: this crate must depend DIRECTLY on the crate that \
                 publishes it (`nros-c` / `nros-cpp`, whose `links` key carries it). \
                 Without it there is no per-build sizes header to compile against \
                 (issue 1569)."
            )
        });
        let file = dir.join("nros").join(header);
        if !file.is_file() {
            panic!(
                "the per-build sizes header {} does not exist. Its writer ran (cargo \
                 orders this script after it) but wrote nothing — most likely its size \
                 probe read 0, i.e. a `cargo check` or an RMW-less feature set. A NuttX \
                 image cannot be sized without it; there is deliberately no committed \
                 fallback (issue 1569).",
                file.display()
            );
        }
        println!("cargo:rerun-if-changed={}", file.display());
        dirs.push(dir);
    }
    dirs
}
