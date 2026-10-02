//! Compile the C++ application into this image, and generate the message
//! bindings it uses.
//!
//! The C++ half of the shape the `c/talker` sibling established (issue 1512):
//! `src/main.rs` owns the reset vector and the board bring-up, this script
//! compiles `src/talker.cpp`, and `NROS_APP_MAIN_REGISTER()` in it emits the
//! `app_main()` that `main.rs` calls. See `README.md` for why the link root is
//! Rust on this board.
//!
//! What differs from C is where the message code lives. `nros generate cpp`
//! emits header-only C++ types plus their CDR serializers as RUST (`*_types.rs`
//! / `*_exports.rs` per message) — on the cmake road those are wrapped in a
//! per-package staticlib. Here they are compiled into the binary crate itself:
//! this script writes `cpp_msg_glue.rs` (one `include!` per file) and
//! `src/main.rs` includes it under the prelude the glue expects.
//!
//! Inputs, none of them a guessed path:
//!
//!  * the C++ headers and the per-build `nros_cpp_config_generated.h` —
//!    `DEP_NROS_CPP_INCLUDE` / `DEP_NROS_CPP_CONFIG_INCLUDE` (`links = "nros_cpp"`);
//!  * the C headers they include, the platform ABI headers and the per-build
//!    `nros_config_generated.h` — `DEP_NROS_C_*` (`links = "nros_c"`). The
//!    `links` keys also ORDER this script after both writers of those headers;
//!  * the generated bindings — `nros generate cpp`, into `OUT_DIR`;
//!  * the compilers for the board's triple — `CC_`/`CXX_thumbv7m_none_eabi` or
//!    cc-rs's default for the triple (`arm-none-eabi-g++`).

use std::{
    fmt::Write as _,
    path::{Path, PathBuf},
    process::Command,
};

fn env(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| {
        panic!(
            "baremetal-cpp-talker: {name} is not set — nros-c / nros-cpp must carry \
             their `links` keys (issue 1512)"
        )
    })
}

/// Every `*_types.rs` / `*_exports.rs` the generator wrote for `pkg`, sorted so
/// the include order is a function of the tree and not of readdir(2).
fn glue_files(pkg_dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    for sub in ["msg", "srv", "action"] {
        let Ok(entries) = std::fs::read_dir(pkg_dir.join(sub)) else {
            continue;
        };
        files.extend(
            entries
                .filter_map(Result::ok)
                .map(|e| e.path())
                .filter(|p| {
                    p.extension().is_some_and(|x| x == "rs")
                        && p.file_stem()
                            .and_then(|s| s.to_str())
                            .is_some_and(|s| s.ends_with("_types") || s.ends_with("_exports"))
                }),
        );
    }
    files.sort();
    files
}

fn main() {
    println!("cargo:rerun-if-changed=src/talker.cpp");
    println!("cargo:rerun-if-changed=package.xml");
    // RFC-0033 per-field capacities, which the generator discovers beside
    // `package.xml` (a leaf that receives `std_msgs/String` needs one: an
    // unbounded type has no receive bound, and codegen poisons rather than
    // invents one — issue 0964). Watched only when present, because cargo
    // re-runs a build script on EVERY build when a watched path is missing
    // (issue 0490) — the existence test is the guard, which is why the path is
    // a variable here rather than a literal `check-build-rs-rerun-paths` reads.
    let caps = "nros-codegen.toml";
    if std::path::Path::new(caps).exists() {
        println!("cargo:rerun-if-changed={caps}");
    }
    println!("cargo:rerun-if-changed=build.rs");

    let out_dir = PathBuf::from(env("OUT_DIR"));
    let generated = out_dir.join("generated");

    // ---- generated message bindings -------------------------------------
    //
    // `--ros-edition humble` pairs with the `ros-humble` feature this leaf names
    // on `nros-cpp` (RFC-0056: the edition decides the keyexpr format, which
    // must match the codegen-baked type hash).
    let tool = std::env::var("NROS_CLI").unwrap_or_else(|_| "nros".to_string());
    let status = Command::new(&tool)
        .args(["generate", "cpp", "--force", "--ros-edition", "humble"])
        .arg("--manifest")
        .arg("package.xml")
        .arg("-o")
        .arg(&generated)
        .status()
        .unwrap_or_else(|e| {
            panic!(
                "baremetal-cpp-talker: could not run `{tool} generate cpp` ({e}). \
                 Build the CLI and put it on PATH: `just setup-cli` then \
                 `source ./activate.sh`, or set NROS_CLI to the `nros` binary."
            )
        });
    assert!(
        status.success(),
        "baremetal-cpp-talker: `{tool} generate cpp` failed ({status})."
    );

    // ---- the Rust half of the message bindings ---------------------------
    //
    // ALL packages' glue, types AND exports, in one module: this binary is the
    // only archive in the image, so the cmake road's per-package split (each
    // archive defines only its OWN `nros_cpp_*` exports, issue 0253) has nothing
    // to protect here.
    let mut glue = String::new();
    let mut any = false;
    for pkg in ["builtin_interfaces", "std_msgs"] {
        for file in glue_files(&generated.join(pkg)) {
            println!("cargo:rerun-if-changed={}", file.display());
            writeln!(glue, "include!({:?});", file.display().to_string()).unwrap();
            any = true;
        }
    }
    assert!(
        any,
        "baremetal-cpp-talker: `nros generate cpp` produced no `*_types.rs` / \
         `*_exports.rs` under {} — the bindings layout changed.",
        generated.display()
    );
    std::fs::write(out_dir.join("cpp_msg_glue.rs"), glue).expect("write cpp_msg_glue.rs");

    // ---- compile ---------------------------------------------------------
    let cpp_include = env("DEP_NROS_CPP_INCLUDE");
    let cpp_config_include = env("DEP_NROS_CPP_CONFIG_INCLUDE");
    let c_include = env("DEP_NROS_C_INCLUDE");
    let c_config_include = env("DEP_NROS_C_CONFIG_INCLUDE");
    let platform_include = env("DEP_NROS_C_PLATFORM_INCLUDE");

    // Watch the CONTENT of the per-build config headers (issue 0491), so a
    // runtime rebuild that moves a size re-runs this script.
    println!("cargo:rerun-if-changed={c_config_include}/nros/nros_config_generated.h");
    println!("cargo:rerun-if-changed={cpp_config_include}/nros/nros_cpp_config_generated.h");

    let mut build = cc::Build::new();
    build
        .cpp(true)
        // No `-lstdc++`: cc-rs links the C++ standard library by default, and
        // this image has none to link (rust-lld: `unable to find library
        // -lstdc++`). Nothing here needs one — the flags below remove what a
        // C++ runtime would otherwise be asked for.
        .cpp_link_stdlib(None)
        // The per-build config dirs BEFORE the crates' own `include/`: both
        // crates commit a stub of the same header name that defines nothing,
        // and whichever directory comes first wins (the `c/` sibling's
        // build.rs has the long form).
        .include(&cpp_config_include)
        .include(&c_config_include)
        .include(&cpp_include)
        .include(&c_include)
        .include(&platform_include)
        .include(&generated)
        // `#include "std_msgs.hpp"`, the umbrella, as every C++ example spells it.
        .include(generated.join("std_msgs"))
        .include(generated.join("builtin_interfaces"))
        .std("c++17")
        // Freestanding, no exceptions, no RTTI: this image has no libc++ runtime
        // and no unwinder. `-ffreestanding` is also what keeps `nros-cpp`'s
        // hosted-STL includes out — each is behind `__STDC_HOSTED__` AND
        // `__has_include`, and freestanding answers the first one.
        .flag("-ffreestanding")
        .flag("-fno-exceptions")
        .flag("-fno-rtti")
        // No `__cxa_guard_*` for function-local statics: there is one thread.
        .flag("-fno-threadsafe-statics")
        // No `__cxa_atexit` / `__dso_handle`: nothing here ever exits to a
        // runtime that would run the registered destructors.
        .flag("-fno-use-cxa-atexit")
        .file("src/talker.cpp");
    // The C++ half of the cc policy (issue 0478's gcc-safe frame pointer).
    // `strict_decls` is C-only — both diagnostics are C-language options, and a
    // C++ compile rejects the constructs outright.
    nros_cc_flags::gcc_safe_frame_pointer(&mut build);
    nros_cc_flags::header_deps::track_header_deps(&mut build);
    build.compile("baremetal_cpp_talker_app");
    // issue 1580 — every header the compile opened, beyond the ones named above.
    nros_cc_flags::header_deps::emit_header_deps(&out_dir);
}
