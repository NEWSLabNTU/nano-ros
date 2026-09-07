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
//!
//! ## phase-420 W5 — the descriptor carries only what convention cannot produce
//!
//! RFC-0087 D4. Five of the fields this script used to READ were convention
//! written longhand: `names` restated the `<nano_ros_provides>` announcements
//! next door, and `cargo_feature` / `cmake_value` / `c_define_token` /
//! `cffi_feature` restated `rmw-<name>` / `<name>` / `UPPER(<name>)` /
//! `<cargo_feature>-cffi`. They are now DERIVED here, from
//! `src/derived_descriptor.rs` — the same file the library compiles, so the
//! generator and any other reader cannot disagree.
//!
//! A descriptor may still STATE one (an out-of-tree backend written against
//! the old shape), and then it must equal what the convention produces: the
//! script panics on a disagreement rather than silently preferring one, which
//! is the belt to `check-derived-descriptor-fields`'s braces. So deleting a
//! derivable field from a descriptor cannot change the generated table, which
//! is exactly W5's acceptance.
//!
//! `cpp_define` is NOT derived and stays authored: its spellings are
//! inconsistent across backends by history (`_CFFI` on two, bare on two) and
//! consumers `#if` on them.

use std::{
    env, fs,
    path::{Path, PathBuf},
};

// One implementation of the conventions, shared with the library (which
// declares the same file as `pub mod derived_descriptor`). A build script
// cannot depend on its own crate, and a second copy of `format!("{kind}-{name}")`
// is exactly the second spelling this wave exists to delete.
#[path = "src/derived_descriptor.rs"]
mod derived_descriptor;

/// The provider kind this script lowers. `cargo_feature` is `<kind>-<name>`.
const KIND: &str = "rmw";

// phase-421 W4 — the descriptor parser + the derivations, shared VERBATIM with
// the library rather than re-implemented here. See the module's own header for
// why it is std-only.
include!("src/serdes_descriptor.rs");

// phase-439 W4 — same arrangement for the RMW descriptor, which until this wave
// was parsed ONLY here, privately. That is what made the in-tree table the only
// answer to "which backends exist" and left an out-of-tree provider discoverable
// and not dispatchable (issue 1214). `resolve_rmw_in` reads a descriptor through
// these same functions at selection time.
include!("src/rmw_descriptor.rs");

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
                    // phase-420 W5 — the sibling `package.xml` is now an INPUT:
                    // it announces the provider's names. Watch it, or adding an
                    // alias would leave a stale table behind a cargo that
                    // thinks it is up to date.
                    let pkg_xml = pkg.join("package.xml");
                    println!("cargo:rerun-if-changed={}", pkg_xml.display());
                    descriptors
                        .push((toml_path.display().to_string(), parse(&toml_path, &pkg_xml)));
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

/// One in-tree backend, as the generated `RMW_ROWS` table sees it: what the
/// descriptor STATES ([`RmwDescriptor`]) plus what convention DERIVES from the
/// sibling `package.xml`.
struct Descriptor {
    names: Vec<String>,
    cargo_feature: String,
    cmake_value: String,
    c_define_token: String,
    cffi_feature: String,
    stated: RmwDescriptor,
}

/// Read one in-tree backend's descriptor into the shape `render` emits.
///
/// The PARSE is `rmw_descriptor::parse_rmw_descriptor`, shared verbatim with
/// the library (phase-439 W4) — this script used to own a private copy, which
/// is why the only readers of `nros-rmw.toml` were in-tree ones compiled into
/// the binary. The DERIVATIONS below are phase-420 W5's: what convention can
/// produce is produced here, and a descriptor that restates one must agree
/// with it.
fn parse(path: &Path, pkg_xml: &Path) -> Descriptor {
    let text = fs::read_to_string(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    let stated =
        parse_rmw_descriptor(&text, &path.display().to_string()).unwrap_or_else(|e| panic!("{e}"));
    let mut d = Descriptor {
        names: stated.stated_names.clone(),
        cargo_feature: stated.stated_cargo_feature.clone(),
        cmake_value: stated.stated_cmake_value.clone(),
        c_define_token: stated.stated_c_define_token.clone(),
        cffi_feature: stated.stated_cffi_feature.clone(),
        stated,
    };
    derive_fields(path, pkg_xml, &mut d);
    d
}

/// Fill in the five convention fields, and refuse a stated value that
/// disagrees with the convention (RFC-0087 D4).
///
/// The order of evidence for `names` is the RFC's: the `<nano_ros_provides>`
/// announcements next to the descriptor are the source. A descriptor with no
/// `package.xml` beside it falls back to a stated `names`, because
/// `check-provider-announcements` deliberately skips such a provider — the
/// migration proceeds one provider at a time and an unmigrated one must keep
/// building.
fn derive_fields(path: &Path, pkg_xml: &Path, d: &mut Descriptor) {
    let announced = fs::read_to_string(pkg_xml)
        .map(|xml| derived_descriptor::announced_names(&xml, KIND))
        .unwrap_or_default();

    if !announced.is_empty() {
        agree(path, "names", &d.names.join(","), &announced.join(","));
        d.names = announced;
    } else if d.names.is_empty() {
        panic!(
            "{}: no <nano_ros_provides kind=\"{KIND}\"/> in {} and no [rmw].names \
             — nothing could resolve to it",
            path.display(),
            pkg_xml.display()
        );
    }

    let canonical = d.names[0].clone();
    let cargo_feature = derived_descriptor::cargo_feature(KIND, &canonical);
    agree(path, "cargo_feature", &d.cargo_feature, &cargo_feature);
    d.cargo_feature = cargo_feature;

    let cmake_value = derived_descriptor::cmake_value(&canonical);
    agree(path, "cmake_value", &d.cmake_value, &cmake_value);
    d.cmake_value = cmake_value;

    let token = derived_descriptor::c_define_token(&canonical);
    agree(path, "c_define_token", &d.c_define_token, &token);
    d.c_define_token = token;

    let cffi = derived_descriptor::cffi_feature(&d.cargo_feature);
    agree(path, "cffi_feature", &d.cffi_feature, &cffi);
    d.cffi_feature = cffi;

    if d.stated.cpp_define.is_empty() {
        // Not derivable — see the module header. An absent one would lower to
        // an empty `#define`, which compiles and means nothing.
        panic!(
            "{}: [rmw].cpp_define is missing — it is the one non-derivable \
             lowering (spellings are inconsistent across backends by history \
             and consumers `#if` on them), so it must be authored",
            path.display()
        );
    }
}

/// A descriptor may restate a derivable field; it may not disagree with it.
///
/// One rule, one implementation: `rmw_descriptor::agree_with_convention` is the
/// shared form, and this is the build script's panicking wrapper around it —
/// the belt to `check-derived-descriptor-fields`'s braces, firing for anyone who
/// builds rather than only for `check-fast`.
fn agree(path: &Path, field: &str, stated: &str, derived: &str) {
    if let Err(e) = agree_with_convention(&path.display().to_string(), field, stated, derived) {
        panic!("{e}");
    }
}

fn render(ds: &[(String, Descriptor)]) -> String {
    let mut out = String::from(
        "// @generated by build.rs from packages/rmw/*/*/nros-rmw.toml — DO NOT EDIT.\n\
         // phase-347 W3: the descriptors are the source; this is their Rust lowering.\n\n",
    );
    out.push_str("pub(crate) struct RmwRow {\n    pub declared: &'static str,\n    pub names: &'static [&'static str],\n    pub cargo_feature: &'static str,\n    pub cmake_value: &'static str,\n    pub c_define_token: &'static str,\n    pub cffi_feature: &'static str,\n    pub cpp_define: &'static str,\n    pub cmake_target: &'static str,\n    pub rlib_dep: &'static str,\n    pub link_strategy: &'static str,\n    pub c_cffi_feature: &'static str,\n    pub needs_cxx_linker: bool,\n    pub capabilities: &'static [(&'static str, &'static str)],\n    pub per_message_hook: &'static str,\n}\n\n");
    out.push_str("pub(crate) static RMW_ROWS: &[RmwRow] = &[\n");
    for (_, d) in ds {
        let names = d
            .names
            .iter()
            .map(|n| format!("{n:?}"))
            .collect::<Vec<_>>()
            .join(", ");
        let caps = d
            .stated
            .capabilities
            .iter()
            .map(|(k, v)| format!("({k:?}, {v:?})"))
            .collect::<Vec<_>>()
            .join(", ");
        out.push_str(&format!(
            "    RmwRow {{\n        declared: {:?},\n        names: &[{names}],\n        cargo_feature: {:?},\n        cmake_value: {:?},\n        c_define_token: {:?},\n        cffi_feature: {:?},\n        cpp_define: {:?},\n        cmake_target: {:?},\n        rlib_dep: {:?},\n        link_strategy: {:?},\n        c_cffi_feature: {:?},\n        needs_cxx_linker: {},\n        capabilities: &[{caps}],\n        per_message_hook: {:?},\n    }},\n",
            d.names[0],
            d.cargo_feature,
            d.cmake_value,
            d.c_define_token,
            d.cffi_feature,
            d.stated.cpp_define,
            d.stated.cmake_target,
            d.stated.rlib_dep,
            d.stated.link_strategy,
            d.stated.c_cffi_feature,
            d.stated.needs_cxx_linker,
            d.stated.per_message_hook,
            caps = caps,
        ));
    }
    out.push_str("];\n");
    out
}

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
