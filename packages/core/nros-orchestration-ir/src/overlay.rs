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

use std::path::{Path, PathBuf};

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
}

impl Overlay {
    /// True when the overlay states nothing, so no file is written.
    pub fn is_empty(&self) -> bool {
        self.features.is_empty() && self.lifecycle_autostart.is_none()
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
        Ok(Self {
            features,
            lifecycle_autostart,
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
        out
    }

    /// Parse a written overlay. Refuses a newer schema and unknown keys: this
    /// file is ours, so anything unexpected in it is a defect, not a choice.
    pub fn parse(text: &str, origin: &Path) -> Result<Self, String> {
        let doc: toml::Table =
            toml::from_str(text).map_err(|e| format!("{}: cannot parse: {e}", origin.display()))?;
        for k in doc.keys() {
            if !matches!(k.as_str(), "meta" | "system" | "lifecycle") {
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
        Ok(Self {
            features,
            lifecycle_autostart,
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
            Overlay::parse("[meta]\nversion = 1\n[image]\n", p)
                .unwrap_err()
                .contains("unknown")
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
}
