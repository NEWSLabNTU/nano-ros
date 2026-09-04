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

// phase-421 W4 — the descriptor parser + the derivations, shared VERBATIM with
// the library rather than re-implemented here. See the module's own header for
// why it is std-only.
include!("src/serdes_descriptor.rs");

fn main() {
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    // packages/cli/cargo-nano-ros -> packages
    let packages = manifest
        .ancestors()
        .nth(2)
        .expect("cargo-nano-ros lives at packages/cli/cargo-nano-ros")
        .to_path_buf();
    println!("cargo:rerun-if-changed=src/serdes_descriptor.rs");
    generate_serdes_table(&packages);
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
    /// `[rmw.codegen].per_message` — a cmake command run per message type.
    per_message_hook: String,
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
        per_message_hook: String::new(),
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
            ("rmw.codegen", "per_message") => d.per_message_hook = scalar(),
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
    out.push_str("pub(crate) struct RmwRow {\n    pub declared: &'static str,\n    pub names: &'static [&'static str],\n    pub cargo_feature: &'static str,\n    pub cmake_value: &'static str,\n    pub c_define_token: &'static str,\n    pub cffi_feature: &'static str,\n    pub cpp_define: &'static str,\n    pub cmake_target: &'static str,\n    pub rlib_dep: &'static str,\n    pub extra_link_libs: &'static [&'static str],\n    pub needs_cxx_linker: bool,\n    pub capabilities: &'static [(&'static str, &'static str)],\n    pub per_message_hook: &'static str,\n}\n\n");
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
            "    RmwRow {{\n        declared: {:?},\n        names: &[{names}],\n        cargo_feature: {:?},\n        cmake_value: {:?},\n        c_define_token: {:?},\n        cffi_feature: {:?},\n        cpp_define: {:?},\n        cmake_target: {:?},\n        rlib_dep: {:?},\n        extra_link_libs: &[{libs}],\n        needs_cxx_linker: {},\n        capabilities: &[{caps}],\n        per_message_hook: {:?},\n    }},\n",
            d.names[0],
            d.cargo_feature,
            d.cmake_value,
            d.c_define_token,
            d.cffi_feature,
            d.cpp_define,
            d.cmake_target,
            d.rlib_dep,
            d.needs_cxx_linker,
            d.per_message_hook,
            caps = caps,
        ));
    }
    out.push_str("];\n");
    out
}

// ===========================================================================
// phase-421 W4 — the serdes table (RFC-0088 D6)
// ===========================================================================

/// One in-tree serdes provider, as the generated table sees it.
struct SerdesEntry {
    names: Vec<String>,
    crate_name: String,
    descriptor: SerdesDescriptor,
    descriptor_path: String,
}

/// Emit `serdes_table.rs` from `packages/*/*/nros-serdes.toml`.
///
/// Same shape as the RMW table above and for the same reasons — the descriptors
/// are the source, nothing central enumerates providers, and the resolver's
/// answers are `'static` for free. One difference, and it is the RFC-0087 D4
/// point: **the NAMES are not in the descriptor.** They come from the sibling
/// `package.xml`'s `<nano_ros_provides kind="serdes" …/>`, because the
/// announcement already carries them and a second spelling drifts.
///
/// Same honest cost too: only IN-TREE providers are found here. An out-of-repo
/// provider is resolved at selection time over the provider search path — see
/// `serdes_resolver::resolve_serdes_in`.
fn generate_serdes_table(packages: &Path) {
    let mut entries: Vec<SerdesEntry> = Vec::new();

    let mut families: Vec<PathBuf> = fs::read_dir(packages)
        .unwrap_or_else(|e| panic!("read_dir {}: {e}", packages.display()))
        .flatten()
        .map(|e| e.path())
        .collect();
    families.sort();
    for family in families {
        let Ok(pkgs) = fs::read_dir(&family) else {
            continue;
        };
        let mut ps: Vec<PathBuf> = pkgs.flatten().map(|e| e.path()).collect();
        ps.sort();
        for pkg in ps {
            let toml_path = pkg.join("nros-serdes.toml");
            if !toml_path.is_file() {
                continue;
            }
            println!("cargo:rerun-if-changed={}", toml_path.display());
            entries.push(read_serdes_provider(&pkg, &toml_path));
        }
        // A new provider DIRECTORY must re-run this script, not just a changed
        // file — same rule the RMW root gets below.
        println!("cargo:rerun-if-changed={}", family.display());
    }
    println!("cargo:rerun-if-changed={}", packages.display());

    if entries.is_empty() {
        panic!(
            "no nros-serdes.toml found under {} — refusing to generate an EMPTY \
             serdes table, which would make every `serdes = ...` unresolvable \
             (including the `cdr` default) while looking like a working build \
             (phase-421 W4)",
            packages.display()
        );
    }

    // Two providers claiming one format name is ambiguous resolution; fail
    // loudly rather than taking whichever sorted first.
    let mut seen: Vec<(String, String)> = Vec::new();
    for e in &entries {
        for n in &e.names {
            if let Some((_, other)) = seen.iter().find(|(name, _)| name == n) {
                panic!(
                    "two packages announce serdes name `{n}`: {other} and {}",
                    e.descriptor_path
                );
            }
            seen.push((n.clone(), e.descriptor_path.clone()));
        }
    }

    let out = PathBuf::from(env::var("OUT_DIR").unwrap()).join("serdes_table.rs");
    fs::write(&out, render_serdes(&entries)).expect("write serdes_table.rs");
}

/// Read one provider: the announcement (names), the manifest (crate) and the
/// descriptor (the two non-derivable fields).
fn read_serdes_provider(pkg: &Path, toml_path: &Path) -> SerdesEntry {
    let origin = toml_path.display().to_string();

    let manifest = pkg.join("package.xml");
    println!("cargo:rerun-if-changed={}", manifest.display());
    let xml = fs::read_to_string(&manifest).unwrap_or_else(|e| {
        panic!(
            "{origin} has no readable sibling package.xml ({}): {e} — a descriptor \
             is read only for the provider that was SELECTED, and selection goes \
             through the <nano_ros_provides kind=\"serdes\"/> announcement, so a \
             descriptor with no announcement can never be reached (RFC-0087 D4)",
            manifest.display()
        )
    });
    let names = package_xml_provides(&xml, "serdes");
    if names.is_empty() {
        panic!(
            "{} announces no <nano_ros_provides kind=\"serdes\" name=\"…\"/>, but \
             {origin} exists beside it — the descriptor carries no `names` on \
             purpose (RFC-0087 D4), so nothing could resolve to this provider",
            manifest.display()
        );
    }

    let cargo_toml = pkg.join("Cargo.toml");
    println!("cargo:rerun-if-changed={}", cargo_toml.display());
    let crate_name = fs::read_to_string(&cargo_toml)
        .ok()
        .as_deref()
        .and_then(cargo_package_name)
        .unwrap_or_else(|| {
            panic!(
                "{}: no [package] name — the serdes `crate` is DERIVED from the \
                 sibling Cargo.toml and there is nothing to derive it from",
                cargo_toml.display()
            )
        });

    let text = fs::read_to_string(toml_path)
        .unwrap_or_else(|e| panic!("read {}: {e}", toml_path.display()));
    let descriptor = parse_serdes_descriptor(&text, &origin).unwrap_or_else(|e| panic!("{e}"));

    SerdesEntry {
        names,
        crate_name,
        descriptor,
        descriptor_path: origin,
    }
}

fn render_serdes(entries: &[SerdesEntry]) -> String {
    let mut out = String::from(
        "// @generated by build.rs from packages/*/*/nros-serdes.toml plus each\n\
         // provider's <nano_ros_provides kind=\"serdes\"/> — DO NOT EDIT.\n\
         // phase-421 W4: the announcement is the source of the NAMES, the\n\
         // descriptor of what cannot be derived, and everything else is derived.\n\n",
    );
    out.push_str(
        "pub(crate) struct SerdesRow {\n    \
         pub declared: &'static str,\n    \
         pub names: &'static [&'static str],\n    \
         pub crate_name: &'static str,\n    \
         pub cargo_feature: &'static str,\n    \
         pub cmake_value: &'static str,\n    \
         pub c_define_token: &'static str,\n    \
         pub impl_strategy: &'static str,\n    \
         pub format_id: Option<u8>,\n\
         }\n\n",
    );
    out.push_str("pub(crate) static SERDES_ROWS: &[SerdesRow] = &[\n");
    for e in entries {
        let declared = &e.names[0];
        let names = e
            .names
            .iter()
            .map(|n| format!("{n:?}"))
            .collect::<Vec<_>>()
            .join(", ");
        let format_id = match e.descriptor.format_id {
            Some(id) => format!("Some({id})"),
            None => "None".to_string(),
        };
        out.push_str(&format!(
            "    SerdesRow {{\n        \
             declared: {:?},\n        \
             names: &[{names}],\n        \
             crate_name: {:?},\n        \
             cargo_feature: {:?},\n        \
             cmake_value: {:?},\n        \
             c_define_token: {:?},\n        \
             impl_strategy: {:?},\n        \
             format_id: {format_id},\n    \
             }},\n",
            declared,
            e.crate_name,
            serdes_cargo_feature(declared),
            serdes_cmake_value(declared),
            serdes_c_define_token(declared),
            e.descriptor.impl_strategy,
        ));
    }
    out.push_str("];\n");
    out
}
