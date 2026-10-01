// Included by `build.rs`. phase-474 W1 — the ENTRY packs are DISCOVERED, not
// authored, exactly as the message packs have been since phase-469 W1
// (`rosidl-codegen/build.rs`).
//
// `codegen/entry/render.rs` carried an `include_str!` row per template and
// `codegen/entry/pack.rs` one per manifest, so a new entry pack was two Rust
// edits beside its directory. Here the directory IS the declaration: every
// `src/codegen/entry/packs/entry/<dir>/pack.toml` is read at build time, its
// `templates = [{ key, file }]` rows are checked against the files beside it,
// and both registries are generated into `OUT_DIR`.
//
// Bundled at build time rather than read from disk at run time for the reason
// `render.rs` gives: `nros` runs from CMake and from build scripts in trees
// that do not contain this checkout.
//
// What this refuses is refused at COMPILE time, naming the file: a pack
// directory with no manifest, a manifest naming a file that does not exist, a
// `.jinja` no manifest claims, and one registry key declared twice. Each is a
// pack that "emits nothing and looks wired", which is the failure
// `check-entry-pack-conformance` exists for — and a build failure is earlier
// than a gate.
//
// What it does NOT check is the manifest's MEANING (kind completeness, the
// component kinds, the filters): the run-time `PackManifest` owns that schema,
// and a second reader of it here would be the duplication this file removes.
// The Rust tests and the conformance gate check it.

fn entry_pack_fail(what: &str) -> ! {
    panic!("nros-cli-core build script (entry packs): {what}");
}

fn generate_entry_packs() {
    use std::{collections::BTreeSet, fmt::Write as _};

    let manifest_dir = std::path::PathBuf::from(
        std::env::var("CARGO_MANIFEST_DIR").expect("cargo sets CARGO_MANIFEST_DIR"),
    );
    let rel_root = "src/codegen/entry/packs/entry";
    let root = manifest_dir.join(rel_root);
    // A DIRECTORY watch: cargo walks it, so a new pack directory or an edited
    // template re-runs this. A list of files would be the authored list again.
    println!("cargo:rerun-if-changed={}", root.display());

    let mut dirs: Vec<std::path::PathBuf> = std::fs::read_dir(&root)
        .unwrap_or_else(|e| entry_pack_fail(&format!("cannot read {}: {e}", root.display())))
        .map(|e| {
            e.unwrap_or_else(|e| entry_pack_fail(&format!("{e}")))
                .path()
        })
        .filter(|p| p.is_dir())
        .collect();
    dirs.sort();
    if dirs.is_empty() {
        entry_pack_fail("no pack directories — refusing to generate an empty registry");
    }

    let mut manifests = String::new();
    let mut templates = String::new();
    let mut seen_key = BTreeSet::new();
    for dir in &dirs {
        let name = dir
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or_else(|| entry_pack_fail("a pack directory with an unreadable name"));
        let ctx = format!("{rel_root}/{name}/pack.toml");
        let text = std::fs::read_to_string(dir.join("pack.toml")).unwrap_or_else(|e| {
            entry_pack_fail(&format!(
                "{ctx}: a pack directory with no readable manifest ({e}). Nothing \
                 would say what it emits or how CMake builds it."
            ))
        });
        let table: toml::Table =
            toml::from_str(&text).unwrap_or_else(|e| entry_pack_fail(&format!("{ctx}: {e}")));
        let rows = match table.get("templates") {
            Some(toml::Value::Array(rows)) => rows.clone(),
            Some(other) => {
                entry_pack_fail(&format!("{ctx}: `templates` must be an array, got {other}"))
            }
            None => entry_pack_fail(&format!(
                "{ctx}: no `templates`, so this pack renders nothing"
            )),
        };
        let _ = writeln!(
            manifests,
            "    ({name:?}, include_str!(concat!(env!(\"CARGO_MANIFEST_DIR\"), \"/{rel_root}/{name}/pack.toml\"))),"
        );
        let mut claimed = BTreeSet::new();
        for row in &rows {
            let t = row.as_table().unwrap_or_else(|| {
                entry_pack_fail(&format!(
                    "{ctx}: every `templates` row must be `{{ key = \"…\", file = \"…\" }}`, got {row}"
                ))
            });
            let field = |k: &str| match t.get(k) {
                Some(toml::Value::String(s)) => s.clone(),
                _ => entry_pack_fail(&format!("{ctx}: a `templates` row has no string `{k}`")),
            };
            let (key, file) = (field("key"), field("file"));
            if !dir.join(&file).is_file() {
                entry_pack_fail(&format!(
                    "{ctx}: row `{key}` names `{file}`, which does not exist beside the manifest"
                ));
            }
            if !seen_key.insert(key.clone()) {
                entry_pack_fail(&format!(
                    "{ctx}: registry key `{key}` is declared twice — the loader would \
                     resolve one and the other would never render"
                ));
            }
            claimed.insert(file.clone());
            let _ = writeln!(
                templates,
                "    ({name:?}, {key:?}, include_str!(concat!(env!(\"CARGO_MANIFEST_DIR\"), \"/{rel_root}/{name}/{file}\"))),"
            );
        }
        for entry in std::fs::read_dir(dir).unwrap_or_else(|e| entry_pack_fail(&format!("{e}"))) {
            let path = entry
                .unwrap_or_else(|e| entry_pack_fail(&format!("{e}")))
                .path();
            let file = path
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or_default();
            if file.ends_with(".jinja") && !claimed.contains(file) {
                entry_pack_fail(&format!(
                    "{rel_root}/{name}/{file} is not named by `{ctx}`, so nothing renders it"
                ));
            }
        }
    }

    let out = format!(
        "// @generated by build_entry_packs.rs from packs/entry/*/pack.toml — do not edit.\n\
         /// Every entry pack manifest, keyed by its directory.\n\
         pub(crate) const ENTRY_PACK_MANIFESTS: &[(&str, &str)] = &[\n{manifests}];\n\
         /// Every entry pack template: `(pack, registry key, source)`.\n\
         pub(crate) const ENTRY_TEMPLATES: &[(&str, &str, &str)] = &[\n{templates}];\n"
    );
    let out_dir = std::path::PathBuf::from(std::env::var("OUT_DIR").expect("cargo sets OUT_DIR"));
    std::fs::write(out_dir.join("entry_packs.rs"), out)
        .unwrap_or_else(|e| entry_pack_fail(&format!("write entry_packs.rs: {e}")));
}
