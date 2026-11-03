//! phase-530 W1 (RFC-0120 D2) -- the configure stamp.
//!
//! An unchanged `nros configure` is a no-op because it can PROVE nothing it
//! reads has moved. The stamp records two things: a digest of every input a
//! configure reads, and the environment it was configured with. [`is_current`]
//! answers "may I skip", and every `Stale` carries the one line saying why.
//!
//! Content-addressed throughout (never mtimes: a rebase touches every file).
// phase-530 W1 Task 3 (the verb) calls everything here; it removes the
// `allow(dead_code)` below.
#![allow(dead_code)]

use std::path::{Path, PathBuf};

use eyre::{Result, WrapErr};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use walkdir::WalkDir;

use crate::{
    atomic_file::atomic_write,
    cmd::entity_census::{CENSUS_DIR, Freshness, census_freshness},
    orchestration::metadata_refresh::stale_sidecar_components,
};

/// Path components a walk never enters: build output, generated trees,
/// cargo's target dir and git's own.
const SKIPPED_DIRS: &[&str] = &["build", "generated", "target", ".git"];
/// File names whose bytes are an input wherever they sit.
const INPUT_NAMES: &[&str] = &[
    "package.xml",
    "system.toml",
    "system.contract.yaml",
    "CMakeLists.txt",
    "Cargo.toml",
];
/// Interface definition extensions.
const INTERFACE_EXTS: &[&str] = &["msg", "srv", "action"];
/// Launch-file extensions, counted only under a `launch/` directory.
const LAUNCH_EXTS: &[&str] = &["xml", "py", "yaml", "yml"];
/// Environment variables whose VALUES are digest inputs.
const ENV_INPUTS: &[&str] = &["AMENT_PREFIX_PATH", "NROS_INTERFACE_SEARCH_PATH"];

/// What a configure recorded about itself.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct Stamp {
    pub(crate) digest: String,
    pub(crate) codegen_version: String,
    pub(crate) ament_prefix: Option<String>,
    pub(crate) launch_resolve: Option<PathBuf>,
}

/// Whether a recorded stamp still describes the project.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Currency {
    Current,
    /// One line: why the configure must run.
    Stale(String),
}

/// `project/build/nros/configured/<image>`; a `:` in a qualified id is `__`.
pub(crate) fn stamp_dir(project: &Path, image: &str) -> PathBuf {
    project
        .join("build")
        .join("nros")
        .join("configured")
        .join(image.replace(':', "__"))
}

/// The one lock every configure of this project takes.
pub(crate) fn lock_path(project: &Path) -> PathBuf {
    project
        .join("build")
        .join("nros")
        .join("configured")
        .join(".lock")
}

fn stamp_file(project: &Path, image: &str) -> PathBuf {
    stamp_dir(project, image).join("stamp.toml")
}

fn is_input(rel: &Path) -> bool {
    let Some(name) = rel.file_name().and_then(|n| n.to_str()) else {
        return false;
    };
    if INPUT_NAMES.contains(&name) {
        return true;
    }
    let ext = rel.extension().and_then(|e| e.to_str()).unwrap_or("");
    if INTERFACE_EXTS.contains(&ext) {
        return true;
    }
    LAUNCH_EXTS.contains(&ext)
        && rel
            .parent()
            .into_iter()
            .flat_map(Path::components)
            .any(|c| c.as_os_str() == "launch")
}

/// Sorted repo-relative paths of every input file under `project`.
fn input_files(project: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let walk = WalkDir::new(project).follow_links(false).into_iter();
    let walk = walk.filter_entry(|e| {
        if e.depth() == 0 || !e.file_type().is_dir() {
            return true;
        }
        let skipped = e
            .file_name()
            .to_str()
            .is_some_and(|n| SKIPPED_DIRS.contains(&n));
        // A nested repository (worktree or submodule) is somebody else's
        // tree: `repo_walk.py`'s rule, stop at its `.git`.
        let nested_repo = e.path().join(".git").exists();
        !skipped && !nested_repo
    });
    for entry in walk.flatten() {
        if !entry.file_type().is_file() {
            continue;
        }
        if let Ok(rel) = entry.path().strip_prefix(project)
            && is_input(rel)
        {
            out.push(rel.to_path_buf());
        }
    }
    out.sort();
    out
}

fn codegen_version() -> String {
    crate::abi_guard::EMITTED_VERSION.to_string()
}

/// `sha256:<hex>` over the project's inputs, the interface search env and the
/// codegen version.
pub(crate) fn input_digest(project: &Path) -> Result<String> {
    let mut h = Sha256::new();
    let mut feed = |tag: &str, bytes: &[u8]| {
        h.update(tag.as_bytes());
        h.update((bytes.len() as u64).to_le_bytes());
        h.update(bytes);
    };
    for rel in input_files(project) {
        let abs = project.join(&rel);
        let bytes =
            std::fs::read(&abs).wrap_err_with(|| format!("cannot read `{}`", abs.display()))?;
        feed("file", rel.to_string_lossy().as_bytes());
        feed("body", &bytes);
    }
    for var in ENV_INPUTS {
        let val = std::env::var(var).unwrap_or_default();
        feed(var, val.as_bytes());
    }
    feed("codegen", codegen_version().as_bytes());
    Ok(format!("sha256:{:x}", h.finalize()))
}

/// The recorded stamp, or `None` if there is none or it cannot be read.
pub(crate) fn read(project: &Path, image: &str) -> Option<Stamp> {
    let raw = std::fs::read_to_string(stamp_file(project, image)).ok()?;
    toml::from_str(&raw).ok()
}

/// Write the stamp atomically.
pub(crate) fn record(project: &Path, image: &str, stamp: &Stamp) -> Result<()> {
    let path = stamp_file(project, image);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)
            .wrap_err_with(|| format!("cannot create `{}`", dir.display()))?;
    }
    let body = toml::to_string_pretty(stamp).wrap_err("cannot serialise the configure stamp")?;
    atomic_write(&path, &body)
}

/// Census files recorded under the project: beside a model, or under
/// `build/nros/census`.
fn recorded_censuses(project: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for root in [
        project.join("build").join("nros").join("models"),
        project.join("build").join(CENSUS_DIR),
    ] {
        for e in WalkDir::new(&root).into_iter().flatten() {
            let name = e.file_name().to_string_lossy();
            let census = e.file_type().is_file()
                && name.ends_with(".json")
                && (name.ends_with(".census.json") || root.ends_with(CENSUS_DIR));
            if census {
                out.push(e.into_path());
            }
        }
    }
    out.sort();
    out
}

/// May a configure of `image` be skipped? Order of reasons: no stamp; digest
/// differs; a metadata sidecar's component sources moved; a recorded census is
/// stale. An image with no recorded census is not stale on that account.
pub(crate) fn is_current(project: &Path, image: &str) -> Result<Currency> {
    let Some(stamp) = read(project, image) else {
        return Ok(Currency::Stale("no configure stamp recorded".into()));
    };
    let now = input_digest(project)?;
    if stamp.digest != now {
        return Ok(Currency::Stale(format!(
            "inputs changed (recorded {}, now {now})",
            stamp.digest
        )));
    }
    if let Some(line) = stale_sidecar_components(project).into_iter().next() {
        return Ok(Currency::Stale(line));
    }
    for census in recorded_censuses(project) {
        match census_freshness(&census, project) {
            Freshness::Fresh => {}
            Freshness::Stale(why) | Freshness::Missing(why) => {
                return Ok(Currency::Stale(format!(
                    "entity census `{}` is stale: {}",
                    census.display(),
                    why.lines().next().unwrap_or("")
                )));
            }
        }
    }
    Ok(Currency::Current)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let dir = crate::test_support::scratch_dir(&format!("configure-stamp-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn stamp_for(project: &Path) -> Stamp {
        Stamp {
            digest: input_digest(project).unwrap(),
            codegen_version: codegen_version(),
            ament_prefix: None,
            launch_resolve: None,
        }
    }

    #[test]
    fn stamp_dir_for_leaf_and_workspace() {
        let p = Path::new("p");
        assert_eq!(
            stamp_dir(p, "zephyr_zenoh"),
            p.join("build/nros/configured/zephyr_zenoh")
        );
        assert_eq!(
            stamp_dir(p, "demo_bringup:zephyr"),
            p.join("build/nros/configured/demo_bringup__zephyr")
        );
    }

    #[test]
    fn digest_moves_on_system_toml_edit() {
        let p = tmp("digest-moves");
        std::fs::write(p.join("system.toml"), "[system]\nname = \"a\"\n").unwrap();
        let before = input_digest(&p).unwrap();
        assert!(before.starts_with("sha256:"));
        std::fs::write(p.join("system.toml"), "[system]\nname = \"b\"\n").unwrap();
        assert_ne!(before, input_digest(&p).unwrap());
    }

    #[test]
    fn digest_sees_launch_files_and_ignores_other_xml() {
        let p = tmp("digest-launch");
        std::fs::create_dir_all(p.join("launch")).unwrap();
        std::fs::write(p.join("launch/a.launch.xml"), "<launch/>").unwrap();
        std::fs::write(p.join("notes.xml"), "<x/>").unwrap();
        let before = input_digest(&p).unwrap();
        std::fs::write(p.join("notes.xml"), "<y/>").unwrap();
        assert_eq!(before, input_digest(&p).unwrap());
        std::fs::write(p.join("launch/a.launch.xml"), "<launch a='1'/>").unwrap();
        assert_ne!(before, input_digest(&p).unwrap());
    }

    #[test]
    fn digest_ignores_build_and_generated() {
        let p = tmp("digest-ignores");
        std::fs::write(p.join("system.toml"), "[system]\n").unwrap();
        let before = input_digest(&p).unwrap();
        for dir in ["build", "generated", "target", "nested/generated"] {
            std::fs::create_dir_all(p.join(dir)).unwrap();
            std::fs::write(p.join(dir).join("package.xml"), "<package/>").unwrap();
            std::fs::write(p.join(dir).join("a.msg"), "int32 x\n").unwrap();
        }
        // A nested repository is not this project's.
        std::fs::create_dir_all(p.join("sub/.git")).unwrap();
        std::fs::write(p.join("sub/Cargo.toml"), "[package]\n").unwrap();
        assert_eq!(before, input_digest(&p).unwrap());
    }

    #[test]
    fn no_stamp_is_stale() {
        let p = tmp("no-stamp");
        assert!(matches!(
            is_current(&p, "img").unwrap(),
            Currency::Stale(why) if why.contains("no configure stamp")
        ));
    }

    #[test]
    fn recorded_stamp_is_current() {
        let p = tmp("recorded");
        std::fs::write(p.join("system.toml"), "[system]\n").unwrap();
        let stamp = stamp_for(&p);
        record(&p, "ws:zephyr", &stamp).unwrap();
        assert_eq!(read(&p, "ws:zephyr"), Some(stamp));
        assert_eq!(is_current(&p, "ws:zephyr").unwrap(), Currency::Current);
        std::fs::write(p.join("system.toml"), "[system]\nname = \"x\"\n").unwrap();
        assert!(matches!(
            is_current(&p, "ws:zephyr").unwrap(),
            Currency::Stale(why) if why.contains("inputs changed")
        ));
    }

    #[test]
    fn stale_when_metadata_sidecar_inputs_moved() {
        let p = tmp("sidecar");
        std::fs::create_dir_all(p.join("src")).unwrap();
        std::fs::create_dir_all(p.join("metadata")).unwrap();
        std::fs::write(
            p.join("package.xml"),
            "<package format=\"3\"><name>talker_pkg</name><version>0.1.0</version>\
             <description>t</description><maintainer email=\"x@x\">x</maintainer>\
             <license>Apache-2.0</license></package>",
        )
        .unwrap();
        std::fs::write(
            p.join("nros.toml"),
            "[component]\nversion = 1\npackage = \"talker_pkg\"\ncomponent = \"talker\"\n\
             language = \"rust\"\n[component.metadata]\nsource_metadata = \"metadata/talker.json\"\n",
        )
        .unwrap();
        std::fs::write(p.join("src/lib.rs"), "// the component\n").unwrap();
        let sidecar = p.join("metadata/talker.json");
        std::fs::write(
            &sidecar,
            r#"{"version": 4, "package": "talker_pkg", "component": "talker",
                "language": "rust", "executable": "talker", "exported_symbol": null,
                "nodes": [], "callbacks": [], "parameters": [],
                "trace": {"generator": "t", "package_manifest": "package.xml",
                          "source_artifacts": []}}"#,
        )
        .unwrap();
        let digest = crate::orchestration::metadata_refresh::source_digest(&p).unwrap();
        crate::orchestration::metadata_refresh::stamp_provenance(
            &sidecar,
            &format!("{digest}:closure:stamp"),
        )
        .unwrap();

        // The sidecar lives in the project; record the stamp over it.
        record(&p, "img", &stamp_for(&p)).unwrap();
        assert_eq!(is_current(&p, "img").unwrap(), Currency::Current);

        // A component source moves; no digest input (`lib.rs`) changed, so
        // the sidecar check is what answers.
        std::fs::write(p.join("src/lib.rs"), "// the component, edited\n").unwrap();
        match is_current(&p, "img").unwrap() {
            Currency::Stale(why) => assert!(why.contains("metadata sidecar"), "{why}"),
            Currency::Current => panic!("a moved component source must stale the stamp"),
        }
    }
}
