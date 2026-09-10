//! Stage 4 — one generated cargo settings file per image (RFC-0098 D1/D7,
//! phase-445 W4).
//!
//! ## What this replaces
//!
//! A cargo image used to reach its settings through THREE carriers, none of
//! them the image's own:
//!
//! | setting | old carrier |
//! | --- | --- |
//! | board link flags, `[unstable] build-std`, `CC_<triple>` | a TRACKED `<ws>/.cargo/config.toml`, hand-mirrored from the descriptor (and disagreeing with it) |
//! | the rustc triple | `--target` on the command line, from the descriptor or the fixture row |
//! | the entity facts (`NROS_DECLARED_*`) | the process environment of one invocation |
//! | in-repo `[patch.crates-io]` rows | sync's managed block in that same tracked config |
//!
//! Now every one of them lands in `build/<coord>/<entry>/nros-cargo.toml`, and
//! stage 5 hands cargo that file with `--config`. Nothing generated sits beside
//! a package, and the working directory stops carrying information.
//!
//! ## One file per IMAGE, not per coordinate
//!
//! RFC-0098 D1 writes `build/<image>/nros-cargo.toml` and phase-445 W4 writes
//! `build/<coord>/nros-cargo.toml`. They are not the same place, and only the
//! first is right: a coordinate is SHARED by images (`examples/workspaces/rust`
//! puts nine native images in `build/posix-zenoh/`), while the entity facts
//! differ per image — the whole point of D7. So the file sits in the image's own
//! entry directory, `build/<coord>/<entry>/`, beside the entry's `Cargo.toml`.
//!
//! ## Relative paths resolve against the file's GRANDPARENT
//!
//! Measured on cargo 1.98.1: for `--config <dir>/<sub>/x.toml`, a relative
//! `[build] target-dir`, a `relative = true` `[env]` value and a `[patch]`
//! `path` all resolve against `<dir>` — the parent of the directory holding the
//! file, exactly as cargo treats `<dir>/.cargo/config.toml`. So every relative
//! path written here is relative to [`base_dir`], `build/<coord>/`, and never to
//! the file's own directory. Getting this wrong is silent: a `target-dir` one
//! level off is still a directory cargo will happily create.
//!
//! Relative, not absolute, for the reason every generated file here is (W3.c):
//! byte-identical across checkouts.
//!
//! ## What does NOT go here
//!
//! A value the CALLER sets in the process environment. Cargo's `[env]` does not
//! override an already-set variable unless `force = true`, and this file never
//! adds `force` — so a lane front-end (a fixture row's `env`) outranks it, which
//! is RFC-0049's ladder: board < app < lane. A descriptor that itself says
//! `force = true` keeps it.

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use toml_edit::{DocumentMut, InlineTable, Item, Table, Value};

use super::paths::relative_or_err;

/// The settings file's name, inside the image's entry directory.
pub const FILE_NAME: &str = "nros-cargo.toml";

/// Everything the settings file states, assembled by the caller.
#[derive(Debug, Clone, Default)]
pub struct CargoConfigSpec {
    /// `[image.<id>]`, for the header.
    pub image_id: String,
    /// Board as authored, for the header.
    pub board: String,
    /// The descriptor's `cargo_config` template, verbatim (with `${workspace}`).
    pub cargo_config: Option<String>,
    /// The triple the descriptor pins — stated or inferred from its template
    /// (issue 0951). Written as `[build] target` when the template has no
    /// `[build]` of its own.
    pub target: Option<String>,
    /// The nano-ros checkout — what `${workspace}` means.
    pub nano_ros_root: PathBuf,
    /// The user's workspace root, handed to the pkg-index as
    /// `NROS_WORKSPACE_ROOT` so the entry's `nros::main!` finds the bringup
    /// however the entry was reached.
    pub workspace: PathBuf,
    /// The per-image cargo target directory.
    pub target_dir: PathBuf,
    /// `[env]` rows: the entity facts and any derived pool knobs. Plain values,
    /// never `force`d.
    pub env: BTreeMap<String, String>,
    /// `[patch.crates-io]`: crate name → absolute crate root. The descriptor's
    /// own `[patch]` rows are merged in by [`render`].
    pub patches: BTreeMap<String, PathBuf>,
}

/// The directory cargo resolves this file's relative paths against.
///
/// See the module docs: the GRANDPARENT of the file, not its parent.
#[must_use]
pub fn base_dir(config_path: &Path) -> Option<&Path> {
    config_path.parent()?.parent()
}

/// Render the settings file that will be written to `config_path`.
pub fn render(spec: &CargoConfigSpec, config_path: &Path) -> Result<String, String> {
    let base = base_dir(config_path).ok_or_else(|| {
        format!(
            "{} has no grandparent directory to resolve relative paths against",
            config_path.display()
        )
    })?;
    let rel = |to: &Path| relative_or_err(base, to);

    let mut doc: DocumentMut = match &spec.cargo_config {
        Some(raw) => raw.parse().map_err(|e| {
            format!(
                "board `{}`: its descriptor's `cargo_config` does not parse: {e}",
                spec.board
            )
        })?,
        None => DocumentMut::new(),
    };

    // ---- the descriptor's own `[patch]` rows, lifted out and merged ---------
    let mut patches: BTreeMap<String, String> = BTreeMap::new();
    if let Some(Item::Table(patch)) = doc.as_table_mut().remove("patch") {
        for (_registry, rows) in patch.iter() {
            let Some(rows) = rows.as_table_like() else {
                continue;
            };
            for (name, row) in rows.iter() {
                let Some(path) = row
                    .as_table_like()
                    .and_then(|t| t.get("path"))
                    .and_then(Item::as_str)
                else {
                    continue;
                };
                patches.insert(name.to_string(), rel(&expand(path, &spec.nano_ros_root))?);
            }
        }
    }
    for (name, dir) in &spec.patches {
        patches.insert(name.clone(), rel(dir)?);
    }

    // ---- `${workspace}` everywhere else -------------------------------------
    for (key, item) in doc.as_table_mut().iter_mut() {
        resolve_placeholders(item, key.get() == "env", spec, base)?;
    }

    // ---- [build]: the triple and the per-image target dir -------------------
    let build = table(&mut doc, "build");
    // A stated target may be a PATH to the board's own target spec (the
    // nuttx-riscv board, phase-445); the triple it names is the file stem, and
    // that is what the pinned triple is compared against. The path itself is
    // written through unchanged — cargo needs the file, not the name.
    match (build.get("target").and_then(Item::as_str), &spec.target) {
        (Some(stated), Some(pinned))
            if crate::orchestration::board_descriptor::triple_of_build_target(stated) != pinned =>
        {
            return Err(format!(
                "board `{}` states two triples: `[build] target = \"{stated}\"` in its \
                 `cargo_config` and `target = \"{pinned}\"`. One board has one triple; fix \
                 the descriptor.",
                spec.board
            ));
        }
        (None, Some(pinned)) => {
            build.insert("target", toml_edit::value(pinned.as_str()));
        }
        _ => {}
    }
    build.insert("target-dir", toml_edit::value(rel(&spec.target_dir)?));

    // ---- [env] ---------------------------------------------------------------
    let env = table(&mut doc, "env");
    let mut ws = InlineTable::new();
    ws.insert("value", rel(&spec.workspace)?.into());
    ws.insert("relative", true.into());
    env.insert("NROS_WORKSPACE_ROOT", Item::Value(Value::InlineTable(ws)));
    for (k, v) in &spec.env {
        env.insert(k, toml_edit::value(v.as_str()));
    }

    // ---- [patch.crates-io] ---------------------------------------------------
    if !patches.is_empty() {
        let mut rows = Table::new();
        for (name, path) in &patches {
            let mut t = InlineTable::new();
            t.insert("path", path.as_str().into());
            rows.insert(name, Item::Value(Value::InlineTable(t)));
        }
        let mut patch = Table::new();
        patch.set_implicit(true);
        patch.insert("crates-io", Item::Table(rows));
        doc.as_table_mut().insert("patch", Item::Table(patch));
    }

    let mut out = format!(
        "# GENERATED by `nros build` (RFC-0098 D1, phase-445 W4) — DO NOT EDIT.\n\
         #\n\
         # Every cargo setting image `{image}` needs, in ONE file: the board's\n\
         # `cargo_config` (board `{board}`), this image's target dir, its entity\n\
         # facts and pool knobs as `[env]`, and the in-repo `[patch]` rows.\n\
         # Regenerated on every build; edit the board descriptor or the image.\n\
         #\n\
         # Build it yourself with:\n\
         #   cargo build --manifest-path <this dir>/Cargo.toml --config <this file>\n\
         #\n\
         # Relative paths below resolve against this file's GRANDPARENT\n\
         # (`build/<coord>/`), which is how cargo reads a `--config` file.\n\
         # `[env]` never uses `force`, so a value set in the calling environment\n\
         # wins (RFC-0049: board < app < lane).\n\n",
        image = spec.image_id,
        board = spec.board,
    );
    out.push_str(doc.to_string().trim_start());
    if !out.ends_with('\n') {
        out.push('\n');
    }
    Ok(out)
}

/// Write the settings file, creating its directory; unchanged content is not
/// rewritten (the mtime treadmill — cargo re-reads config, and a touched file
/// is noise in every fixture signature that hashes the build tree).
pub fn write(spec: &CargoConfigSpec, config_path: &Path) -> Result<PathBuf, String> {
    let body = render(spec, config_path)?;
    if let Some(dir) = config_path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("creating {}: {e}", dir.display()))?;
    }
    if std::fs::read_to_string(config_path).ok().as_deref() != Some(body.as_str()) {
        std::fs::write(config_path, &body)
            .map_err(|e| format!("writing {}: {e}", config_path.display()))?;
    }
    Ok(config_path.to_path_buf())
}

/// `${workspace}/x` → `<nano_ros_root>/x`.
fn expand(s: &str, nano_ros_root: &Path) -> PathBuf {
    match s.strip_prefix("${workspace}/") {
        Some(rest) => nano_ros_root.join(rest),
        None if s == "${workspace}" => nano_ros_root.to_path_buf(),
        None => PathBuf::from(s.replace("${workspace}", &nano_ros_root.display().to_string())),
    }
}

fn table<'a>(doc: &'a mut DocumentMut, key: &str) -> &'a mut Table {
    if !doc.as_table().contains_key(key) {
        doc.as_table_mut().insert(key, Item::Table(Table::new()));
    }
    doc.as_table_mut()
        .get_mut(key)
        .and_then(Item::as_table_mut)
        .expect("just inserted as a table")
}

/// Resolve `${workspace}` in a descriptor value.
///
/// `[env]` values become `{ value = "<rel>", relative = true }` — cargo then
/// absolutises them against [`base_dir`], so a build script sees a real path
/// whatever its own working directory is. Anywhere else (a rustflag, a runner)
/// there is no such mechanism: rustc and the linker run with cargo's working
/// directory, which this file does not control, so the path is written
/// ABSOLUTE. No shipped descriptor has one today; the rule is here so the first
/// one is correct rather than one level off.
fn resolve_placeholders(
    item: &mut Item,
    in_env: bool,
    spec: &CargoConfigSpec,
    base: &Path,
) -> Result<(), String> {
    match item {
        Item::Table(t) => {
            for (_, i) in t.iter_mut() {
                resolve_placeholders(i, in_env, spec, base)?;
            }
        }
        Item::Value(v) => resolve_value(v, in_env, spec, base)?,
        Item::ArrayOfTables(a) => {
            for t in a.iter_mut() {
                for (_, i) in t.iter_mut() {
                    resolve_placeholders(i, in_env, spec, base)?;
                }
            }
        }
        Item::None => {}
    }
    Ok(())
}

fn resolve_value(
    v: &mut Value,
    in_env: bool,
    spec: &CargoConfigSpec,
    base: &Path,
) -> Result<(), String> {
    match v {
        Value::String(s) if s.value().contains("${workspace}") => {
            let path = expand(s.value(), &spec.nano_ros_root);
            if in_env {
                let mut t = InlineTable::new();
                t.insert("value", relative_or_err(base, &path)?.into());
                t.insert("relative", true.into());
                *v = Value::InlineTable(t);
            } else {
                *v = path.display().to_string().into();
            }
        }
        Value::Array(a) => {
            for e in a.iter_mut() {
                resolve_value(e, false, spec, base)?;
            }
        }
        Value::InlineTable(t) => {
            // An `[env]` row that is already a table carries `force` beside its
            // `value`; the placeholder is in `value`, and `relative` joins it.
            let is_env_row = in_env && t.contains_key("value");
            let mut made_relative = false;
            for (k, inner) in t.iter_mut() {
                if let Value::String(s) = inner
                    && s.value().contains("${workspace}")
                {
                    let path = expand(s.value(), &spec.nano_ros_root);
                    if is_env_row && k == "value" {
                        *inner = relative_or_err(base, &path)?.into();
                        made_relative = true;
                    } else {
                        *inner = path.display().to_string().into();
                    }
                } else {
                    resolve_value(inner, false, spec, base)?;
                }
            }
            if made_relative {
                t.insert("relative", true.into());
                t.fmt();
            }
        }
        _ => {}
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const ESP32: &str = r#"
[build]
target = "riscv32imc-unknown-none-elf"
[target.riscv32imc-unknown-none-elf]
rustflags = ["-C", "link-arg=-Tlinkall.x"]
[env]
ESP_LOG = "info"
[unstable]
build-std = ["core", "alloc"]
"#;

    fn spec() -> CargoConfigSpec {
        CargoConfigSpec {
            image_id: "esp32".into(),
            board: "esp32-qemu".into(),
            cargo_config: Some(ESP32.into()),
            target: Some("riscv32imc-unknown-none-elf".into()),
            nano_ros_root: PathBuf::from("/nros"),
            workspace: PathBuf::from("/nros/examples/workspaces/rust"),
            target_dir: PathBuf::from(
                "/nros/examples/workspaces/rust/build/esp32-zenoh/esp32_entry/target",
            ),
            env: BTreeMap::from([("NROS_DECLARED_NODES".into(), "2".into())]),
            patches: BTreeMap::new(),
        }
    }

    fn cfg_path() -> PathBuf {
        PathBuf::from("/nros/examples/workspaces/rust/build/esp32-zenoh/esp32_entry")
            .join(FILE_NAME)
    }

    fn parsed(body: &str) -> toml::Value {
        toml::from_str(body).expect("the generated file is TOML")
    }

    #[test]
    fn relative_paths_are_relative_to_the_grandparent() {
        // Measured: cargo resolves a `--config` file's relative paths against
        // the parent of the file's directory. The target dir is INSIDE the
        // image's entry dir, so from `build/<coord>/` it is `<entry>/target`.
        let v = parsed(&render(&spec(), &cfg_path()).unwrap());
        assert_eq!(
            v["build"]["target-dir"].as_str(),
            Some("esp32_entry/target")
        );
        assert_eq!(
            v["env"]["NROS_WORKSPACE_ROOT"]["value"].as_str(),
            Some("../..")
        );
        assert_eq!(
            v["env"]["NROS_WORKSPACE_ROOT"]["relative"].as_bool(),
            Some(true)
        );
    }

    #[test]
    fn the_board_template_is_carried_verbatim() {
        let v = parsed(&render(&spec(), &cfg_path()).unwrap());
        assert_eq!(
            v["build"]["target"].as_str(),
            Some("riscv32imc-unknown-none-elf")
        );
        assert_eq!(v["env"]["ESP_LOG"].as_str(), Some("info"));
        assert!(v["unstable"]["build-std"].is_array());
        assert!(v["target"]["riscv32imc-unknown-none-elf"]["rustflags"].is_array());
    }

    #[test]
    fn an_inferred_triple_becomes_build_target() {
        // Issue 0951: the mps2 FreeRTOS descriptor states its triple only as a
        // `[target.<triple>]` header. `--target` used to carry it; this file
        // is now the one carrier.
        let mut s = spec();
        s.cargo_config = Some("[target.thumbv7m-none-eabi]\nrustflags = []\n".into());
        s.target = Some("thumbv7m-none-eabi".into());
        let v = parsed(&render(&s, &cfg_path()).unwrap());
        assert_eq!(v["build"]["target"].as_str(), Some("thumbv7m-none-eabi"));
    }

    #[test]
    fn a_target_spec_path_is_written_absolute_and_matches_its_stem() {
        // The nuttx-riscv board names its own spec; `${workspace}` outside
        // `[env]` has no config-relative mechanism, so it is written absolute,
        // and the pinned triple (the stem) must not read as a second triple.
        let mut s = spec();
        s.cargo_config = Some(
            "[build]\ntarget = \"${workspace}/packages/boards/b/riscv32imac-unknown-nuttx-elf.json\"\n\
             [target.riscv32imac-unknown-nuttx-elf]\nlinker = \"riscv-none-elf-gcc\"\n"
                .into(),
        );
        s.target = Some("riscv32imac-unknown-nuttx-elf".into());
        let v = parsed(&render(&s, &cfg_path()).expect("stem matches the pinned triple"));
        assert_eq!(
            v["build"]["target"].as_str(),
            Some("/nros/packages/boards/b/riscv32imac-unknown-nuttx-elf.json")
        );
        s.target = Some("thumbv7m-none-eabi".into());
        assert!(
            render(&s, &cfg_path()).is_err(),
            "a different stem still disagrees"
        );
    }

    #[test]
    fn two_triples_for_one_board_is_an_error() {
        let mut s = spec();
        s.target = Some("thumbv7m-none-eabi".into());
        let e = render(&s, &cfg_path()).expect_err("disagreement");
        assert!(e.contains("esp32-qemu"), "{e}");
    }

    #[test]
    fn a_hosted_board_gets_no_target_but_still_a_target_dir() {
        let mut s = spec();
        s.cargo_config = None;
        s.target = None;
        let v = parsed(&render(&s, &cfg_path()).unwrap());
        assert!(v["build"].get("target").is_none());
        assert!(v["build"]["target-dir"].is_str());
    }

    #[test]
    fn entity_facts_are_env_and_never_forced() {
        // A lane that sets the same variable must win (RFC-0049's ladder).
        let body = render(&spec(), &cfg_path()).unwrap();
        let v = parsed(&body);
        assert_eq!(v["env"]["NROS_DECLARED_NODES"].as_str(), Some("2"));
        assert!(!body.contains("force = true"), "{body}");
    }

    #[test]
    fn descriptor_patch_rows_merge_with_the_callers_relative_to_the_base() {
        let mut s = spec();
        s.cargo_config = Some(
            "[patch.crates-io]\nlibc = { path = \"${workspace}/third-party/nuttx/libc\" }\n".into(),
        );
        s.target = None;
        s.patches.insert(
            "nros-core".into(),
            PathBuf::from("/nros/packages/core/nros-core"),
        );
        let v = parsed(&render(&s, &cfg_path()).unwrap());
        let rows = &v["patch"]["crates-io"];
        assert_eq!(
            rows["libc"]["path"].as_str(),
            Some("../../../../../third-party/nuttx/libc")
        );
        assert_eq!(
            rows["nros-core"]["path"].as_str(),
            Some("../../../../../packages/core/nros-core")
        );
    }

    #[test]
    fn a_workspace_placeholder_in_env_becomes_a_relative_env_row() {
        let mut s = spec();
        s.cargo_config = Some(
            "[env]\nCFG = { value = \"${workspace}/packages/boards/b/config\", force = true }\n"
                .into(),
        );
        s.target = None;
        let v = parsed(&render(&s, &cfg_path()).unwrap());
        assert_eq!(
            v["env"]["CFG"]["value"].as_str(),
            Some("../../../../../packages/boards/b/config")
        );
        assert_eq!(v["env"]["CFG"]["relative"].as_bool(), Some(true));
        assert_eq!(
            v["env"]["CFG"]["force"].as_bool(),
            Some(true),
            "authored force survives"
        );
    }

    #[test]
    fn no_absolute_path_is_written_for_an_in_tree_image() {
        let body = render(&spec(), &cfg_path()).unwrap();
        assert!(!body.contains("\"/nros"), "{body}");
    }

    #[test]
    fn writing_twice_does_not_touch_the_file() {
        let tmp = tempfile::tempdir().unwrap();
        let mut s = spec();
        s.nano_ros_root = tmp.path().to_path_buf();
        s.workspace = tmp.path().join("ws");
        s.target_dir = tmp.path().join("ws/build/c/e/target");
        let p = tmp.path().join("ws/build/c/e").join(FILE_NAME);
        write(&s, &p).unwrap();
        let m1 = std::fs::metadata(&p).unwrap().modified().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(20));
        write(&s, &p).unwrap();
        assert_eq!(m1, std::fs::metadata(&p).unwrap().modified().unwrap());
    }
}
