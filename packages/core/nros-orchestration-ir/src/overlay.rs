//! phase-486 W1 (RFC-0060 amendment 2026-10-10) — the nano-ros OVERLAY.
//!
//! The SystemModel carries what every realizer reads with the same meaning:
//! topology, the platform-agnostic contract, scheduling. A nano-ros BUILD fact
//! — the capability switches (`[system] features`, `[param_services]`) and the
//! system-wide `[lifecycle] autostart` default — has no Linux meaning, so it
//! travels beside the model instead, in `nros.toml`, written into the same
//! directory as every model resolved for a bringup.
//!
//! One file per DIRECTORY, not per model: every model in a directory comes
//! from one bringup, and the overlay is a function of that bringup's
//! `system.toml` alone, never of the launch file or its arguments.
//!
//! W1 is ADDITIVE: producers write the overlay and nothing reads it yet. Its
//! content is exactly what rlm's `apply_to_launch` projects into the model
//! (`execution.features`) and what `lifecycle_autostart()` returns, which
//! `overlay_agrees_with_the_model_projection` in nros-cli-core asserts for
//! every tracked `system.toml` — the safety net for W3, where the readers move.
//!
//! No overlay content ⇒ no file, and a stale one is deleted (RFC-0100's
//! 2026-10-10 rule, and the PR #1830 lesson: a reader acts on whatever file
//! sits at the path).

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

/// The overlay's file name, beside the model.
pub const OVERLAY_FILE: &str = "nros.toml";

/// Overlay schema version. A reader refuses a newer one.
pub const OVERLAY_VERSION: u32 = 1;

/// The resolved nano-ros facts for one bringup.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Overlay {
    /// The capability switches, in declaration order, `[param_services]`
    /// sugar folded in — the list rlm projects as `execution.features`.
    pub features: Vec<String>,
    /// `[lifecycle] autostart`: `none`, `configure` or `active`. An
    /// unrecognised value is dropped, as rlm's `lifecycle_autostart()` drops it.
    pub lifecycle_autostart: Option<String>,
    /// phase-486 W2 — per-image switches ADDED over [`Self::features`]:
    /// `[image_defaults] features` then `[image.<id>] features`, the same
    /// base-then-specific concatenation `ImageBlock::merged_over` applies.
    /// Only images that add something appear.
    pub images: BTreeMap<String, Vec<String>>,
}

impl Overlay {
    /// True when the overlay states nothing, so no file is written.
    pub fn is_empty(&self) -> bool {
        self.features.is_empty() && self.lifecycle_autostart.is_none() && self.images.is_empty()
    }

    /// The switches image `id` builds with: the system list plus whatever the
    /// image adds, first spelling kept.
    pub fn features_for_image(&self, id: &str) -> Vec<String> {
        let mut out = self.features.clone();
        for f in self.images.get(id).into_iter().flatten() {
            if !out.contains(f) {
                out.push(f.clone());
            }
        }
        out
    }

    /// Derive the overlay from a bringup's `system.toml` text.
    ///
    /// Lax about every key it does not read, like rlm's parser: validating the
    /// file is the strict `SystemToml` parser's job, not this one's.
    pub fn from_system_toml_str(text: &str, origin: &Path) -> Result<Self, String> {
        let doc: toml::Table =
            toml::from_str(text).map_err(|e| format!("{}: cannot parse: {e}", origin.display()))?;
        let mut features: Vec<String> = Vec::new();
        if let Some(v) = doc
            .get("system")
            .and_then(|s| s.as_table())
            .and_then(|s| s.get("features"))
        {
            let arr = v.as_array().ok_or_else(|| {
                format!(
                    "{}: `[system] features` must be an array of strings",
                    origin.display()
                )
            })?;
            for item in arr {
                let s = item.as_str().ok_or_else(|| {
                    format!(
                        "{}: every `[system] features` element must be a string; found {item}",
                        origin.display()
                    )
                })?;
                features.push(s.to_string());
            }
        }
        // `[param_services]` is sugar for `features = ["param_services"]`.
        if doc.contains_key("param_services") && !features.iter().any(|f| f == "param_services") {
            features.push("param_services".to_string());
        }
        let lifecycle_autostart = doc
            .get("lifecycle")
            .and_then(|l| l.as_table())
            .and_then(|l| l.get("autostart"))
            .and_then(|a| a.as_str())
            .filter(|a| matches!(*a, "none" | "configure" | "active"))
            .map(str::to_string);
        // phase-486 W2 — per-image switches. A NEW reader of an existing key:
        // `ImageBlock.features` parsed and was read by nothing, so an image
        // stating one was silently built without it. Unknown names are refused
        // here, naming the image — this key has no compatibility to keep.
        let list_at = |t: Option<&toml::Value>, what: &str| -> Result<Vec<String>, String> {
            let Some(v) = t.and_then(|t| t.get("features")) else {
                return Ok(Vec::new());
            };
            let arr = v.as_array().ok_or_else(|| {
                format!(
                    "{}: `{what} features` must be an array of strings",
                    origin.display()
                )
            })?;
            let mut out = Vec::new();
            for item in arr {
                let f = item.as_str().ok_or_else(|| {
                    format!(
                        "{}: every `{what} features` element must be a string",
                        origin.display()
                    )
                })?;
                if crate::leaf_capabilities::axis(f).is_none() {
                    let known: Vec<&str> = crate::leaf_capabilities::LEAF_CAPABILITY_AXES
                        .iter()
                        .map(|a| a.declared)
                        .collect();
                    return Err(format!(
                        "{}: `{what} features` names `{f}`, which is not a capability axis \
                         (known: {})",
                        origin.display(),
                        known.join(", ")
                    ));
                }
                out.push(f.to_string());
            }
            Ok(out)
        };
        let defaults = list_at(doc.get("image_defaults"), "[image_defaults]")?;
        let mut images = BTreeMap::new();
        if let Some(tbl) = doc.get("image").and_then(|i| i.as_table()) {
            for (id, block) in tbl {
                let mut add = defaults.clone();
                for f in list_at(Some(block), &format!("[image.{id}]"))? {
                    if !add.contains(&f) {
                        add.push(f);
                    }
                }
                add.retain(|f| !features.contains(f));
                if !add.is_empty() {
                    images.insert(id.clone(), add);
                }
            }
        }
        Ok(Self {
            features,
            lifecycle_autostart,
            images,
        })
    }

    /// Derive the overlay from a `system.toml` on disk.
    pub fn from_system_toml(path: &Path) -> Result<Self, String> {
        let text = std::fs::read_to_string(path)
            .map_err(|e| format!("{}: cannot read: {e}", path.display()))?;
        Self::from_system_toml_str(&text, path)
    }

    /// The file's text. Deterministic, so write-if-changed is byte-exact.
    pub fn render(&self) -> String {
        let mut out = String::new();
        out.push_str(
            "# Written by nros (phase-486). The nano-ros facts that do not belong in\n\
             # the SystemModel beside it — do not edit; re-run `nros sync`.\n",
        );
        out.push_str(&format!("[meta]\nversion = {OVERLAY_VERSION}\n"));
        if !self.features.is_empty() {
            let items: Vec<String> = self.features.iter().map(|f| format!("{f:?}")).collect();
            out.push_str(&format!("\n[system]\nfeatures = [{}]\n", items.join(", ")));
        }
        if let Some(a) = &self.lifecycle_autostart {
            out.push_str(&format!("\n[lifecycle]\nautostart = {a:?}\n"));
        }
        for (id, feats) in &self.images {
            let items: Vec<String> = feats.iter().map(|f| format!("{f:?}")).collect();
            out.push_str(&format!(
                "\n[image.{id:?}]\nfeatures = [{}]\n",
                items.join(", ")
            ));
        }
        out
    }

    /// Parse a written overlay. Refuses a newer schema and unknown keys: this
    /// file is ours, so anything unexpected in it is a defect, not a choice.
    pub fn parse(text: &str, origin: &Path) -> Result<Self, String> {
        let doc: toml::Table =
            toml::from_str(text).map_err(|e| format!("{}: cannot parse: {e}", origin.display()))?;
        for k in doc.keys() {
            if !matches!(k.as_str(), "meta" | "system" | "lifecycle" | "image") {
                return Err(format!(
                    "{}: unknown overlay table `[{k}]`",
                    origin.display()
                ));
            }
        }
        let version = doc
            .get("meta")
            .and_then(|m| m.get("version"))
            .and_then(|v| v.as_integer())
            .ok_or_else(|| format!("{}: overlay has no `[meta] version`", origin.display()))?;
        if version > i64::from(OVERLAY_VERSION) {
            return Err(format!(
                "{}: overlay version {version} is newer than this nros reads ({OVERLAY_VERSION}); \
                 update nros",
                origin.display()
            ));
        }
        let features = doc
            .get("system")
            .and_then(|s| s.get("features"))
            .and_then(|f| f.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|x| x.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();
        let lifecycle_autostart = doc
            .get("lifecycle")
            .and_then(|l| l.get("autostart"))
            .and_then(|a| a.as_str())
            .map(str::to_string);
        let mut images = BTreeMap::new();
        if let Some(tbl) = doc.get("image").and_then(|i| i.as_table()) {
            for (id, block) in tbl {
                let feats: Vec<String> = block
                    .get("features")
                    .and_then(|f| f.as_array())
                    .map(|a| {
                        a.iter()
                            .filter_map(|x| x.as_str().map(str::to_string))
                            .collect()
                    })
                    .unwrap_or_default();
                images.insert(id.clone(), feats);
            }
        }
        Ok(Self {
            features,
            lifecycle_autostart,
            images,
        })
    }
}

/// Where the overlay for `model_path` lives: beside it.
pub fn overlay_path_for_model(model_path: &Path) -> PathBuf {
    model_path
        .parent()
        .map(|d| d.join(OVERLAY_FILE))
        .unwrap_or_else(|| PathBuf::from(OVERLAY_FILE))
}

/// Write (or delete) the overlay for the bringup whose `system.toml` is
/// `system_toml`, beside `model_path`.
///
/// Write-if-changed: rewriting identical bytes would bump the mtime, and a
/// consumer that registers the overlay as a build input would then rebuild
/// forever (the `ensure_model` freshness lesson). An empty overlay deletes a
/// stale file. Returns the path when a file now exists.
pub fn write_beside(model_path: &Path, system_toml: &Path) -> Result<Option<PathBuf>, String> {
    let overlay = Overlay::from_system_toml(system_toml)?;
    let path = overlay_path_for_model(model_path);
    if overlay.is_empty() {
        match std::fs::remove_file(&path) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => {
                return Err(format!(
                    "{}: cannot remove stale overlay: {e}",
                    path.display()
                ));
            }
        }
        return Ok(None);
    }
    let text = overlay.render();
    if std::fs::read_to_string(&path).ok().as_deref() != Some(text.as_str()) {
        std::fs::write(&path, &text)
            .map_err(|e| format!("{}: cannot write: {e}", path.display()))?;
    }
    Ok(Some(path))
}

/// phase-486 W3 — a resolved model and the overlay beside it, read together.
///
/// Every nano-ros reader of a build switch or of the lifecycle default reads
/// [`Self::overlay`], never `model.execution.features` (gated by
/// `check-launch-model-boundary`). Derefs to the model, so topology,
/// contracts and scheduling read exactly as before.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LoadedModel {
    pub model: ros_launch_manifest_model::SystemModel,
    pub overlay: Overlay,
}

impl std::ops::Deref for LoadedModel {
    type Target = ros_launch_manifest_model::SystemModel;
    fn deref(&self) -> &Self::Target {
        &self.model
    }
}

impl LoadedModel {
    /// A model with no overlay: a bringup that states no nano-ros facts.
    pub fn bare(model: ros_launch_manifest_model::SystemModel) -> Self {
        Self {
            model,
            overlay: Overlay::default(),
        }
    }

    /// Model YAML in the shape an rlm before `v0.1.55` wrote, paired with the
    /// overlay `nros sync` would have written for it: the switches projected
    /// into `execution: features:` move into the overlay. rlm deleted the
    /// typed field (phase-486 W8), so they are read from the text by
    /// [`legacy_projected_features`].
    ///
    /// For TESTS whose fixtures are model YAML written in the old shape. No
    /// production path uses it: a real model always has its overlay beside it.
    pub fn from_legacy_yaml(yaml: &str) -> Result<Self, String> {
        let model = ros_launch_manifest_model::SystemModel::from_yaml_str(yaml)
            .map_err(|e| format!("load SystemModel: {e}"))?;
        let overlay = Overlay {
            features: legacy_projected_features(yaml),
            ..Overlay::default()
        };
        Ok(Self { model, overlay })
    }

    /// Read the model at `path` and the overlay beside it.
    pub fn load(path: &Path) -> Result<Self, String> {
        let yaml = std::fs::read_to_string(path)
            .map_err(|e| format!("read SystemModel {}: {e}", path.display()))?;
        let model = ros_launch_manifest_model::SystemModel::from_yaml_str(&yaml)
            .map_err(|e| format!("load SystemModel {}: {e}", path.display()))?;
        Self::with_overlay_beside(model, path)
    }

    /// Pair an already-parsed `model` with the overlay beside `model_path`.
    pub fn with_overlay_beside(
        model: ros_launch_manifest_model::SystemModel,
        model_path: &Path,
    ) -> Result<Self, String> {
        let overlay = load_beside(model_path)?;
        Ok(Self { model, overlay })
    }

    /// The image's capability switches: the overlay's system list plus what
    /// `image` adds (phase-486 W2). `None` = the system list alone.
    pub fn features(&self, image: Option<&str>) -> Vec<String> {
        match image {
            Some(id) => self.overlay.features_for_image(id),
            None => self.overlay.features.clone(),
        }
    }

    /// The image's lifecycle boot transition — [`reduce_lifecycle_autostart`].
    pub fn lifecycle_autostart(
        &self,
    ) -> Result<Option<ros_launch_manifest_model::Autostart>, String> {
        reduce_lifecycle_autostart(&self.model, &self.overlay)
    }
}

/// Read the overlay beside `model_path`.
///
/// Absent file ⇒ an empty overlay: the bringup states no nano-ros facts. The
/// one exception is a model that still CARRIES build switches — written by an
/// `nros` from before phase-486 W1, or by an rlm that still projected them —
/// with no overlay beside it. Reading "no switches" there would silently
/// build the image without them, so it is refused, naming the fix.
///
/// The question is asked of the model FILE, not of a parsed
/// `SystemModel`: rlm deleted the typed `execution.features` field at
/// phase-486 W8, so a typed model can no longer say whether an old writer
/// put switches there. [`legacy_projected_features`] reads the raw key.
pub fn load_beside(model_path: &Path) -> Result<Overlay, String> {
    let path = overlay_path_for_model(model_path);
    match std::fs::read_to_string(&path) {
        Ok(text) => Overlay::parse(&text, &path),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            let legacy = std::fs::read_to_string(model_path)
                .map(|t| legacy_projected_features(&t))
                .unwrap_or_default();
            if legacy.is_empty() {
                Ok(Overlay::default())
            } else {
                Err(format!(
                    "{}: this model carries capability switches ({}) but no `{OVERLAY_FILE}` \
                     sits beside it. nano-ros reads its switches from that overlay \
                     (phase-486); re-run `nros sync` to write it.",
                    model_path.display(),
                    legacy.join(", ")
                ))
            }
        }
        Err(e) => Err(format!("{}: cannot read: {e}", path.display())),
    }
}

/// The capability switches an older writer projected into a model's
/// `execution: features:` key (rlm before `v0.1.55`), read from the YAML
/// text. Empty for any model written since, and for text that is not a
/// model at all — the caller's own parse reports that.
#[must_use]
pub fn legacy_projected_features(model_yaml: &str) -> Vec<String> {
    // Field names differ from the YAML keys on purpose: this is the one
    // reader of the retired key, and `check-launch-model-boundary` refuses
    // the typed spelling anywhere in the tree.
    #[derive(serde::Deserialize, Default)]
    struct Doc {
        #[serde(default, rename = "execution")]
        exec: Exec,
    }
    #[derive(serde::Deserialize, Default)]
    struct Exec {
        #[serde(default, rename = "features")]
        switches: Vec<String>,
    }
    serde_yaml_ng::from_str::<Doc>(model_yaml)
        .map(|d| d.exec.switches)
        .unwrap_or_default()
}

/// phase-486 W3 — does the image this model was resolved for run `fqn`?
///
/// `execution.deploy` is a placement map: a node in it is placed, and a node
/// absent from a NON-EMPTY map was placed elsewhere (`[host.*]` with
/// `nodes = [..]`). An empty map places nothing, so every node runs. The
/// board-slice rule that also read `Deploy.target` is gone (no tracked
/// producer, no other reader; rlm stopped writing it).
pub fn placed_here(model: &ros_launch_manifest_model::SystemModel, fqn: &str) -> bool {
    model.execution.deploy.is_empty() || model.execution.deploy.contains_key(fqn)
}

/// phase-486 W0 Correction 2 — ONE lifecycle boot transition per image.
///
/// The runtime has one executor-wide autostart, while the model states it per
/// node. Three readers used to take the first node's value. The rule now:
/// - every node that states a value agrees, and that value wins;
/// - if none states one, the overlay's `[lifecycle] autostart` applies;
/// - if two disagree, refuse, naming both.
pub fn reduce_lifecycle_autostart(
    model: &ros_launch_manifest_model::SystemModel,
    overlay: &Overlay,
) -> Result<Option<ros_launch_manifest_model::Autostart>, String> {
    use ros_launch_manifest_model::Autostart;
    let mut first: Option<(&str, Autostart)> = None;
    for (fqn, n) in &model.structure.nodes {
        let Some(a) = n.lifecycle_autostart else {
            continue;
        };
        match first {
            None => first = Some((fqn.as_str(), a)),
            Some((f0, a0)) if a0 != a => {
                return Err(format!(
                    "lifecycle autostart disagrees within one image: `{f0}` states {a0:?}, \
                     `{fqn}` states {a:?}. The runtime has one executor-wide boot transition; \
                     make the launch file (or contract) state one value."
                ));
            }
            Some(_) => {}
        }
    }
    if let Some((_, a)) = first {
        return Ok(Some(a));
    }
    Ok(match overlay.lifecycle_autostart.as_deref() {
        Some("none") => Some(Autostart::None),
        Some("configure") => Some(Autostart::Configure),
        Some("active") => Some(Autostart::Active),
        _ => None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn o(text: &str) -> Overlay {
        Overlay::from_system_toml_str(text, Path::new("system.toml")).unwrap()
    }

    #[test]
    fn a_bringup_with_no_switches_states_nothing() {
        assert!(o("[system]\nname = \"t\"\nrmw = \"zenoh\"\n").is_empty());
    }

    #[test]
    fn param_services_sugar_folds_in_once() {
        assert_eq!(
            o("[system]\nname = \"t\"\n[param_services]\n").features,
            vec!["param_services"]
        );
        assert_eq!(
            o("[system]\nfeatures = [\"param_services\"]\n[param_services]\n").features,
            vec!["param_services"]
        );
    }

    #[test]
    fn lifecycle_autostart_is_read_and_an_unknown_level_is_dropped() {
        assert_eq!(
            o("[lifecycle]\nautostart = \"active\"\n")
                .lifecycle_autostart
                .as_deref(),
            Some("active")
        );
        assert_eq!(
            o("[lifecycle]\nautostart = \"bogus\"\n").lifecycle_autostart,
            None
        );
    }

    #[test]
    fn render_then_parse_round_trips() {
        let ov = o(
            "[system]\nfeatures = [\"lifecycle\", \"param_services\"]\n[lifecycle]\nautostart = \"configure\"\n",
        );
        let back = Overlay::parse(&ov.render(), Path::new("nros.toml")).unwrap();
        assert_eq!(back, ov);
    }

    #[test]
    fn parse_refuses_a_newer_version_and_unknown_tables() {
        let p = Path::new("nros.toml");
        assert!(
            Overlay::parse("[meta]\nversion = 99\n", p)
                .unwrap_err()
                .contains("newer")
        );
        assert!(
            Overlay::parse("[meta]\nversion = 1\n[deploy]\n", p)
                .unwrap_err()
                .contains("unknown")
        );
    }

    #[test]
    fn an_image_adds_switches_over_the_system_list_and_defaults_apply_to_every_image() {
        let ov = o("[system]\nfeatures = [\"safety\"]\n\
                    [image_defaults]\nfeatures = [\"rosout\"]\n\
                    [image.a]\nboard = \"native\"\nfeatures = [\"param_services\"]\n\
                    [image.b]\nboard = \"native\"\n");
        assert_eq!(
            ov.features_for_image("a"),
            vec!["safety", "rosout", "param_services"]
        );
        assert_eq!(ov.features_for_image("b"), vec!["safety", "rosout"]);
        // An image the overlay does not mention builds with the system list.
        assert_eq!(ov.features_for_image("c"), vec!["safety"]);
        let back = Overlay::parse(&ov.render(), Path::new("nros.toml")).unwrap();
        assert_eq!(back, ov);
    }

    #[test]
    fn a_switch_only_one_image_states_stays_off_the_others() {
        let ov = o("[image.native]\nfeatures = [\"param_services\"]\n[image.mcu]\nboard = \"x\"\n");
        assert!(ov.features.is_empty());
        assert_eq!(ov.features_for_image("native"), vec!["param_services"]);
        assert!(ov.features_for_image("mcu").is_empty());
    }

    #[test]
    fn an_unknown_image_switch_is_refused_naming_the_image() {
        let e = Overlay::from_system_toml_str(
            "[image.native]\nfeatures = [\"param_servics\"]\n",
            Path::new("system.toml"),
        )
        .unwrap_err();
        assert!(
            e.contains("[image.native]") && e.contains("param_servics"),
            "{e}"
        );
    }

    #[test]
    fn write_beside_writes_and_a_switch_removed_deletes_the_file() {
        let dir = std::env::temp_dir().join(format!("nros-overlay-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let st = dir.join("system.toml");
        let model = dir.join("system_model.yaml");
        std::fs::write(&st, "[system]\nfeatures = [\"param_services\"]\n").unwrap();
        let written = write_beside(&model, &st).unwrap().expect("written");
        assert_eq!(written, dir.join(OVERLAY_FILE));
        let mtime = std::fs::metadata(&written).unwrap().modified().unwrap();
        // Same content: the file is not rewritten.
        write_beside(&model, &st).unwrap();
        assert_eq!(
            std::fs::metadata(&written).unwrap().modified().unwrap(),
            mtime
        );
        // Switch removed: the stale file goes.
        std::fs::write(&st, "[system]\nname = \"t\"\n").unwrap();
        assert_eq!(write_beside(&model, &st).unwrap(), None);
        assert!(!written.exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    // ---- phase-486 W3: the reader side ----

    use ros_launch_manifest_model::{Autostart, Deploy, NodeInstance, SystemModel};

    fn nodes(vals: &[(&str, Option<Autostart>)]) -> SystemModel {
        let mut m = SystemModel::default();
        for (fqn, a) in vals {
            m.structure.nodes.insert(
                (*fqn).to_string(),
                NodeInstance {
                    lifecycle_autostart: *a,
                    ..NodeInstance::default()
                },
            );
        }
        m
    }

    #[test]
    fn nodes_that_agree_win_over_the_overlay_default() {
        let m = nodes(&[
            ("/a", Some(Autostart::Active)),
            ("/b", None),
            ("/c", Some(Autostart::Active)),
        ]);
        let o = Overlay {
            lifecycle_autostart: Some("configure".into()),
            ..Overlay::default()
        };
        assert_eq!(
            reduce_lifecycle_autostart(&m, &o).unwrap(),
            Some(Autostart::Active)
        );
    }

    #[test]
    fn no_node_stating_one_takes_the_overlay_default() {
        let m = nodes(&[("/a", None)]);
        let o = Overlay {
            lifecycle_autostart: Some("configure".into()),
            ..Overlay::default()
        };
        assert_eq!(
            reduce_lifecycle_autostart(&m, &o).unwrap(),
            Some(Autostart::Configure)
        );
        assert_eq!(
            reduce_lifecycle_autostart(&m, &Overlay::default()).unwrap(),
            None
        );
    }

    /// The first-node-wins readers this replaces would have answered `None`
    /// here (BTreeMap order puts `/a` first) and silently booted `/b` wrong.
    #[test]
    fn two_nodes_that_disagree_are_refused_by_name() {
        let m = nodes(&[
            ("/a", Some(Autostart::None)),
            ("/b", Some(Autostart::Active)),
        ]);
        let e = reduce_lifecycle_autostart(&m, &Overlay::default()).unwrap_err();
        assert!(e.contains("/a") && e.contains("/b"), "{e}");
    }

    #[test]
    fn placement_is_membership_of_a_non_empty_map() {
        let mut m = nodes(&[("/a", None), ("/b", None)]);
        assert!(
            placed_here(&m, "/a") && placed_here(&m, "/b"),
            "an empty map places everything"
        );
        m.execution.deploy.insert("/a".into(), Deploy::default());
        assert!(placed_here(&m, "/a"));
        assert!(
            !placed_here(&m, "/b"),
            "absent from a non-empty map = placed elsewhere"
        );
    }

    #[test]
    fn a_model_carrying_switches_with_no_overlay_is_refused() {
        let dir = std::env::temp_dir().join(format!("nros-overlay-load-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("system_model.yaml");
        // The shape an rlm before `v0.1.55` wrote: switches in the model.
        std::fs::write(
            &path,
            "meta:\n  version: 2\nstructure: {}\nexecution:\n  features:\n  - param_services\n",
        )
        .unwrap();
        let e = load_beside(&path).unwrap_err();
        assert!(
            e.contains("nros sync") && e.contains("param_services"),
            "{e}"
        );
        // No switches in the model and no file: nothing stated, no error.
        std::fs::write(&path, "meta:\n  version: 2\nstructure: {}\n").unwrap();
        assert_eq!(load_beside(&path).unwrap(), Overlay::default());
        // With the file, the file answers, whatever the model carries.
        std::fs::write(
            &path,
            "meta:\n  version: 2\nstructure: {}\nexecution:\n  features:\n  - param_services\n",
        )
        .unwrap();
        std::fs::write(
            dir.join(OVERLAY_FILE),
            "[meta]\nversion = 1\n[system]\nfeatures = [\"safety\"]\n",
        )
        .unwrap();
        assert_eq!(load_beside(&path).unwrap().features, vec!["safety"]);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn the_legacy_projection_moves_switches_out_of_the_model() {
        let l = LoadedModel::from_legacy_yaml(
            "meta:\n  version: 2\nstructure: {}\nexecution:\n  features:\n  - param_services\n",
        )
        .unwrap();
        assert_eq!(l.features(None), vec!["param_services"]);
        assert!(legacy_projected_features("meta:\n  version: 2\nstructure: {}\n").is_empty());
        assert!(legacy_projected_features("not: [a model").is_empty());
    }
}
