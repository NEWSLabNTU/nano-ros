//! Issue 1895 — the launch slice of a bringup's `system.toml`, parsed ONE way:
//! `[system] default_launch` and the `[[model]]` declarations.
//!
//! Three readers used to parse it, two of them as raw `toml` values: this
//! crate's `model_location::launch_to_model_rel`, and `nros sync`'s model
//! resolution (`cmd/ws.rs`), each walking `get("model")…as_str()` by hand. The
//! typed field in the CLI's `SystemToml` had no reader at all. Three parsers of
//! one table drift independently — a renamed key lands in one of them — and
//! the raw ones also failed OPEN: a `[[model]]` row missing `out`, or an `args`
//! value that is not a string, was dropped without a word, so a declared
//! binding variant could silently stop existing.
//!
//! The typed slice lives HERE because this crate sits below `nros-cli-core` in
//! the graph: the CLI's full-schema `SystemToml` holds `[[model]]` rows of THIS
//! type, so the schema of a row is stated once, and both crates read it.
//!
//! The slice reads only its own keys. The rest of `system.toml` is the CLI
//! schema's to check (`deny_unknown_fields` there); refusing it here would make
//! this reader a second, partial copy of that schema.

use std::{collections::BTreeMap, path::Path};

use serde::{Deserialize, Serialize};

/// The launch file a bringup resolves when `[system] default_launch` names
/// none.
pub const DEFAULT_LAUNCH_FILE: &str = "system.launch.xml";

/// phase-330 W4.0 — one `[[model]]` declaration: which launch file, with which
/// argument bindings, produces which model file.
///
/// ```toml
/// [[model]]
/// launch = "multihost.launch.xml"
/// out    = "multihost_robot1_model.yaml"
/// args   = { host = "robot1" }
/// ```
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelDecl {
    /// Launch file name, relative to the bringup's `launch/`.
    pub launch: String,
    /// Output model file name (`<stem>_<binding>_model.yaml` by convention).
    pub out: String,
    /// Launch argument bindings, passed as `key:=value`.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub args: BTreeMap<String, String>,
}

/// The `[system] default_launch` + `[[model]]` slice of one `system.toml`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SystemLaunch {
    /// `[system] default_launch`, as authored.
    pub default_launch: Option<String>,
    /// Every `[[model]]` row, in file order.
    pub models: Vec<ModelDecl>,
}

#[derive(Deserialize)]
struct View {
    #[serde(default)]
    system: Option<SystemView>,
    #[serde(default, rename = "model")]
    models: Vec<ModelDecl>,
}

#[derive(Deserialize)]
struct SystemView {
    #[serde(default)]
    default_launch: Option<String>,
}

impl SystemLaunch {
    /// Parse the slice out of a `system.toml` body.
    pub fn from_toml_str(raw: &str) -> Result<Self, String> {
        let v: View = toml::from_str(raw).map_err(|e| e.to_string())?;
        Ok(Self {
            default_launch: v.system.and_then(|s| s.default_launch),
            models: v.models,
        })
    }

    /// Read `<path>`. An ABSENT file is an empty slice — a bringup with no
    /// `system.toml` declares nothing, and the derived defaults apply. A file
    /// that exists and does not parse is an error naming it, never an empty
    /// slice: that is the fail-open this module replaced.
    pub fn read(path: &Path) -> Result<Self, String> {
        let raw = match std::fs::read_to_string(path) {
            Ok(raw) => raw,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(e) => return Err(format!("reading `{}`: {e}", path.display())),
        };
        Self::from_toml_str(&raw).map_err(|e| format!("`{}`: {e}", path.display()))
    }

    /// The launch file this bringup resolves by default.
    #[must_use]
    pub fn default_launch(&self) -> &str {
        self.default_launch
            .as_deref()
            .unwrap_or(DEFAULT_LAUNCH_FILE)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_default_launch_and_models_and_ignores_the_rest() {
        let s = SystemLaunch::from_toml_str(
            r#"
[system]
name = "demo"
default_launch = "talker_only.launch.xml"

[[component]]
pkg = "x"

[[model]]
launch = "multihost.launch.xml"
out = "multihost_robot1_model.yaml"
args = { host = "robot1" }

[[model]]
launch = "solo.launch.xml"
out = "solo_model.yaml"
"#,
        )
        .expect("parses");
        assert_eq!(s.default_launch(), "talker_only.launch.xml");
        assert_eq!(s.models.len(), 2);
        assert_eq!(
            s.models[0].args.get("host").map(String::as_str),
            Some("robot1")
        );
        assert!(s.models[1].args.is_empty());
    }

    #[test]
    fn no_default_launch_means_the_conventional_one() {
        let s = SystemLaunch::from_toml_str("[system]\nname = \"demo\"\n").unwrap();
        assert_eq!(s.default_launch(), DEFAULT_LAUNCH_FILE);
        assert!(s.models.is_empty());
    }

    /// The rows the raw readers dropped without a word are refused.
    #[test]
    fn a_malformed_model_row_is_an_error_not_a_missing_row() {
        for (bad, what) in [
            ("[[model]]\nlaunch = \"a.launch.xml\"\n", "out"),
            (
                "[[model]]\nlaunch = \"a.launch.xml\"\nout = \"a.yaml\"\nargs = { n = 1 }\n",
                "args",
            ),
            (
                "[[model]]\nlaunch = \"a.launch.xml\"\nout = \"a.yaml\"\nvariant = \"x\"\n",
                "variant",
            ),
        ] {
            let e = SystemLaunch::from_toml_str(bad).expect_err(bad);
            assert!(e.contains(what), "{what}: {e}");
        }
    }

    /// The fixture manifest reads `[system] default_launch` in Python, because
    /// its validation runs on the fast line, where no `nros` binary is built to
    /// ask (issue 1895). It is the one reader left outside this module, so the
    /// two facts it restates are held to this one: the key, and the default.
    #[test]
    fn the_fixture_manifest_reads_the_same_key_and_default() {
        let repo = Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(3)
            .expect("checkout root");
        let py = std::fs::read_to_string(repo.join("scripts/build/fixtures-manifest.py"))
            .expect("fixtures-manifest.py");
        assert!(
            py.contains(&format!("DEFAULT_LAUNCH_FILE = \"{DEFAULT_LAUNCH_FILE}\"")),
            "fixtures-manifest.py's DEFAULT_LAUNCH_FILE is not {DEFAULT_LAUNCH_FILE:?}"
        );
        assert!(
            py.contains("system.get(\"default_launch\")"),
            "fixtures-manifest.py no longer reads `[system] default_launch`"
        );
    }

    #[test]
    fn an_absent_file_is_empty_and_an_unparsable_one_names_itself() {
        let tmp = std::env::temp_dir().join(format!("nros-system-launch-{}", std::process::id()));
        std::fs::create_dir_all(&tmp).unwrap();
        let path = tmp.join("system.toml");
        let _ = std::fs::remove_file(&path);
        assert_eq!(SystemLaunch::read(&path).unwrap(), SystemLaunch::default());
        std::fs::write(&path, "[system\n").unwrap();
        let e = SystemLaunch::read(&path).unwrap_err();
        assert!(e.contains("system.toml"), "{e}");
        std::fs::remove_dir_all(&tmp).unwrap();
    }
}
