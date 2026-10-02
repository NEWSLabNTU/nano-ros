//! Compile the C application (and the message C bindings it uses) into this
//! image.
//!
//! This is the half of the leaf that makes a C application reach a pure
//! bare-metal board (issue 1512). The other half is `src/main.rs`, which owns
//! the reset vector and the board bring-up; see `README.md` for why the link
//! root has to be Rust here and is C on every RTOS.
//!
//! The script is LEAF-AGNOSTIC — every C leaf in this family carries the same
//! bytes. It compiles every `src/*.c`, and binds every interface package that
//! `nros generate c` resolved from `package.xml` (transitively: an action leaf
//! naming `example_interfaces` also gets `action_msgs`,
//! `unique_identifier_msgs`, `builtin_interfaces`). So a leaf differs from its
//! siblings in `package.xml` and its own C file, and nowhere in here.
//!
//! Three inputs, none of them a guessed path:
//!
//!  * the nano-ros C headers, the platform ABI headers and the per-build
//!    `nros_config_generated.h` — `DEP_NROS_C_INCLUDE`,
//!    `DEP_NROS_C_PLATFORM_INCLUDE` and `DEP_NROS_C_CONFIG_INCLUDE`, all three
//!    published by `nros-c`'s own build script over its `links = "nros_c"` key.
//!    Using the cargo channel rather than
//!    `$NROS_REPO_DIR/packages/api/nros-c/include` is not tidiness: `links` is
//!    also what makes cargo run THIS script after the one that writes the config
//!    header, instead of racing it.
//!  * the generated C bindings — emitted into `OUT_DIR` by `nros generate c`
//!    from this package's `package.xml`. They go to `OUT_DIR`, not to the leaf's
//!    `generated/`, because that directory belongs to `nros sync`'s RUST output
//!    and the two must not fight over it.
//!  * the C compiler for the board's triple — `CC_thumbv7m_none_eabi`, which
//!    the board descriptor's `cargo_config` provides.

use std::{
    path::{Path, PathBuf},
    process::Command,
};

/// The `.c` files directly in `dir`, sorted so an archive's member order is a
/// function of the tree and not of readdir(2).
fn c_sources(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut sources: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "c"))
        .collect();
    sources.sort();
    sources
}

fn main() {
    let leaf = std::env::var("CARGO_PKG_NAME").expect("CARGO_PKG_NAME");
    let lib = leaf.replace('-', "_");
    println!("cargo:rerun-if-changed=src");
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

    let out_dir = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR"));
    let generated = out_dir.join("generated");

    // ---- generated message bindings -------------------------------------
    //
    // `nros generate c` writes plain `.h`/`.c` — the same pack the cmake road's
    // `nros_generate_interfaces(LANGUAGE c)` compiles into a library.
    //
    // `NROS_CLI` then PATH, which is the first two rungs of the resolution order
    // `nros_cli_bin()` in `scripts/build/cargo.sh` documents. Not a new variable
    // for "which nros binary": there is one, and the leaf uses it.
    //
    // `--ros-edition humble` pairs with the `ros-humble` feature this leaf names
    // on `nros-c` (RFC-0056: the edition drives the keyexpr format, which must
    // match the codegen-baked type_hash — a mismatch links, boots, and fails to
    // interoperate). The two spellings sit in this leaf's two files and nowhere
    // else, which is the same arrangement every `rust/` sibling has.
    let tool = std::env::var("NROS_CLI").unwrap_or_else(|_| "nros".to_string());
    let status = Command::new(&tool)
        .args(["generate", "c", "--force", "--ros-edition", "humble"])
        .arg("--manifest")
        .arg("package.xml")
        .arg("-o")
        .arg(&generated)
        .status()
        .unwrap_or_else(|e| {
            panic!(
                "{leaf}: could not run `{tool} generate c` ({e}). Build the CLI and \
                 put it on PATH: `just setup-cli` then `source ./activate.sh`, or \
                 set NROS_CLI to the `nros` binary."
            )
        });
    assert!(
        status.success(),
        "{leaf}: `{tool} generate c` failed ({status}). The C bindings for this \
         package's `<depend>` list could not be generated."
    );

    // Every interface package the generator resolved, sorted — one directory
    // each, with the package's umbrella header at its root.
    let mut packages: Vec<PathBuf> = std::fs::read_dir(&generated)
        .unwrap_or_else(|e| panic!("{leaf}: {}: {e}", generated.display()))
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    packages.sort();

    // ---- compile ---------------------------------------------------------
    let nros_include = std::env::var("DEP_NROS_C_INCLUDE")
        .expect("DEP_NROS_C_INCLUDE — nros-c must carry `links = \"nros_c\"`");
    let config_include = std::env::var("DEP_NROS_C_CONFIG_INCLUDE")
        .expect("DEP_NROS_C_CONFIG_INCLUDE — nros-c must carry `links = \"nros_c\"`");
    let platform_include = std::env::var("DEP_NROS_C_PLATFORM_INCLUDE")
        .expect("DEP_NROS_C_PLATFORM_INCLUDE — nros-c must carry `links = \"nros_c\"`");

    // Watch the CONTENT of the per-build config header, never the variable that
    // names it (issue 0491: one directory has several spellings, and cargo
    // compares an env value as text). This is what makes a `nros-c` rebuild that
    // moves a size or the codegen version re-run this script, instead of leaving
    // a museum `generated/` tree in OUT_DIR — the 1018/1360 class, whose
    // fail-closed backstop is the `#error` in every generated header.
    println!("cargo:rerun-if-changed={config_include}/nros/nros_config_generated.h");

    // TWO compiles, and the split is about ownership rather than flags: the
    // authored TU is linted, the GENERATED bindings are not. cc-rs turns on
    // `-Wall -Wextra`, and `nros generate c`'s `Char` pack trips
    // `-Wpointer-sign` (`char*` into a `uint8_t*` parameter) once per build —
    // dozens of types' worth of noise over a diagnostic nobody here can act on,
    // which is exactly how a real warning gets missed. Generated code is not
    // ours to fix from a leaf (CLAUDE.md: don't modify vendored/generated).
    let mut build = cc::Build::new();
    build
        // BEFORE the crate's own `include/`, and the order is load-bearing:
        // `nros-c/include/nros/nros_config_generated.h` is a COMMITTED STUB that
        // deliberately defines nothing, so whichever directory comes first wins.
        // With the stub first, every generated message header hits its own
        // fail-closed `#ifndef NROS_CODEGEN_VERSION / #error` — a clear message
        // that names the wrong cause ("rebuild the nano-ros runtime"), because
        // the runtime is built and the include path is what is wrong. Same
        // ordering the cmake road establishes by putting the mirrored header
        // dir ahead of `include/` on `nros_c-static`'s INTERFACE.
        .include(&config_include)
        .include(&nros_include)
        .include(&platform_include)
        .include(&generated);
    // `#include "std_msgs.h"` — each package's umbrella header, the way every C
    // example in the tree spells it.
    for pkg in &packages {
        build.include(pkg);
    }
    build
        .std("c11")
        // Freestanding: this image has no libc. The string/heap helpers the
        // application and the generated bindings call (`memset`, `memcpy`,
        // `malloc`) come from `nros-baremetal-common`'s `libc-stubs` /
        // `libc-heap`, which the board's platform crate enables by default.
        .flag("-ffreestanding");
    // The nano-ros cc policy (issue 1542): strict declarations as errors, plus
    // the gcc-safe frame pointer (issue 0478) — `strict_decls` applies both.
    // Applied BEFORE the clone below, so the generated bindings take it too:
    // `warnings(false)` only drops `-Wall -Wextra`, never these two `-Werror=`.
    nros_cc_flags::strict_decls(&mut build);

    let mut bindings = build.clone();

    let app_sources = c_sources(Path::new("src"));
    assert!(
        !app_sources.is_empty(),
        "{leaf}: no `src/*.c` — this leaf's application is C, and there is none to compile."
    );
    for src in app_sources {
        build.file(src);
    }
    nros_cc_flags::header_deps::track_header_deps(&mut build);
    build.compile(&format!("{lib}_app"));

    bindings.warnings(false);
    let mut any = false;
    for pkg in &packages {
        for sub in ["msg", "srv", "action"] {
            for src in c_sources(&pkg.join(sub)) {
                bindings.file(src);
                any = true;
            }
        }
    }
    // `nros generate c` said it generated, so an EMPTY set means the output moved
    // and the image would fail at link on a `*_publish` symbol instead — a
    // message about the application, four frames from the cause.
    assert!(
        any,
        "{leaf}: `nros generate c` produced no `.c` under {}/<pkg>/{{msg,srv,action}} \
         — the bindings layout changed, or the generator wrote somewhere else.",
        generated.display()
    );
    nros_cc_flags::header_deps::track_header_deps(&mut bindings);
    bindings.compile(&format!("{lib}_msgs"));
    // issue 1580 — every file both compiles OPENED: the generated message
    // headers, the nros-c headers and the per-build config header, beyond the
    // `src/` tree and the one config header this script names by hand.
    nros_cc_flags::header_deps::emit_header_deps(&out_dir);
}
