//! RFC-0104 D3 (phase-485 W2) — a leaf `Cargo.lock` is SEEDED from the
//! registry pin set, never resolved from scratch.
//!
//! The index names the pin set (`[rust] registry_pins`): a registry-only
//! `Cargo.lock` holding one version per semver-compatible range of every
//! registry crate any root in the tree resolves. A leaf with no lock of its own
//! gets a copy of it, and cargo then resolves the leaf against that starting
//! point. Measured in phase-485 M1 and re-checked in W1: cargo KEEPS every
//! seeded version the leaf's manifests accept, PRUNES the rest, and adds the
//! leaf's path crates (generated message crates are `0.0.0`, so they move no
//! registry entry). `nros` decides the starting point; cargo does the
//! resolution (D8) — nothing here parses or edits a resolve.
//!
//! An EXISTING lock is never touched: it is either a previous seed, a lock the
//! user moved on purpose, or a tracked one. Whether it still agrees with the
//! pins is the drift check's question (D9), not this one's.

use std::path::{Path, PathBuf};

use eyre::{Result, WrapErr};

use crate::orchestration::sdk_index::SdkIndex;

/// The index file a nano-ros root carries.
const INDEX_FILE: &str = "nros-sdk-index.toml";

/// What [`seed`] did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Seed {
    /// The leaf already had a `Cargo.lock`; it was left alone.
    Present,
    /// A copy of the pin set was written as the leaf's `Cargo.lock`.
    Seeded { pins: PathBuf },
    /// No pin set to seed from: no nano-ros root, an index that names none, or
    /// a named file that is absent. Cargo resolves fresh, as before RFC-0104.
    NoPinSet,
}

/// The registry pin set a nano-ros root's index names, if it names one that
/// exists. The path is relative to the index's directory.
pub fn pin_file(nano_ros_root: &Path) -> Result<Option<PathBuf>> {
    let index_path = nano_ros_root.join(INDEX_FILE);
    if !index_path.is_file() {
        return Ok(None);
    }
    let index = SdkIndex::load(&index_path)?;
    Ok(index
        .rust
        .registry_pins
        .map(|rel| nano_ros_root.join(rel))
        .filter(|p| p.is_file()))
}

/// The pin set a project resolves against: the SDK's `pins` with the project's
/// own `[cargo_override.*]` (phase-485 W10) replacing the pin in each
/// override's semver range.
#[must_use]
pub fn effective(
    pins: &[RegistryPkg],
    overrides: &std::collections::BTreeMap<String, crate::orchestration::sdk_store::CargoOverride>,
) -> Vec<RegistryPkg> {
    let mut out: Vec<RegistryPkg> = pins
        .iter()
        .filter(|p| {
            overrides
                .get(&p.name)
                .is_none_or(|o| compat_range(&o.version) != compat_range(&p.version))
        })
        .cloned()
        .collect();
    for (name, o) in overrides {
        out.push(RegistryPkg {
            name: name.clone(),
            version: o.version.clone(),
            source: o.source.clone(),
            checksum: o.checksum.clone(),
            dependencies: Vec::new(),
        });
    }
    out
}

/// The `[cargo_override.*]` of the project lock `dir` belongs to.
pub fn project_overrides(
    dir: &Path,
) -> Result<std::collections::BTreeMap<String, crate::orchestration::sdk_store::CargoOverride>> {
    let lock = crate::orchestration::sdk_store::SdkLock::load(
        &crate::orchestration::store::lock_path_for(dir),
    )?;
    Ok(lock.cargo_override)
}

/// Seed `<manifest_dir>/Cargo.lock` from the pin set when the leaf has none —
/// with the project's own overrides laid over it (phase-485 W10).
pub fn seed(manifest_dir: &Path, nano_ros_root: Option<&Path>) -> Result<Seed> {
    let lock = manifest_dir.join("Cargo.lock");
    if lock.exists() {
        return Ok(Seed::Present);
    }
    let Some(root) = nano_ros_root else {
        return Ok(Seed::NoPinSet);
    };
    let Some(pins) = pin_file(root)? else {
        return Ok(Seed::NoPinSet);
    };
    write_seed(manifest_dir, &pins)?;
    Ok(Seed::Seeded { pins })
}

/// Write `<manifest_dir>/Cargo.lock` from `pins`, with the project's own
/// overrides laid over it, replacing whatever lock is there.
pub fn write_seed(manifest_dir: &Path, pins: &Path) -> Result<()> {
    let lock = manifest_dir.join("Cargo.lock");
    let overrides = project_overrides(manifest_dir)?;
    if overrides.is_empty() {
        std::fs::copy(pins, &lock)
            .wrap_err_with(|| format!("seed {} from {}", lock.display(), pins.display()))?;
    } else {
        let base = parse_registry(&std::fs::read_to_string(pins)?)?;
        std::fs::write(&lock, render(&effective(&base, &overrides)))
            .wrap_err_with(|| format!("seed {}", lock.display()))?;
    }
    Ok(())
}

/// One registry entry of a `Cargo.lock`, as the pin set holds it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RegistryPkg {
    pub name: String,
    pub version: String,
    pub source: String,
    pub checksum: Option<String>,
    pub dependencies: Vec<String>,
}

/// Cargo's semver-compatibility key: `1.4.2` -> `1`, `0.8.3` -> `0.8`,
/// `0.0.5` -> `0.0.5`. Two versions with the same key cannot coexist in one
/// resolve, so the pin set holds at most one per key.
#[must_use]
pub fn compat_range(version: &str) -> String {
    let (ma, mi, pa) = version_key(version);
    if ma > 0 {
        format!("{ma}")
    } else if mi > 0 {
        format!("0.{mi}")
    } else {
        format!("0.0.{pa}")
    }
}

/// `major.minor.patch`, ignoring pre-release and build metadata.
#[must_use]
pub fn version_key(version: &str) -> (u64, u64, u64) {
    let core = version.split(['+', '-']).next().unwrap_or(version);
    let mut it = core.split('.').map(|p| p.parse::<u64>().unwrap_or(0));
    (
        it.next().unwrap_or(0),
        it.next().unwrap_or(0),
        it.next().unwrap_or(0),
    )
}

/// The registry entries of a `Cargo.lock`'s text.
pub fn parse_registry(text: &str) -> Result<Vec<RegistryPkg>> {
    let doc: toml::Value = toml::from_str(text).wrap_err("parse Cargo.lock")?;
    let Some(pkgs) = doc.get("package").and_then(|p| p.as_array()) else {
        return Ok(Vec::new());
    };
    let s = |p: &toml::Value, k: &str| p.get(k).and_then(|v| v.as_str()).map(str::to_string);
    Ok(pkgs
        .iter()
        .filter(|p| s(p, "source").is_some_and(|v| v.starts_with("registry+")))
        .map(|p| RegistryPkg {
            name: s(p, "name").unwrap_or_default(),
            version: s(p, "version").unwrap_or_default(),
            source: s(p, "source").unwrap_or_default(),
            checksum: s(p, "checksum"),
            dependencies: p
                .get("dependencies")
                .and_then(|d| d.as_array())
                .map(|a| {
                    a.iter()
                        .filter_map(|d| d.as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default(),
        })
        .collect())
}

/// The pin file's text: cargo's lock format (version 4), sorted by name then
/// version, so `cargo` can read it as a seed directly and a diff is a review.
#[must_use]
pub fn render(pkgs: &[RegistryPkg]) -> String {
    let mut sorted: Vec<&RegistryPkg> = pkgs.iter().collect();
    sorted.sort_by(|a, b| {
        a.name
            .cmp(&b.name)
            .then(version_key(&a.version).cmp(&version_key(&b.version)))
    });
    let mut s = String::from(
        "# The Rust registry pin set (RFC-0104 D2). GENERATED by `nros update` --\n\
         # one version per semver-compatible range of every registry crate a root in\n\
         # this tree resolves. Move a pin with `nros update --crate <name>`; never\n\
         # hand-edit.\n\
         version = 4\n",
    );
    for p in sorted {
        s.push_str(&format!(
            "\n[[package]]\nname = \"{}\"\nversion = \"{}\"\nsource = \"{}\"\n",
            p.name, p.version, p.source
        ));
        if let Some(c) = &p.checksum {
            s.push_str(&format!("checksum = \"{c}\"\n"));
        }
        if !p.dependencies.is_empty() {
            s.push_str("dependencies = [\n");
            for d in &p.dependencies {
                s.push_str(&format!(" \"{d}\",\n"));
            }
            s.push_str("]\n");
        }
    }
    s
}

/// Two locked roots holding different versions in one semver range.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Disagreement {
    pub name: String,
    pub range: String,
    /// `(lock path, version)` for each side.
    pub versions: Vec<(String, String)>,
}

/// The pin set from the locked roots' locks and the previous pin set.
///
/// One version per `(name, range)`: the locked roots' version where any root
/// holds that range (they must agree — a disagreement is returned, not
/// resolved, because which one is right is a decision); otherwise the previous
/// pin. Nothing is dropped: a range no root names stays pinned, because a leaf
/// not built here may still resolve it.
pub fn regenerate(
    roots: &[(String, Vec<RegistryPkg>)],
    previous: &[RegistryPkg],
) -> std::result::Result<Vec<RegistryPkg>, Vec<Disagreement>> {
    use std::collections::BTreeMap;
    let mut chosen: BTreeMap<(String, String), (String, RegistryPkg)> = BTreeMap::new();
    let mut bad: BTreeMap<(String, String), Disagreement> = BTreeMap::new();
    for (lock, pkgs) in roots {
        for p in pkgs {
            let key = (p.name.clone(), compat_range(&p.version));
            match chosen.get(&key) {
                Some((first, q)) if q.version != p.version => {
                    bad.entry(key.clone())
                        .or_insert_with(|| Disagreement {
                            name: p.name.clone(),
                            range: key.1.clone(),
                            versions: vec![(first.clone(), q.version.clone())],
                        })
                        .versions
                        .push((lock.clone(), p.version.clone()));
                }
                Some(_) => {}
                None => {
                    chosen.insert(key, (lock.clone(), p.clone()));
                }
            }
        }
    }
    if !bad.is_empty() {
        return Err(bad.into_values().collect());
    }
    for p in previous {
        chosen
            .entry((p.name.clone(), compat_range(&p.version)))
            .or_insert_with(|| (String::new(), p.clone()));
    }
    Ok(chosen.into_values().map(|(_, p)| p).collect())
}

/// Issue 1873 — the keep list, beside the pin set: ranges no locked root
/// resolves that a LEAF needs, each with why. Also read by the Python gate
/// (`scripts/lib/cargo_pins.py`).
pub const LEAF_ONLY_PINS: &str = "locks/leaf-only-pins.toml";

/// `(name, compat range)` pairs from the keep list's `[[keep]]` entries.
pub fn parse_keep(text: &str) -> Result<std::collections::BTreeSet<(String, String)>> {
    let doc: toml::Value = toml::from_str(text).wrap_err("parse the leaf-only pin list")?;
    let mut out = std::collections::BTreeSet::new();
    for k in doc
        .get("keep")
        .and_then(|v| v.as_array())
        .into_iter()
        .flatten()
    {
        let s = |f: &str| k.get(f).and_then(|v| v.as_str()).map(str::to_string);
        match (s("name"), s("range")) {
            (Some(n), Some(r)) => {
                out.insert((n, r));
            }
            _ => eyre::bail!("a [[keep]] entry needs `name` and `range`: {k}"),
        }
    }
    Ok(out)
}

/// Issue 1873 — drop the pins whose range no locked root resolves and the keep
/// list does not name. `regenerate` never drops anything, on purpose (a leaf
/// may need a range no root has), so a bump left its old range behind: issue
/// 1805 moved every root to quick-xml 0.41 and 0.37.5 stayed pinned.
///
/// Returns `(kept, dropped)`. Refuses when a kept pin depends on a dropped
/// one, since cargo would then seed an entry whose dependency is gone.
pub fn prune(
    pins: &[RegistryPkg],
    roots: &[Vec<RegistryPkg>],
    keep: &std::collections::BTreeSet<(String, String)>,
) -> Result<(Vec<RegistryPkg>, Vec<RegistryPkg>)> {
    let used: std::collections::BTreeSet<(String, String)> = roots
        .iter()
        .flatten()
        .map(|p| (p.name.clone(), compat_range(&p.version)))
        .collect();
    let (kept, dropped): (Vec<_>, Vec<_>) = pins.iter().cloned().partition(|p| {
        let key = (p.name.clone(), compat_range(&p.version));
        used.contains(&key) || keep.contains(&key)
    });
    for k in &kept {
        for d in &k.dependencies {
            // A dependency is `name` or, where several versions exist,
            // `name version`.
            let mut it = d.split_whitespace();
            let (dn, dv) = (it.next().unwrap_or_default(), it.next());
            let hit = dropped.iter().any(|x| {
                x.name == dn
                    && dv.is_none_or(|v| x.version == v)
                    && !kept
                        .iter()
                        .any(|y| y.name == dn && dv.is_none_or(|v| y.version == v))
            });
            if hit {
                eyre::bail!(
                    "pruning would leave {} {} depending on `{d}`, which no root resolves; \
                     keep that range in {LEAF_ONLY_PINS} or prune {} too",
                    k.name,
                    k.version,
                    k.name
                );
            }
        }
    }
    Ok((kept, dropped))
}

#[cfg(test)]
mod tests {
    fn dep_pkg(n: &str, v: &str, deps: &[&str]) -> RegistryPkg {
        RegistryPkg {
            name: n.into(),
            version: v.into(),
            source: "registry+https://github.com/rust-lang/crates.io-index".into(),
            checksum: None,
            dependencies: deps.iter().map(|d| (*d).to_string()).collect(),
        }
    }

    /// Issue 1873 — a superseded range goes, a kept one stays, and a kept pin
    /// may not lose its dependency.
    #[test]
    fn prune_drops_what_no_root_and_no_keep_entry_claims() {
        let pins = vec![
            dep_pkg("quick-xml", "0.37.5", &["memchr"]),
            dep_pkg("quick-xml", "0.41.0", &["memchr"]),
            dep_pkg("memchr", "2.7.4", &[]),
            dep_pkg("nom", "7.1.3", &["memchr", "minimal-lexical"]),
            dep_pkg("minimal-lexical", "0.2.1", &[]),
        ];
        let roots = vec![vec![
            dep_pkg("quick-xml", "0.41.0", &[]),
            dep_pkg("memchr", "2.7.4", &[]),
        ]];
        let mut keep = std::collections::BTreeSet::new();
        keep.insert(("nom".to_string(), "7".to_string()));
        keep.insert(("minimal-lexical".to_string(), "0.2".to_string()));
        let (kept, dropped) = prune(&pins, &roots, &keep).unwrap();
        assert_eq!(
            dropped
                .iter()
                .map(|p| p.version.as_str())
                .collect::<Vec<_>>(),
            ["0.37.5"]
        );
        assert_eq!(kept.len(), 4);

        // Keeping `nom` without its dependency is refused, not half-done.
        keep.remove(&("minimal-lexical".to_string(), "0.2".to_string()));
        let err = prune(&pins, &roots, &keep).unwrap_err().to_string();
        assert!(err.contains("minimal-lexical"), "{err}");
    }

    use super::*;

    const PINS: &str = "version = 4\n\n[[package]]\nname = \"heapless\"\nversion = \"0.8.0\"\n\
                        source = \"registry+https://github.com/rust-lang/crates.io-index\"\n";

    fn root_with_pins(dir: &Path) {
        std::fs::write(
            dir.join(INDEX_FILE),
            "[rust]\nregistry_pins = \"locks/cargo-registry.lock\"\n",
        )
        .unwrap();
        std::fs::create_dir_all(dir.join("locks")).unwrap();
        std::fs::write(dir.join("locks/cargo-registry.lock"), PINS).unwrap();
    }

    fn pkg(name: &str, version: &str) -> RegistryPkg {
        RegistryPkg {
            name: name.into(),
            version: version.into(),
            source: "registry+https://github.com/rust-lang/crates.io-index".into(),
            checksum: Some("ab".into()),
            dependencies: vec!["dep".into()],
        }
    }

    #[test]
    fn a_project_override_replaces_the_pin_in_its_range_only() {
        use crate::orchestration::sdk_store::CargoOverride;
        let pins = vec![
            pkg("libc", "0.2.186"),
            pkg("heapless", "0.8.0"),
            pkg("heapless", "0.9.3"),
        ];
        let ov = std::collections::BTreeMap::from([(
            "heapless".to_string(),
            CargoOverride {
                version: "0.8.1".into(),
                source: "registry+https://github.com/rust-lang/crates.io-index".into(),
                checksum: None,
            },
        )]);
        let mut v: Vec<(String, String)> = effective(&pins, &ov)
            .into_iter()
            .map(|p| (p.name, p.version))
            .collect();
        v.sort();
        assert_eq!(
            v,
            vec![
                ("heapless".into(), "0.8.1".into()),
                ("heapless".into(), "0.9.3".into()),
                ("libc".into(), "0.2.186".into()),
            ]
        );
    }

    #[test]
    fn ranges_follow_cargo_semver_compatibility() {
        assert_eq!(compat_range("1.4.2"), "1");
        assert_eq!(compat_range("0.8.3"), "0.8");
        assert_eq!(compat_range("0.0.5"), "0.0.5");
        assert_eq!(compat_range("1.0.3+wasi-0.2.9"), "1");
    }

    #[test]
    fn roots_win_previous_pins_persist_and_disagreement_is_returned() {
        let roots = vec![
            ("Cargo.lock".to_string(), vec![pkg("heapless", "0.8.1")]),
            (
                "cli/Cargo.lock".to_string(),
                vec![pkg("heapless", "0.8.1"), pkg("serde", "1.0.2")],
            ),
        ];
        let previous = vec![pkg("heapless", "0.8.0"), pkg("zzz", "0.1.0")];
        let got = regenerate(&roots, &previous).unwrap();
        let v: Vec<(&str, &str)> = got
            .iter()
            .map(|p| (p.name.as_str(), p.version.as_str()))
            .collect();
        assert_eq!(
            v,
            vec![("heapless", "0.8.1"), ("serde", "1.0.2"), ("zzz", "0.1.0")]
        );

        let split = vec![
            ("a".to_string(), vec![pkg("heapless", "0.8.0")]),
            ("b".to_string(), vec![pkg("heapless", "0.8.1")]),
        ];
        let e = regenerate(&split, &[]).unwrap_err();
        assert_eq!(e.len(), 1);
        assert_eq!(e[0].versions.len(), 2);
    }

    #[test]
    fn render_round_trips_through_parse() {
        let pkgs = vec![pkg("b", "1.0.0"), pkg("a", "0.2.0"), pkg("a", "0.10.0")];
        let text = render(&pkgs);
        let back = parse_registry(&text).unwrap();
        let v: Vec<(&str, &str)> = back
            .iter()
            .map(|p| (p.name.as_str(), p.version.as_str()))
            .collect();
        // Numeric, not lexical, version order.
        assert_eq!(v, vec![("a", "0.2.0"), ("a", "0.10.0"), ("b", "1.0.0")]);
        assert_eq!(back[0].dependencies, vec!["dep".to_string()]);
    }

    #[test]
    fn a_leaf_with_no_lock_gets_a_copy_of_the_pin_set() {
        let root = tempfile::tempdir().unwrap();
        root_with_pins(root.path());
        let leaf = tempfile::tempdir().unwrap();
        let got = seed(leaf.path(), Some(root.path())).unwrap();
        assert_eq!(
            got,
            Seed::Seeded {
                pins: root.path().join("locks/cargo-registry.lock")
            }
        );
        assert_eq!(
            std::fs::read_to_string(leaf.path().join("Cargo.lock")).unwrap(),
            PINS
        );
    }

    #[test]
    fn an_existing_lock_is_never_reseeded() {
        let root = tempfile::tempdir().unwrap();
        root_with_pins(root.path());
        let leaf = tempfile::tempdir().unwrap();
        std::fs::write(leaf.path().join("Cargo.lock"), "version = 4\n").unwrap();
        assert_eq!(seed(leaf.path(), Some(root.path())).unwrap(), Seed::Present);
        assert_eq!(
            std::fs::read_to_string(leaf.path().join("Cargo.lock")).unwrap(),
            "version = 4\n"
        );
    }

    #[test]
    fn no_pin_set_leaves_the_resolve_to_cargo() {
        let leaf = tempfile::tempdir().unwrap();
        assert_eq!(seed(leaf.path(), None).unwrap(), Seed::NoPinSet);
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join(INDEX_FILE), "").unwrap();
        assert_eq!(
            seed(leaf.path(), Some(root.path())).unwrap(),
            Seed::NoPinSet
        );
        assert!(!leaf.path().join("Cargo.lock").exists());
    }
}
