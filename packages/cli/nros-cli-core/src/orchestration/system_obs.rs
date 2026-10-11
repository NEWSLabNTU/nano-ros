//! RFC-0104 D6 (phase-485 W6) — system libraries are RECORDED and WARNED
//! about, never enforced.
//!
//! The build system asks one question of an environment (issue 1248); pinning
//! the host's ROS would encode it. So `nros build` records what it OBSERVED in
//! `[system.*]` of the project lock and, when an observation moved since the
//! last build, says so ONCE, naming what moved and what it regenerated:
//!
//! ```text
//! nano-ros: system changed since the last build (nros-sdk.lock):
//!   std_msgs        4.9.2 -> 4.9.3   regenerated: std_msgs
//!   rmw_zenoh_cpp   0.1.1 -> 0.1.9
//! ```
//!
//! A warning is only useful against a declared range, so the index states
//! COMPATIBILITY RANGES (`[compat.<name>]`) for the system pieces a pinned
//! piece must interoperate with. An observation outside its range warns LOUDLY,
//! on every build while it stays outside, naming the range and why it exists —
//! issue 0609's drift (ROS's zenoh moving under a pinned zenoh-pico), caught at
//! build time instead of as a silent interop failure.
//!
//! What is observed is DATA, not a list in this file: each `[compat.*]` row
//! names an ament package (its `package.xml` version) or a CMake package (its
//! `<Pkg>ConfigVersion.cmake`), and every ament package a build generated
//! interfaces from is observed too.

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use crate::orchestration::registry_pins::version_key;

/// `[system.<name>]` — one observed system package.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SystemRecord {
    pub version: String,
}

/// The ament prefixes, FIRST ENTRY WINS (issue 1781 — ament's own rule).
#[must_use]
pub fn ament_prefixes() -> Vec<PathBuf> {
    std::env::var("AMENT_PREFIX_PATH")
        .unwrap_or_default()
        .split(':')
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .collect()
}

/// `<version>` from an ament package's `package.xml`, first prefix that has it.
#[must_use]
pub fn ament_version(pkg: &str, prefixes: &[PathBuf]) -> Option<String> {
    prefixes.iter().find_map(|p| {
        let text = std::fs::read_to_string(p.join("share").join(pkg).join("package.xml")).ok()?;
        package_xml_version(&text)
    })
}

/// The text of the first `<version>` element.
#[must_use]
pub fn package_xml_version(text: &str) -> Option<String> {
    let i = text.find("<version>")? + "<version>".len();
    let j = text[i..].find("</version>")? + i;
    Some(text[i..j].trim().to_string())
}

/// `PACKAGE_VERSION` from a CMake package's `<Pkg>ConfigVersion.cmake`, looked
/// up under each prefix's `lib/cmake` and `lib/<multiarch>/cmake`.
#[must_use]
pub fn cmake_version(pkg: &str, prefixes: &[PathBuf]) -> Option<String> {
    let file = format!("{pkg}ConfigVersion.cmake");
    prefixes.iter().find_map(|p| {
        let mut dirs = vec![p.join("lib/cmake").join(pkg)];
        if let Ok(entries) = std::fs::read_dir(p.join("lib")) {
            dirs.extend(entries.flatten().map(|e| e.path().join("cmake").join(pkg)));
        }
        dirs.into_iter().find_map(|d| {
            let text = std::fs::read_to_string(d.join(&file)).ok()?;
            let i = text.find("set(PACKAGE_VERSION \"")? + "set(PACKAGE_VERSION \"".len();
            let j = text[i..].find('"')? + i;
            Some(text[i..j].to_string())
        })
    })
}

/// Is `version` inside `range` — comma-separated `>=`, `>`, `<=`, `<`, `=`
/// bounds on `major.minor.patch`? `None` when the range does not parse.
#[must_use]
pub fn in_range(version: &str, range: &str) -> Option<bool> {
    let v = version_key(version);
    let mut ok = true;
    for part in range.split(',').map(str::trim).filter(|s| !s.is_empty()) {
        let (op, rest) = ["<=", ">=", "<", ">", "="]
            .iter()
            .find_map(|op| part.strip_prefix(op).map(|r| (*op, r.trim())))?;
        let b = version_key(rest);
        ok &= match op {
            "<=" => v <= b,
            ">=" => v >= b,
            "<" => v < b,
            ">" => v > b,
            _ => v == b,
        };
    }
    Some(ok)
}

/// One observation and the range it is held to, if any.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Observed {
    pub name: String,
    pub version: String,
    pub range: Option<(String, String)>,
}

/// Compare `observed` with the lock's previous `[system.*]` and return the
/// lines to print: one block for what MOVED (once — the lock is updated), and a
/// loud line per observation OUTSIDE its range (every build while it is).
#[must_use]
pub fn report(
    previous: &BTreeMap<String, SystemRecord>,
    observed: &[Observed],
    regenerated: &[String],
) -> Vec<String> {
    let mut moved = Vec::new();
    for o in observed {
        if let Some(prev) = previous.get(&o.name)
            && prev.version != o.version
        {
            let regen = if regenerated.contains(&o.name) {
                format!("   regenerated: {}", o.name)
            } else {
                String::new()
            };
            moved.push(format!(
                "  {:<16} {} -> {}{regen}",
                o.name, prev.version, o.version
            ));
        }
    }
    let mut out = Vec::new();
    if !moved.is_empty() {
        out.push("nano-ros: system changed since the last build (nros-sdk.lock):".to_string());
        out.extend(moved);
    }
    for o in observed {
        if let Some((range, why)) = &o.range
            && in_range(&o.version, range) == Some(false)
        {
            out.push(format!(
                "nano-ros: WARNING — {} {} is OUTSIDE its declared range `{range}`: {why}",
                o.name, o.version
            ));
        }
    }
    out
}

/// Observe what the index's `[compat.*]` rows name, plus each GENERATED
/// interface package by name (its `package.xml` version through `prefixes`),
/// against `prefixes`.
#[must_use]
pub fn observe(
    compat: &BTreeMap<String, crate::orchestration::sdk_index::CompatEntry>,
    generated: &[String],
    prefixes: &[PathBuf],
) -> Vec<Observed> {
    let mut out = Vec::new();
    for (name, c) in compat {
        let version = match (&c.ament, &c.cmake) {
            (Some(a), _) => ament_version(a, prefixes),
            (None, Some(m)) => cmake_version(m, prefixes),
            (None, None) => None,
        };
        if let Some(version) = version {
            out.push(Observed {
                name: name.clone(),
                version,
                range: Some((c.range.clone(), c.why.clone())),
            });
        }
    }
    for pkg in generated {
        if out.iter().any(|o| &o.name == pkg) {
            continue;
        }
        if let Some(version) = ament_version(pkg, prefixes) {
            out.push(Observed {
                name: pkg.clone(),
                version,
                range: None,
            });
        }
    }
    out
}

/// The interface packages `nros sync` generated for the workspace at `ws`:
/// each `<ws>/generated/<pkg>` carrying the per-package record issue 1780's fix
/// writes (`nros-generated.toml`). A workspace-local package is skipped by
/// [`observe`] (no ament `package.xml` to read), which is right: it is the
/// user's source, not the host's.
#[must_use]
pub fn generated_names(ws: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(ws.join("generated")) else {
        return Vec::new();
    };
    let mut out: Vec<String> = entries
        .flatten()
        .filter(|e| e.path().join("nros-generated.toml").is_file())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    out.sort();
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ranges_bound_both_ways() {
        assert_eq!(in_range("0.1.9", ">=0.1.1, <0.3"), Some(true));
        assert_eq!(in_range("0.3.0", ">=0.1.1, <0.3"), Some(false));
        assert_eq!(in_range("0.1.0", ">=0.1.1, <0.3"), Some(false));
        assert_eq!(in_range("0.10.5", ">=0.10, <0.11"), Some(true));
        assert_eq!(
            in_range("1.0", "~1"),
            None,
            "an unknown operator is not a verdict"
        );
    }

    #[test]
    fn a_move_is_reported_once_and_an_escape_every_time() {
        let prev = BTreeMap::from([
            (
                "std_msgs".to_string(),
                SystemRecord {
                    version: "4.9.2".into(),
                },
            ),
            (
                "rmw_zenoh_cpp".to_string(),
                SystemRecord {
                    version: "0.1.1".into(),
                },
            ),
        ]);
        let obs = vec![
            Observed {
                name: "std_msgs".into(),
                version: "4.9.3".into(),
                range: None,
            },
            Observed {
                name: "rmw_zenoh_cpp".into(),
                version: "0.3.0".into(),
                range: Some((">=0.1.1, <0.3".into(), "zenoh 1.x".into())),
            },
        ];
        let lines = report(&prev, &obs, &["std_msgs".to_string()]);
        assert!(lines[0].starts_with("nano-ros: system changed"));
        assert!(
            lines
                .iter()
                .any(|l| l.contains("4.9.2 -> 4.9.3") && l.contains("regenerated: std_msgs"))
        );
        assert!(
            lines
                .iter()
                .any(|l| l.contains("OUTSIDE") && l.contains("zenoh 1.x"))
        );
        // Nothing moved since: only the standing escape is said again.
        let now = BTreeMap::from([
            (
                "std_msgs".to_string(),
                SystemRecord {
                    version: "4.9.3".into(),
                },
            ),
            (
                "rmw_zenoh_cpp".to_string(),
                SystemRecord {
                    version: "0.3.0".into(),
                },
            ),
        ]);
        let again = report(&now, &obs, &[]);
        assert_eq!(again.len(), 1);
        assert!(again[0].contains("OUTSIDE"));
    }

    #[test]
    fn versions_are_read_from_package_xml_and_cmake_config() {
        let t = tempfile::tempdir().unwrap();
        let p = t.path();
        std::fs::create_dir_all(p.join("share/rmw_zenoh_cpp")).unwrap();
        std::fs::write(
            p.join("share/rmw_zenoh_cpp/package.xml"),
            "<package><name>rmw_zenoh_cpp</name><version>0.1.9</version></package>",
        )
        .unwrap();
        std::fs::create_dir_all(p.join("lib/x86_64-linux-gnu/cmake/CycloneDDS")).unwrap();
        std::fs::write(
            p.join("lib/x86_64-linux-gnu/cmake/CycloneDDS/CycloneDDSConfigVersion.cmake"),
            "set(PACKAGE_VERSION \"0.10.5\")\n",
        )
        .unwrap();
        let pre = vec![p.to_path_buf()];
        assert_eq!(
            ament_version("rmw_zenoh_cpp", &pre).as_deref(),
            Some("0.1.9")
        );
        assert_eq!(cmake_version("CycloneDDS", &pre).as_deref(), Some("0.10.5"));
        assert_eq!(ament_version("absent", &pre), None);
    }
}
