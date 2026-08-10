//! phase-347 W3 — the RMW table is GENERATED FROM THE DESCRIPTORS.
//!
//! RFC-0071: a backend declares itself in `nros-rmw.toml`; nothing central
//! enumerates backends. This build script globs the in-tree descriptors and
//! emits the table `rmw_resolver.rs` used to hold as a hand-written `match`
//! (`KNOWN_RMW`, `canonical_rmw`, `resolve_rmw`).
//!
//! Compile-time rather than runtime on purpose:
//!
//! * the resolver's public API returns `&'static str` and is called from ~29
//!   sites; keeping that shape means no ripple, and a generated `const` table
//!   is `'static` for free;
//! * `cargo-nano-ros` is the LOWER crate of the CLI and has no business
//!   locating a repo root at runtime.
//!
//! The cost is the honest one: **only IN-TREE backends are found here.** An
//! out-of-tree backend needs runtime discovery over a workspace search path,
//! which is phase-348 and deliberately not this wave.

use std::{
    env, fs,
    path::{Path, PathBuf},
};

fn main() {
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    // packages/cli/cargo-nano-ros -> packages
    let packages = manifest
        .ancestors()
        .nth(2)
        .expect("cargo-nano-ros lives at packages/cli/cargo-nano-ros")
        .to_path_buf();
    let rmw_root = packages.join("rmw");

    let mut descriptors: Vec<(String, Descriptor)> = Vec::new();
    if let Ok(families) = fs::read_dir(&rmw_root) {
        let mut fams: Vec<PathBuf> = families.flatten().map(|e| e.path()).collect();
        fams.sort();
        for family in fams {
            let Ok(pkgs) = fs::read_dir(&family) else {
                continue;
            };
            let mut ps: Vec<PathBuf> = pkgs.flatten().map(|e| e.path()).collect();
            ps.sort();
            for pkg in ps {
                let toml_path = pkg.join("nros-rmw.toml");
                if toml_path.is_file() {
                    println!("cargo:rerun-if-changed={}", toml_path.display());
                    descriptors.push((toml_path.display().to_string(), parse(&toml_path)));
                }
            }
        }
    }
    // A new backend dir must re-run this script, not just a changed file.
    println!("cargo:rerun-if-changed={}", rmw_root.display());

    if descriptors.is_empty() {
        panic!(
            "no nros-rmw.toml found under {} — refusing to generate an EMPTY \
             backend table, which would make every `rmw = ...` unresolvable \
             while looking like a working build (phase-347 W3)",
            rmw_root.display()
        );
    }

    // Two descriptors claiming one name is ambiguous resolution; fail loudly at
    // build time rather than silently taking whichever sorted first.
    let mut seen: Vec<(String, String)> = Vec::new();
    for (path, d) in &descriptors {
        for n in &d.names {
            if let Some((other, _)) = seen.iter().find(|(name, _)| name == n) {
                panic!("two descriptors claim rmw name `{n}`: {other} and {path}");
            }
            seen.push((n.clone(), path.clone()));
        }
    }

    let out = PathBuf::from(env::var("OUT_DIR").unwrap()).join("rmw_table.rs");
    fs::write(&out, render(&descriptors)).expect("write rmw_table.rs");
}

struct Descriptor {
    names: Vec<String>,
    cargo_feature: String,
    cmake_value: String,
    c_define_token: String,
    cffi_feature: String,
    cpp_define: String,
    cmake_target: String,
    rlib_dep: String,
    extra_link_libs: Vec<String>,
    needs_cxx_linker: bool,
    /// `[rmw.capabilities]` — capability name -> THIS backend's own feature.
    capabilities: Vec<(String, String)>,
}

/// Minimal line parser for the descriptor's flat `key = value` shape.
///
/// Deliberately NOT the `toml` crate: it is a normal dependency, not a
/// build-dependency, and adding one would move `Cargo.lock` — which this repo
/// only permits through `just lock-update`. The descriptor format is flat and
/// under our control, and `NanoRosCapabilities.cmake` already reads
/// `nros-board.toml` exactly this way (`file(STRINGS ... REGEX ...)`), so this
/// is the established idiom rather than a shortcut.
///
/// Only the `[rmw]` and `[rmw.link]` tables are read; `[rmw.capabilities]`,
/// `[rmw.provides.*]` and `[rmw.codegen]` are for later waves and are ignored
/// here rather than half-parsed.
fn parse(path: &Path) -> Descriptor {
    let text = fs::read_to_string(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    let mut section = String::new();
    let mut d = Descriptor {
        names: Vec::new(),
        cargo_feature: String::new(),
        cmake_value: String::new(),
        c_define_token: String::new(),
        cffi_feature: String::new(),
        cpp_define: String::new(),
        cmake_target: String::new(),
        rlib_dep: String::new(),
        extra_link_libs: Vec::new(),
        needs_cxx_linker: false,
        capabilities: Vec::new(),
    };
    for raw in text.lines() {
        let line = raw.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            section = name.to_string();
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let (key, value) = (key.trim(), value.trim());
        let scalar = || value.trim_matches('"').to_string();
        let list = || {
            value
                .trim_start_matches('[')
                .trim_end_matches(']')
                .split(',')
                .map(|x| x.trim().trim_matches('"').to_string())
                .filter(|x| !x.is_empty())
                .collect::<Vec<_>>()
        };
        match (section.as_str(), key) {
            ("rmw", "names") => d.names = list(),
            ("rmw", "cargo_feature") => d.cargo_feature = scalar(),
            ("rmw", "cmake_value") => d.cmake_value = scalar(),
            ("rmw", "c_define_token") => d.c_define_token = scalar(),
            ("rmw", "cffi_feature") => d.cffi_feature = scalar(),
            ("rmw", "cpp_define") => d.cpp_define = scalar(),
            ("rmw", "cmake_target") => d.cmake_target = scalar(),
            ("rmw.link", "rlib_dep") => d.rlib_dep = scalar(),
            ("rmw.link", "extra_link_libs") => d.extra_link_libs = list(),
            ("rmw.link", "needs_cxx_linker") => d.needs_cxx_linker = value == "true",
            // Open vocabulary by design (RFC-0071 D6 / the platform
            // descriptor's precedent): whatever the backend declares is a
            // capability it offers, and core never learns the right-hand side.
            ("rmw.capabilities", k) => d.capabilities.push((k.to_string(), scalar())),
            _ => {}
        }
    }
    if d.names.is_empty() {
        panic!(
            "{}: [rmw].names is empty — nothing could resolve to it",
            path.display()
        );
    }
    d
}

fn render(ds: &[(String, Descriptor)]) -> String {
    let mut out = String::from(
        "// @generated by build.rs from packages/rmw/*/*/nros-rmw.toml — DO NOT EDIT.\n\
         // phase-347 W3: the descriptors are the source; this is their Rust lowering.\n\n",
    );
    out.push_str("pub(crate) struct RmwRow {\n    pub declared: &'static str,\n    pub names: &'static [&'static str],\n    pub cargo_feature: &'static str,\n    pub cmake_value: &'static str,\n    pub c_define_token: &'static str,\n    pub cffi_feature: &'static str,\n    pub cpp_define: &'static str,\n    pub cmake_target: &'static str,\n    pub rlib_dep: &'static str,\n    pub extra_link_libs: &'static [&'static str],\n    pub needs_cxx_linker: bool,\n    pub capabilities: &'static [(&'static str, &'static str)],\n}\n\n");
    out.push_str("pub(crate) static RMW_ROWS: &[RmwRow] = &[\n");
    for (_, d) in ds {
        let names = d
            .names
            .iter()
            .map(|n| format!("{n:?}"))
            .collect::<Vec<_>>()
            .join(", ");
        let caps = d
            .capabilities
            .iter()
            .map(|(k, v)| format!("({k:?}, {v:?})"))
            .collect::<Vec<_>>()
            .join(", ");
        let libs = d
            .extra_link_libs
            .iter()
            .map(|n| format!("{n:?}"))
            .collect::<Vec<_>>()
            .join(", ");
        out.push_str(&format!(
            "    RmwRow {{\n        declared: {:?},\n        names: &[{names}],\n        cargo_feature: {:?},\n        cmake_value: {:?},\n        c_define_token: {:?},\n        cffi_feature: {:?},\n        cpp_define: {:?},\n        cmake_target: {:?},\n        rlib_dep: {:?},\n        extra_link_libs: &[{libs}],\n        needs_cxx_linker: {},\n        capabilities: &[{caps}],\n    }},\n",
            d.names[0],
            d.cargo_feature,
            d.cmake_value,
            d.c_define_token,
            d.cffi_feature,
            d.cpp_define,
            d.cmake_target,
            d.rlib_dep,
            d.needs_cxx_linker,
            caps = caps,
        ));
    }
    out.push_str("];\n");
    out
}
