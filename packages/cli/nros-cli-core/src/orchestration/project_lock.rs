//! RFC-0104 D4 (phase-485 W3) — the project lock records what a BUILD used,
//! across every kind of dependency, not just tools.
//!
//! `nros-sdk.lock` held `[tool.*]` only. It now also holds:
//!
//! * `[source.<name>]` — each vendored C/C++ source tree the build READ, at
//!   the commit it was read at, and from which rung of the location ladder;
//! * `[west.<module>]` — each Zephyr module the build used, at its revision
//!   (RFC-0104 D4a);
//! * `[cargo.<root>]` — each cargo root's REGISTRY set, by hash (the leaf
//!   `Cargo.lock` beside the root is the diffable set, so the lock carries only
//!   enough to notice it moved — RFC-0104 open question 3).
//!
//! `[source]` and `[west]` are DERIVED from what the build recorded, never
//! from what the index declares (phase-485 M3): an image's set is the union of
//! its cargo half's dep-info and build-script `rerun-if-changed` lines, its
//! ninja dependency records, and a Zephyr build's own `zephyr_modules.txt`.
//! Declared is not used — `mbedtls` is in the index and no measured image read
//! it.
//!
//! Recording never stops a build: a scan that cannot read something records
//! less, and says nothing (the record is a reproducibility aid, and the drift
//! check — not this — is what refuses).

use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    process::Command,
};

use serde::{Deserialize, Serialize};

/// `[source.<name>]` — one vendored tree a build read.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceRecord {
    /// The commit the tree was at, or the index `ref` for a tree that is not a
    /// git checkout (a store tarball).
    pub commit: String,
    /// The location rung that answered (`checkout`, `store`, `local-edit`,
    /// `env`, `project`, `arg`).
    pub origin: String,
    /// The tree had uncommitted changes when the build read it.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub dirty: bool,
}

/// `[west.<module>]` — one Zephyr module a build used.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WestRecord {
    pub revision: String,
}

/// `[cargo.<root>]` — one cargo root's registry set.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CargoRecord {
    /// `sha256:<hex>` over the sorted `name version checksum` lines of the
    /// root's registry entries.
    pub registry: String,
    /// How many registry entries that is.
    pub crates: usize,
}

/// Every path a build in `dirs` recorded reading, from cargo dep-info,
/// build-script `rerun-if-changed`, ninja's dependency log and Zephyr's module
/// list. Canonicalised where the path exists.
pub fn paths_read(dirs: &[PathBuf]) -> BTreeSet<PathBuf> {
    let mut out = BTreeSet::new();
    for dir in dirs {
        walk(dir, 0, &mut out);
    }
    out.into_iter()
        .map(|p| std::fs::canonicalize(&p).unwrap_or(p))
        .collect()
}

/// Recurse a build directory for the record files. Bounded: a target dir is a
/// few levels deep, and a symlink loop must not hang a build's epilogue.
fn walk(dir: &Path, depth: usize, out: &mut BTreeSet<PathBuf>) {
    if depth > 8 {
        return;
    }
    // A cmake build tree keeps its compile dependencies in ninja's binary log,
    // not in `.d` files, so ask ninja wherever the walk meets one.
    if dir.join("build.ninja").is_file() {
        ninja_deps(dir, out);
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let Ok(ft) = e.file_type() else { continue };
        let p = e.path();
        if ft.is_dir() {
            walk(&p, depth + 1, out);
            continue;
        }
        let name = e.file_name();
        let name = name.to_string_lossy();
        if name.ends_with(".d") {
            dep_info(&p, out);
        } else if name == "output"
            && p.parent()
                .is_some_and(|d| d.parent().is_some_and(|b| b.ends_with("build")))
        {
            build_script_output(&p, out);
        } else if name == "zephyr_modules.txt" {
            zephyr_modules(&p, out);
        }
    }
}

/// Make-style dep-info: `target: dep dep` rules, `\ ` escaping a space and a
/// trailing `\` continuing the rule (gcc writes those; rustc does not).
fn dep_info(path: &Path, out: &mut BTreeSet<PathBuf>) {
    let Ok(text) = std::fs::read_to_string(path) else {
        return;
    };
    let text = text.replace("\\\n", " ");
    for line in text.lines() {
        let Some((_, deps)) = line.split_once(": ") else {
            continue;
        };
        let mut cur = String::new();
        let mut chars = deps.chars().peekable();
        while let Some(c) = chars.next() {
            match c {
                '\\' if chars.peek() == Some(&' ') => {
                    cur.push(' ');
                    chars.next();
                }
                ' ' | '\\' => {
                    if !cur.is_empty() {
                        out.insert(PathBuf::from(std::mem::take(&mut cur)));
                    }
                }
                _ => cur.push(c),
            }
        }
        if !cur.is_empty() {
            out.insert(PathBuf::from(cur));
        }
    }
}

/// A build script's `output`: `cargo:rerun-if-changed=<path>` (either prefix).
fn build_script_output(path: &Path, out: &mut BTreeSet<PathBuf>) {
    let Ok(text) = std::fs::read_to_string(path) else {
        return;
    };
    for line in text.lines() {
        let rest = line
            .strip_prefix("cargo:rerun-if-changed=")
            .or_else(|| line.strip_prefix("cargo::rerun-if-changed="));
        if let Some(p) = rest {
            out.insert(PathBuf::from(p));
        }
    }
}

/// Zephyr's `zephyr_modules.txt`: `"<name>":"<path>":"<cmake dir>"…` lines.
/// The module DIRECTORY is the record: west modules are attributed by root.
fn zephyr_modules(path: &Path, out: &mut BTreeSet<PathBuf>) {
    let Ok(text) = std::fs::read_to_string(path) else {
        return;
    };
    for line in text.lines() {
        let fields: Vec<&str> = line.split("\":\"").collect();
        if fields.len() >= 2 {
            out.insert(PathBuf::from(fields[1].trim_matches('"')));
        }
    }
}

/// `ninja -t deps`: every header and source a compile edge recorded.
fn ninja_deps(dir: &Path, out: &mut BTreeSet<PathBuf>) {
    let Ok(o) = Command::new("ninja")
        .arg("-C")
        .arg(dir)
        .args(["-t", "deps"])
        .output()
    else {
        return;
    };
    if !o.status.success() {
        return;
    }
    for line in String::from_utf8_lossy(&o.stdout).lines() {
        let t = line.trim_start();
        if line.starts_with(char::is_whitespace) && !t.is_empty() {
            let p = Path::new(t);
            out.insert(if p.is_absolute() {
                p.to_path_buf()
            } else {
                dir.join(p)
            });
        }
    }
}

/// The commit of the git checkout at `dir` and whether it is dirty, or `None`
/// for a tree that is not a git checkout.
pub fn git_commit(dir: &Path) -> Option<(String, bool)> {
    let head = crate::source_stamp::nros_git_command("git")
        .arg("-C")
        .arg(dir)
        .args(["rev-parse", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())?;
    let commit = String::from_utf8_lossy(&head.stdout).trim().to_string();
    let dirty = crate::source_stamp::nros_git_command("git")
        .arg("-C")
        .arg(dir)
        .args(["status", "--porcelain", "--untracked-files=no"])
        .output()
        .ok()
        .is_some_and(|o| o.status.success() && !o.stdout.is_empty());
    Some((commit, dirty))
}

/// `[cargo.<root>]` for the cargo root at `dir`, from its `Cargo.lock`.
pub fn cargo_record(dir: &Path) -> Option<CargoRecord> {
    let text = std::fs::read_to_string(dir.join("Cargo.lock")).ok()?;
    let doc: toml::Value = toml::from_str(&text).ok()?;
    let mut lines: Vec<String> = doc
        .get("package")?
        .as_array()?
        .iter()
        .filter(|p| {
            p.get("source")
                .and_then(|s| s.as_str())
                .is_some_and(|s| s.starts_with("registry+"))
        })
        .map(|p| {
            let f = |k: &str| p.get(k).and_then(|v| v.as_str()).unwrap_or("");
            format!("{} {} {}", f("name"), f("version"), f("checksum"))
        })
        .collect();
    lines.sort();
    let crates = lines.len();
    Some(CargoRecord {
        registry: format!("sha256:{}", {
            use sha2::Digest;
            format!("{:x}", sha2::Sha256::digest(lines.join("\n").as_bytes()))
        }),
        crates,
    })
}

/// Attribute `read` to the source trees at `roots` (name -> located dir).
/// A tree is USED iff some path the build read lies under it.
pub fn sources_used<'a>(
    read: &BTreeSet<PathBuf>,
    roots: impl IntoIterator<Item = (&'a str, &'a Path)>,
) -> BTreeSet<String> {
    let mut used = BTreeSet::new();
    for (name, root) in roots {
        let root = std::fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
        if read.iter().any(|p| p.starts_with(&root)) {
            used.insert(name.to_string());
        }
    }
    used
}

/// Each Zephyr module a build in `dirs` used, by name, from the build's own
/// `zephyr_modules.txt`.
pub fn west_modules(dirs: &[PathBuf]) -> BTreeMap<String, PathBuf> {
    fn find(dir: &Path, depth: usize, out: &mut BTreeMap<String, PathBuf>) {
        if depth > 4 {
            return;
        }
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for e in entries.flatten() {
            let p = e.path();
            if e.file_type().is_ok_and(|t| t.is_dir()) {
                find(&p, depth + 1, out);
            } else if e.file_name() == "zephyr_modules.txt"
                && let Ok(text) = std::fs::read_to_string(&p)
            {
                for line in text.lines() {
                    let fields: Vec<&str> = line.split("\":\"").collect();
                    if fields.len() >= 2 {
                        out.insert(
                            fields[0].trim_matches('"').to_string(),
                            PathBuf::from(fields[1].trim_matches('"')),
                        );
                    }
                }
            }
        }
    }
    let mut out = BTreeMap::new();
    for d in dirs {
        find(d, 0, &mut out);
    }
    out
}

/// What one plan's build left behind to read, and where its record goes.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BuildRecord {
    /// The project the build belongs to; its `nros-sdk.lock` is found from
    /// here the way `nros setup` finds it (`store::lock_path_for`).
    pub project: PathBuf,
    /// Cargo roots the build resolved (each holds a `Cargo.lock`).
    pub cargo_roots: Vec<PathBuf>,
    /// Build directories whose records name what was read (cargo target
    /// dirs, cmake/west build dirs).
    pub build_dirs: Vec<PathBuf>,
}

/// Fold one finished build into the project lock, and write it only when its
/// content changed. Returns the lock path when something was recorded.
///
/// Entries are MERGED: a build records what IT read, and a project with
/// several images keeps every image's sources. An entry is only ever
/// replaced by a newer reading of the same tree.
pub fn record(rec: &BuildRecord, nano_ros_root: Option<&Path>) -> eyre::Result<Option<PathBuf>> {
    use crate::orchestration::sdk_store::SdkLock;
    let lock_path = crate::orchestration::store::lock_path_for(&rec.project);
    let mut lock = SdkLock::load(&lock_path)?;
    let before = lock.clone();

    // Keyed relative to the LOCK's directory, not the project's: the lock is
    // found by walking up to the nearest pin file, so in a checkout every leaf
    // records into the one lock at the root, and a project-relative key would
    // be "." for all of them.
    let lock_dir = lock_path.parent().unwrap_or(Path::new("."));
    for root in &rec.cargo_roots {
        if let Some(c) = cargo_record(root) {
            let canon = |p: &Path| std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
            let key = canon(root)
                .strip_prefix(canon(lock_dir))
                .map(|r| r.display().to_string())
                .ok()
                .map(|s| if s.is_empty() { ".".to_string() } else { s })
                .unwrap_or_else(|| root.display().to_string());
            lock.cargo.insert(key, c);
        }
    }

    let read = paths_read(&rec.build_dirs);
    if let Some(nr) = nano_ros_root
        && let Ok(index) =
            crate::orchestration::sdk_index::SdkIndex::load(&nr.join("nros-sdk-index.toml"))
    {
        let located: Vec<(String, PathBuf, &'static str)> = index
            .source
            .keys()
            .filter_map(|name| {
                crate::cmd::locate::locate_answer(&index, nr, name, Some(&rec.project))
                    .ok()
                    .map(|a| (name.clone(), a.path, a.rung.label()))
            })
            .collect();
        let used = sources_used(
            &read,
            located.iter().map(|(n, p, _)| (n.as_str(), p.as_path())),
        );
        for (name, path, rung) in &located {
            if !used.contains(name) {
                continue;
            }
            let (commit, dirty) = git_commit(path).unwrap_or_else(|| {
                (
                    index.source[name].git_ref.clone().unwrap_or_default(),
                    false,
                )
            });
            lock.source.insert(
                name.clone(),
                SourceRecord {
                    commit,
                    origin: (*rung).to_string(),
                    dirty,
                },
            );
        }
    }

    for (name, path) in west_modules(&rec.build_dirs) {
        if let Some((revision, _)) = git_commit(&path) {
            lock.west.insert(name, WestRecord { revision });
        }
    }

    if lock == before {
        return Ok(None);
    }
    lock.save(&lock_path)?;
    Ok(Some(lock_path))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dep_info_build_script_and_zephyr_records_are_all_read() {
        let t = tempfile::tempdir().unwrap();
        let d = t.path();
        std::fs::create_dir_all(d.join("debug/deps")).unwrap();
        std::fs::write(
            d.join("debug/deps/x.d"),
            "/out/x: /src/a.rs /src/with\\ space.rs \\\n /src/b.rs\n",
        )
        .unwrap();
        std::fs::create_dir_all(d.join("debug/build/zpico-sys-1")).unwrap();
        std::fs::write(
            d.join("debug/build/zpico-sys-1/output"),
            "cargo:rerun-if-changed=/vendor/zp/a.c\ncargo::rerun-if-changed=/vendor/zp/b.h\n\
             cargo:rustc-link-lib=static=zp\n",
        )
        .unwrap();
        std::fs::write(
            d.join("zephyr_modules.txt"),
            "\"picolibc\":\"/ws/modules/lib/picolibc\":\"/ws/modules/lib/picolibc/zephyr\"\n",
        )
        .unwrap();
        let read = paths_read(&[d.to_path_buf()]);
        for want in [
            "/src/a.rs",
            "/src/with space.rs",
            "/src/b.rs",
            "/vendor/zp/a.c",
            "/vendor/zp/b.h",
            "/ws/modules/lib/picolibc",
        ] {
            assert!(
                read.contains(Path::new(want)),
                "{want} missing from {read:?}"
            );
        }
        assert!(
            !read
                .iter()
                .any(|p| p.to_string_lossy().contains("rustc-link-lib"))
        );
        assert_eq!(
            west_modules(&[d.to_path_buf()]).get("picolibc"),
            Some(&PathBuf::from("/ws/modules/lib/picolibc"))
        );
    }

    #[test]
    fn a_source_is_used_only_when_the_build_read_under_it() {
        let read: BTreeSet<PathBuf> = ["/vendor/zp/a.c", "/src/a.rs"]
            .iter()
            .map(PathBuf::from)
            .collect();
        let used = sources_used(
            &read,
            [
                ("zenoh-pico", Path::new("/vendor/zp")),
                ("mbedtls", Path::new("/vendor/mbedtls")),
                // A sibling sharing a PREFIX is not "under" it.
                ("zp2", Path::new("/vendor/z")),
            ],
        );
        assert_eq!(used, BTreeSet::from(["zenoh-pico".to_string()]));
    }

    #[test]
    fn the_cargo_record_hashes_only_the_registry_set() {
        let t = tempfile::tempdir().unwrap();
        let reg = "registry+https://github.com/rust-lang/crates.io-index";
        let lock = |extra: &str| {
            format!(
                "version = 4\n\n[[package]]\nname = \"heapless\"\nversion = \"0.8.0\"\n\
                 source = \"{reg}\"\nchecksum = \"ab\"\n\n[[package]]\nname = \"{extra}\"\n\
                 version = \"0.0.0\"\n"
            )
        };
        std::fs::write(t.path().join("Cargo.lock"), lock("local_a")).unwrap();
        let a = cargo_record(t.path()).unwrap();
        std::fs::write(t.path().join("Cargo.lock"), lock("local_b")).unwrap();
        let b = cargo_record(t.path()).unwrap();
        assert_eq!(a, b, "a path crate is the LOCAL half and moves nothing");
        assert_eq!(a.crates, 1);
        assert!(a.registry.starts_with("sha256:"));
    }
}
