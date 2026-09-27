//! phase-469 W1 — the message packs are DISCOVERED, not authored.
//!
//! `render.rs` used to carry 28 `include_str!` rows and `generator::naming.rs`
//! two `match` tables over a hand-written `Surface` enum, so a new language
//! surface was three Rust edits in two files plus a `.jinja` directory. Here the
//! directory IS the declaration: every `packs/<pack>/pack.toml` is read at build
//! time and the registry plus the naming surfaces are generated from it.
//!
//! ## Why a build script rather than a directory read at run time
//!
//! Same reason the templates are `include_str!`'d in the first place, and the
//! same choice `codegen/entry/pack.rs` makes: `nros` is invoked from CMake and
//! from build scripts in trees that do not contain this checkout, so a manifest
//! resolved from disk at run time would be a path dependency the caller cannot
//! satisfy. Reading them at BUILD time keeps the packs bundled while making the
//! list derived.
//!
//! ## Two invariants this file must not break
//!
//! 1. **The registry's ORDER is load-bearing.** `codegen_fingerprint` (RFC-0061
//!    / phase-335 W4.a) hashes `bundled_packs()` as a sequence, and every
//!    fixture in the tree is keyed on that hash — so reordering the rows
//!    re-stales the whole tree for output that is byte-identical. The order
//!    therefore comes from data: `registry_order` per pack, then the `templates`
//!    array's own order inside a pack. The numbers are the order the authored
//!    list had.
//! 2. **A malformed or incomplete manifest must FAIL THE BUILD**, loudly and
//!    naming the file. A pack that emits nothing and looks wired is the failure
//!    mode `check-entry-pack-conformance` exists for; here the compiler can
//!    refuse it outright, which is earlier.

use std::{collections::BTreeSet, fmt::Write as _, path::Path};

/// One `templates` row: the stable registry key and the file beside the manifest.
struct Template {
    key: String,
    file: String,
}

/// One `packs/<dir>/pack.toml`.
struct Pack {
    /// Directory name — the pack's identity, and the `Surface` variant's stem.
    dir: String,
    order: i64,
    shared: bool,
    language: Option<String>,
    header_extension: Option<String>,
    guard_suffix: Option<String>,
    source_extension: Option<String>,
    templates: Vec<Template>,
}

impl Pack {
    /// A pack is an artifact-naming SURFACE exactly when it declares what its
    /// header is called. Deliberately a declared property rather than a name
    /// test: `cpp` is a surface and `rust` is not, and nothing about the two
    /// strings says so.
    fn is_naming_surface(&self) -> bool {
        self.header_extension.is_some()
    }

    /// `c` -> `C`, `cpp` -> `Cpp`, `my_lang` -> `MyLang` — the generated
    /// `Surface` variant.
    ///
    /// Upper-camel per WORD, not just the first letter: a pack directory named
    /// with an underscore would otherwise generate `My_lang`, and
    /// `non_camel_case_types` is a warning — i.e. an ERROR under the `-D
    /// warnings` every `check` lane runs. A new pack's directory name must not
    /// be able to break the build of a crate its author never opened. (Measured
    /// while proving discovery: a throwaway `zz_probe` pack did exactly that.)
    fn variant(&self) -> String {
        self.dir
            .split(['_', '-'])
            .filter(|w| !w.is_empty())
            .map(|w| {
                let mut c = w.chars();
                match c.next() {
                    Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                    None => String::new(),
                }
            })
            .collect()
    }
}

fn fail(what: &str) -> ! {
    panic!("rosidl-codegen build script: {what}");
}

fn str_field(t: &toml::Table, key: &str, ctx: &str) -> Option<String> {
    match t.get(key) {
        None => None,
        Some(toml::Value::String(s)) => Some(s.clone()),
        Some(other) => fail(&format!("{ctx}: `{key}` must be a string, got {other}")),
    }
}

fn read_pack(dir: &Path) -> Pack {
    let name = dir
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or_else(|| fail("a pack directory with an unreadable name"))
        .to_string();
    let manifest = dir.join("pack.toml");
    let ctx = format!("packs/{name}/pack.toml");
    let text = std::fs::read_to_string(&manifest).unwrap_or_else(|e| {
        fail(&format!(
            "{ctx}: a pack directory with no readable manifest ({e}). Every \
             directory under `packs/` must declare itself — a pack nobody \
             describes renders nothing and looks wired."
        ))
    });
    let table: toml::Table = toml::from_str(&text).unwrap_or_else(|e| fail(&format!("{ctx}: {e}")));

    let shared = match table.get("shared") {
        None => false,
        Some(toml::Value::Boolean(b)) => *b,
        Some(other) => fail(&format!("{ctx}: `shared` must be a boolean, got {other}")),
    };
    let order = match table.get("registry_order") {
        Some(toml::Value::Integer(i)) => *i,
        Some(other) => fail(&format!(
            "{ctx}: `registry_order` must be an integer, got {other}"
        )),
        None => fail(&format!(
            "{ctx}: no `registry_order`. The registry's order is hashed by \
             `codegen_fingerprint`, so it cannot be left to directory order."
        )),
    };
    let language = str_field(&table, "language", &ctx);
    let header_extension = str_field(&table, "header_extension", &ctx);
    let guard_suffix = str_field(&table, "guard_suffix", &ctx);
    let source_extension = str_field(&table, "source_extension", &ctx);

    let templates = match table.get("templates") {
        Some(toml::Value::Array(rows)) => rows
            .iter()
            .map(|row| {
                let t = row.as_table().unwrap_or_else(|| {
                    fail(&format!(
                        "{ctx}: every `templates` row must be an inline table \
                         `{{ key = \"…\", file = \"…\" }}`, got {row}"
                    ))
                });
                let key = str_field(t, "key", &ctx)
                    .unwrap_or_else(|| fail(&format!("{ctx}: a `templates` row has no `key`")));
                let file = str_field(t, "file", &ctx).unwrap_or_else(|| {
                    fail(&format!("{ctx}: `templates` row `{key}` has no `file`"))
                });
                if !dir.join(&file).is_file() {
                    fail(&format!(
                        "{ctx}: `templates` row `{key}` names `{file}`, which does \
                         not exist beside the manifest"
                    ));
                }
                Template { key, file }
            })
            .collect(),
        Some(other) => fail(&format!("{ctx}: `templates` must be an array, got {other}")),
        None => fail(&format!("{ctx}: no `templates`")),
    };

    // Kind completeness, the same rule the entry packs carry: a shared pack
    // renders no artifact of its own, so declaring a language pack's fields
    // here makes the two kinds confusable.
    if shared {
        for (field, present) in [
            ("language", language.is_some()),
            ("header_extension", header_extension.is_some()),
            ("guard_suffix", guard_suffix.is_some()),
            ("source_extension", source_extension.is_some()),
        ] {
            if present {
                fail(&format!(
                    "{ctx}: a shared pack declares `{field}`. A shared pack is \
                     partials other packs include; it names no artifact."
                ));
            }
        }
    } else if language.is_none() {
        fail(&format!(
            "{ctx}: a language pack must declare `language` (an \
             `nros_lang::Language` spelling), or `shared = true` if it is only \
             partials."
        ));
    }
    // The naming row is all-or-nothing: a header extension with no guard suffix
    // would generate a surface whose guards collide with another surface's.
    if header_extension.is_some() != guard_suffix.is_some() {
        fail(&format!(
            "{ctx}: `header_extension` and `guard_suffix` are one row — declare \
             both (an artifact-naming surface) or neither."
        ));
    }
    if source_extension.is_some() && header_extension.is_none() {
        fail(&format!(
            "{ctx}: `source_extension` without `header_extension`: a pack that \
             names a translation unit and no header is not a surface \
             `generator::naming` can derive from."
        ));
    }

    Pack {
        dir: name,
        order,
        shared,
        language,
        header_extension,
        guard_suffix,
        source_extension,
        templates,
    }
}

fn main() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let packs_dir = root.join("packs");
    // A directory watch: cargo walks it recursively, so a new pack directory —
    // or an edited `.jinja` — re-runs this script. Watching the directory
    // rather than the files is what makes DISCOVERY work; a list of watched
    // files would be the authored list again, one layer down.
    println!("cargo:rerun-if-changed={}", packs_dir.display());

    let mut dirs: Vec<_> = std::fs::read_dir(&packs_dir)
        .unwrap_or_else(|e| fail(&format!("cannot read {}: {e}", packs_dir.display())))
        .map(|e| {
            e.unwrap_or_else(|e| fail(&format!("bad dir entry: {e}")))
                .path()
        })
        .filter(|p| p.is_dir())
        .collect();
    dirs.sort();
    if dirs.is_empty() {
        fail("no pack directories under `packs/` — refusing to generate an empty registry");
    }

    let mut packs: Vec<Pack> = dirs.iter().map(|d| read_pack(d)).collect();
    packs.sort_by_key(|p| (p.order, p.dir.clone()));

    // `registry_order` decides the hash input, so two packs claiming one
    // position would make the registry depend on directory order after all.
    let mut seen_order = BTreeSet::new();
    for p in &packs {
        if !seen_order.insert(p.order) {
            fail(&format!(
                "two packs declare `registry_order = {}` (one of them is \
                 `packs/{}`) — the registry's order is hashed, so it must be \
                 total.",
                p.order, p.dir
            ));
        }
    }
    let mut seen_key = BTreeSet::new();
    for p in &packs {
        for t in &p.templates {
            if !seen_key.insert(t.key.clone()) {
                fail(&format!(
                    "registry key `{}` is declared twice (again by `packs/{}`) — \
                     the loader resolves the FIRST, so the second template would \
                     never render.",
                    t.key, p.dir
                ));
            }
        }
    }

    // Every `.jinja` on disk must be claimed by some manifest. A file nobody
    // renders parses, looks wired, and emits nothing — the same check the entry
    // side's `every_pack_file_is_registered` makes, moved to build time because
    // here it can refuse to compile.
    let claimed: BTreeSet<String> = packs
        .iter()
        .flat_map(|p| p.templates.iter().map(|t| format!("{}/{}", p.dir, t.file)))
        .collect();
    for d in &dirs {
        let name = d.file_name().unwrap().to_str().unwrap();
        for entry in std::fs::read_dir(d).unwrap_or_else(|e| fail(&format!("read {d:?}: {e}"))) {
            let path = entry
                .unwrap_or_else(|e| fail(&format!("bad entry: {e}")))
                .path();
            if path.extension().is_some_and(|e| e == "jinja") {
                let rel = format!("{name}/{}", path.file_name().unwrap().to_str().unwrap());
                if !claimed.contains(&rel) {
                    fail(&format!(
                        "packs/{rel} is not named by `packs/{name}/pack.toml`, so \
                         nothing renders it."
                    ));
                }
            }
        }
    }

    let mut registry = String::new();
    registry.push_str(
        "// @generated by build.rs from packs/*/pack.toml — do not edit.\n\
         /// Every bundled pack template, keyed by the stable name a\n\
         /// `render(name, …)` call and any `{% import %}` use, in each\n\
         /// manifest's `registry_order`.\n\
         const PACKS: &[(&str, &str)] = &[\n",
    );
    for p in &packs {
        let _ = writeln!(registry, "    // packs/{} (order {})", p.dir, p.order);
        for t in &p.templates {
            let _ = writeln!(
                registry,
                "    (\n        {:?},\n        include_str!(concat!(env!(\"CARGO_MANIFEST_DIR\"), \"/packs/{}/{}\")),\n    ),",
                t.key, p.dir, t.file
            );
        }
    }
    registry.push_str("];\n");

    let surfaces: Vec<&Pack> = packs.iter().filter(|p| p.is_naming_surface()).collect();
    if surfaces.is_empty() {
        fail(
            "no pack declares `header_extension`, so `generator::naming` would \
             have no surface to derive a name for",
        );
    }
    let mut naming = String::new();
    naming.push_str("// @generated by build.rs from packs/*/pack.toml — do not edit.\n");
    naming.push_str(
        "/// The language surface an artifact is emitted for.\n\
         ///\n\
         /// GENERATED from the packs that declare `header_extension` — see the\n\
         /// module docs. Adding a surface is adding a `packs/<dir>/pack.toml`\n\
         /// with its three naming fields; the variant appears here.\n\
         #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]\n\
         pub enum Surface {\n",
    );
    for p in &surfaces {
        let _ = writeln!(naming, "    /// `packs/{}`.\n    {},", p.dir, p.variant());
    }
    naming.push_str("}\n\nimpl Surface {\n");
    naming.push_str("    /// Every naming surface, in `registry_order`.\n");
    let _ = writeln!(
        naming,
        "    pub const ALL: [Surface; {}] = [{}];",
        surfaces.len(),
        surfaces
            .iter()
            .map(|p| format!("Surface::{}", p.variant()))
            .collect::<Vec<_>>()
            .join(", ")
    );
    // Four accessors, each a match generated from one manifest field.
    let arms = |f: &dyn Fn(&Pack) -> String| -> String {
        surfaces
            .iter()
            .map(|p| format!("            Surface::{} => {},\n", p.variant(), f(p)))
            .collect::<String>()
    };
    let _ = write!(
        naming,
        "\n    /// The pack directory this surface's templates live in.\n\
         \x20   pub const fn pack(self) -> &'static str {{\n\
         \x20       match self {{\n{}        }}\n    }}\n",
        arms(&|p| format!("{:?}", p.dir))
    );
    let _ = write!(
        naming,
        "\n    /// Extension of the header this surface emits.\n\
         \x20   const fn header_ext(self) -> &'static str {{\n\
         \x20       match self {{\n{}        }}\n    }}\n",
        arms(&|p| format!("{:?}", p.header_extension.as_ref().unwrap()))
    );
    let _ = write!(
        naming,
        "\n    /// Suffix the include guard carries, so two surfaces' guards for\n\
         \x20   /// one type never collide.\n\
         \x20   const fn guard_suffix(self) -> &'static str {{\n\
         \x20       match self {{\n{}        }}\n    }}\n",
        arms(&|p| format!("{:?}", p.guard_suffix.as_ref().unwrap()))
    );
    let _ = write!(
        naming,
        "\n    /// Extension of the translation unit this surface compiles, when\n\
         \x20   /// it has one. A header-only surface has none.\n\
         \x20   const fn source_ext(self) -> Option<&'static str> {{\n\
         \x20       match self {{\n{}        }}\n    }}\n}}\n",
        arms(&|p| match &p.source_extension {
            Some(e) => format!("Some({e:?})"),
            None => "None".to_string(),
        })
    );

    // The manifests' own facts, for the tests and diagnostics that must read
    // them without re-spelling a pack.toml.
    let mut meta = String::new();
    meta.push_str(
        "// @generated by build.rs from packs/*/pack.toml — do not edit.\n\
         /// One discovered pack, as its `pack.toml` declares it.\n\
         #[derive(Clone, Copy, Debug, PartialEq, Eq)]\n\
         pub struct PackInfo {\n\
         \x20   /// Directory name under `packs/`.\n\
         \x20   pub dir: &'static str,\n\
         \x20   /// Position in the bundled registry (`registry_order`).\n\
         \x20   pub order: i64,\n\
         \x20   /// `true` for a pack of partials other packs include.\n\
         \x20   pub shared: bool,\n\
         \x20   /// The `nros_lang::Language` spelling it declares, if any.\n\
         \x20   pub language: Option<&'static str>,\n\
         \x20   /// Registry keys it contributes, in order.\n\
         \x20   pub templates: &'static [&'static str],\n\
         }\n\n\
         /// Every pack discovered at build time, in `registry_order`.\n\
         pub const PACK_INFO: &[PackInfo] = &[\n",
    );
    for p in &packs {
        let _ = writeln!(
            meta,
            "    PackInfo {{ dir: {:?}, order: {}, shared: {}, language: {}, templates: &[{}] }},",
            p.dir,
            p.order,
            p.shared,
            match &p.language {
                Some(l) => format!("Some({l:?})"),
                None => "None".to_string(),
            },
            p.templates
                .iter()
                .map(|t| format!("{:?}", t.key))
                .collect::<Vec<_>>()
                .join(", ")
        );
    }
    meta.push_str("];\n");

    let out = std::env::var("OUT_DIR").unwrap_or_else(|_| fail("OUT_DIR is unset"));
    let out = Path::new(&out);
    for (name, body) in [
        ("pack_registry.rs", registry),
        ("pack_info.rs", meta),
        ("naming_surfaces.rs", naming),
    ] {
        std::fs::write(out.join(name), body)
            .unwrap_or_else(|e| fail(&format!("writing {name}: {e}")));
    }
}
