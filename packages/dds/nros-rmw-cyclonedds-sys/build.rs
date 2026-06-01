//! Build script for `nros-rmw-cyclonedds-sys` (Phase 212.K.2).
//!
//! When the `vendored` feature is **off** (default), this script is a
//! no-op — the C++ backend is supplied externally (Zephyr module,
//! standalone CMake project) and we only re-export the Rust shim that
//! declares `nros_rmw_cyclonedds_register`.
//!
//! When `vendored` is **on**, we compile the existing C++ backend
//! (`packages/dds/nros-rmw-cyclonedds/src/*.cpp`) plus a small
//! `rmw_dds_common_graph` descriptor (generated at build time via the
//! host `idlc` shipped by the sibling `cyclonedds-sys` crate) into a
//! single static library. The result is link-fed into the cargo
//! command line with `+whole-archive` so the C++ static-init
//! `nros_rmw_cyclonedds_register` (via the `vtable.cpp` register entry)
//! plus the bundled descriptor's `__attribute__((constructor))` are
//! both pulled in.

fn main() {
    println!("cargo:rerun-if-changed=build.rs");

    // Off-by-default. The default register-shim build is symbol-only.
    #[cfg(feature = "vendored")]
    vendored_build();
}

#[cfg(feature = "vendored")]
fn vendored_build() {
    use std::{env, path::PathBuf};

    // Pick up Cyclone artifacts published by `cyclonedds-sys` via the
    // `links = "ddsc"` metadata bridge. Cargo turns each `cargo:KEY=VAL`
    // emitted by the upstream into `DEP_DDSC_<KEY upper-cased>`.
    let ddsc_include = env::var_os("DEP_DDSC_INCLUDE").map(PathBuf::from).expect(
        "DEP_DDSC_INCLUDE not set — cyclonedds-sys must run first; \
         ensure the `vendored` feature pulls it in as a dependency.",
    );
    let idlc = env::var_os("DEP_DDSC_IDLC").map(PathBuf::from).expect(
        "DEP_DDSC_IDLC not set — cyclonedds-sys did not export the \
         host idlc path.",
    );
    println!("cargo:rerun-if-env-changed=DEP_DDSC_INCLUDE");
    println!("cargo:rerun-if-env-changed=DEP_DDSC_IDLC");

    let repo_root = nros_build_paths::repo_root();
    let backend_dir = repo_root.join("packages/dds/nros-rmw-cyclonedds");
    let backend_src = backend_dir.join("src");
    let backend_inc = backend_dir.join("include");
    let rmw_cffi_inc = repo_root.join("packages/core/nros-rmw-cffi/include");

    if !backend_src.is_dir() {
        panic!(
            "nros-rmw-cyclonedds-sys: backend src dir not found at {}",
            backend_src.display(),
        );
    }

    let out_dir = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR"));

    // -------------------------------------------------------------
    // Bake descriptors via host idlc:
    //   - rmw_dds_common::msg::dds_::ParticipantEntitiesInfo_  (from
    //     the backend's hand-written IDL — required for ros_discovery_info).
    //   - std_msgs::msg::dds_::Int32_  (used by the native Rust
    //     talker/listener examples — phase 212.K.4 will replace this
    //     with per-example codegen, but baking it here keeps the
    //     wave-8 acceptance test pure-cargo).
    // -------------------------------------------------------------
    let gen_dir = out_dir.join("cyclonedds-types");
    std::fs::create_dir_all(&gen_dir).expect("mkdir gen");
    let mut cc_c = cc::Build::new();
    cc_c.include(&ddsc_include)
        .include(&gen_dir)
        .flag_if_supported("-Wno-unused-parameter");

    // ---- rmw_dds_common graph ----
    let graph_idl = backend_src.join("idl/rmw_dds_common_graph.idl");
    println!("cargo:rerun-if-changed={}", graph_idl.display());
    bake_descriptor(
        &idlc,
        &graph_idl,
        &gen_dir,
        "rmw_dds_common_graph",
        &[(
            "rmw_dds_common::msg::dds_::ParticipantEntitiesInfo_",
            "rmw_dds_common_msg_dds__ParticipantEntitiesInfo__desc",
        )],
        &mut cc_c,
    );

    // ---- std_msgs/Int32 — synthesise the IDL via nros-msg-to-idl ----
    let int32_idl_path = gen_dir.join("Int32.idl");
    let int32_idl = nros_msg_to_idl::msg_to_idl("int32 data\n", "std_msgs", "Int32")
        .expect("nros-msg-to-idl: std_msgs/Int32");
    std::fs::write(&int32_idl_path, int32_idl).expect("write Int32.idl");
    bake_descriptor(
        &idlc,
        &int32_idl_path,
        &gen_dir,
        "Int32",
        &[(
            "std_msgs::msg::dds_::Int32_",
            "std_msgs_msg_dds__Int32__desc",
        )],
        &mut cc_c,
    );
    // cc::Build emits `cargo:rustc-link-lib=static=…` by default. We
    // want whole-archive for the descriptor so its
    // `__attribute__((constructor))` register TU isn't dropped.
    // Disable the default emit, compile manually, then emit our own.
    cc_c.cargo_metadata(false);
    cc_c.compile("nros_rmw_cyclonedds_descriptors");
    println!("cargo:rustc-link-search=native={}", out_dir.display());
    println!("cargo:rustc-link-lib=static:+whole-archive,-bundle=nros_rmw_cyclonedds_descriptors");

    // -------------------------------------------------------------
    // Compile the C++ backend.
    // -------------------------------------------------------------
    let cpp_files = [
        "vtable.cpp",
        "session.cpp",
        "publisher.cpp",
        "subscriber.cpp",
        "service.cpp",
        "descriptors.cpp",
        "graph.cpp",
        "qos.cpp",
        "sertype_min.cpp",
    ];
    for f in cpp_files {
        println!("cargo:rerun-if-changed={}", backend_src.join(f).display());
    }
    let mut cc_cpp = cc::Build::new();
    cc_cpp
        .cpp(true)
        .std("c++14")
        .include(&ddsc_include)
        .include(&backend_inc)
        .include(&rmw_cffi_inc)
        .include(&backend_src)
        .include(&gen_dir)
        // Mirror the project CMakeLists's hardening flags.
        .flag_if_supported("-fno-exceptions")
        .flag_if_supported("-fno-rtti")
        .flag_if_supported("-fno-threadsafe-statics")
        .flag_if_supported("-ffunction-sections")
        .flag_if_supported("-fdata-sections")
        .flag_if_supported("-Wno-unused-parameter")
        .flag_if_supported("-Wno-pedantic");
    for f in cpp_files {
        cc_cpp.file(backend_src.join(f));
    }
    cc_cpp.cargo_metadata(false);
    cc_cpp.compile("nros_rmw_cyclonedds");
    println!("cargo:rustc-link-search=native={}", out_dir.display());
    // Force whole-archive on the C++ backend — `vtable.cpp`'s
    // `nros_rmw_cyclonedds_register` is the constructor-side entry
    // pulled in by the Rust `register()` shim. The +whole-archive
    // modifier survives Cargo propagation (unlike `cargo:rustc-link-arg`
    // which is bin-target-local).
    println!("cargo:rustc-link-lib=static:+whole-archive,-bundle=nros_rmw_cyclonedds");

    // Cyclone's wrapper drags in stdc++.
    println!("cargo:rustc-link-lib=dylib=stdc++");

    // The cmake project's `target_compile_definitions` are only relevant
    // for embedded RTOS targets — on hosted POSIX we leave them off.
}

/// Drive `idlc -t -l c` over `idl_path`, emit a tiny register TU per
/// `(type_name, descriptor_symbol)` pair (one constructor each), and
/// add both to `cc`. Matches the cmake helper
/// `nros_rmw_cyclonedds_idlc_compile()` in
/// `packages/dds/nros-rmw-cyclonedds/cmake/NrosRmwCycloneddsTypeSupport.cmake`.
#[cfg(feature = "vendored")]
fn bake_descriptor(
    idlc: &std::path::Path,
    idl_path: &std::path::Path,
    gen_dir: &std::path::Path,
    stem: &str,
    types: &[(&str, &str)],
    cc: &mut cc::Build,
) {
    use std::process::Command;
    let status = Command::new(idlc)
        .args(["-t", "-l", "c", "-o"])
        .arg(gen_dir)
        .arg(idl_path)
        .status()
        .expect("invoke idlc");
    if !status.success() {
        panic!("idlc failed on {}", idl_path.display());
    }
    let gen_c = gen_dir.join(format!("{stem}.c"));
    let gen_h = gen_dir.join(format!("{stem}.h"));
    if !gen_c.is_file() || !gen_h.is_file() {
        panic!(
            "idlc did not emit expected outputs at {} / {}",
            gen_c.display(),
            gen_h.display(),
        );
    }
    cc.file(&gen_c);
    for (idx, (type_name, desc_sym)) in types.iter().enumerate() {
        let reg = gen_dir.join(format!("{stem}_register_{idx}.c"));
        let src = format!(
            r#"/* Auto-generated by nros-rmw-cyclonedds-sys build.rs */
#include "dds/dds.h"
#include "{stem}.h"

extern const dds_topic_descriptor_t {desc_sym};

void nros_rmw_cyclonedds_register_descriptor(
    const char *type_name, const dds_topic_descriptor_t *desc);

void register_{stem}_{idx}(void) {{
    nros_rmw_cyclonedds_register_descriptor(
        "{type_name}", &{desc_sym});
}}

__attribute__((constructor))
static void register_{stem}_{idx}_constructor(void) {{
    register_{stem}_{idx}();
}}
"#,
        );
        std::fs::write(&reg, src).expect("write register TU");
        cc.file(&reg);
    }
}
