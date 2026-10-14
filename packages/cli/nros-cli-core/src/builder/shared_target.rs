//! RFC-0100 D13 (issue 1869) — several images, one caller-named cargo target dir.
//!
//! `nros build` gives every image its own target dir (`<entry>/target`, in
//! `nros-cargo.toml`). A caller may override that with `--target-dir` after
//! `--` — the fixture lanes do, so the rows of one workspace share one tree
//! (phase-340, RFC-0070 R4). Every image's settings then differ in exactly the
//! facts that are per image, and two of those reach a build script:
//!
//! * `NROS_SIZING_DESCRIPTOR` names a different FILE per image. Cargo compares
//!   the variable as TEXT (`rerun-if-env-changed`, which issue 1623 needs for the
//!   unset→set edge), so a different path re-runs every consumer — issue 0491's
//!   shape — even where the two files state the same facts.
//! * Where the facts really differ, one unit identity carries two
//!   configurations — issue 0616's shape. Removing the text edge would not fix
//!   that: it would swap the rebuild for a WRONG-CONFIG reuse.
//!
//! So the configuration is made part of where the build lives, the content-keyed
//! rule `nros_shared_cargo_dir` applies on the cmake road (RFC-0094 D4):
//!
//! ```text
//!   <target-dir>/nros-class/<key>/sizing.toml   the descriptor, as a class copy
//!   <target-dir>/nros-class/<key>/build/        cargo's `build.build-dir`
//!   <target-dir>/<profile>/<entry>              final artifacts, where they were
//! ```
//!
//! `<key>` hashes every `[env]` value this image's settings carry, with the
//! descriptor contributing its CONTENT (as a class copy, [`as_build_class`])
//! rather than its path. Images with equal configurations get one key, one
//! descriptor path (so the text edge never fires between them) and one build
//! dir; images that differ get separate build dirs and never touch each other's
//! units. Cargo's `build-dir` keeps only intermediates there — the binaries are
//! still uplifted into `<target-dir>/<profile>/`, which is where every test-side
//! locator reads them (RFC-0070 R3 is untouched).
//!
//! The user's road names no `--target-dir`, so none of this runs there.
//!
//! [`as_build_class`]: nros_sizing_descriptor::as_build_class

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use sha2::{Digest, Sha256};

/// The subdirectory of a shared target dir that holds the classes.
pub const CLASS_SUBDIR: &str = "nros-class";
/// The class copy of the descriptor, inside a class dir.
pub const CLASS_DESCRIPTOR: &str = "sizing.toml";
/// Cargo's `build.build-dir`, inside a class dir.
pub const CLASS_BUILD_DIR: &str = "build";

/// The target dir a caller named, from the cargo arguments it passed after
/// `--` or else cargo's own environment (`CARGO_TARGET_DIR`,
/// `CARGO_BUILD_TARGET_DIR`). Relative to `cwd`, cargo's working directory.
pub fn caller_target_dir(
    native_args: &[String],
    env: &dyn Fn(&str) -> Option<String>,
    cwd: &Path,
) -> Option<PathBuf> {
    let mut named: Option<String> = None;
    let mut it = native_args.iter();
    while let Some(a) = it.next() {
        if a == "--" {
            break;
        }
        if a == "--target-dir" {
            named = it.next().cloned();
        } else if let Some(v) = a.strip_prefix("--target-dir=") {
            named = Some(v.to_string());
        }
    }
    let named = named
        .or_else(|| env("CARGO_TARGET_DIR"))
        .or_else(|| env("CARGO_BUILD_TARGET_DIR"))
        .filter(|s| !s.trim().is_empty())?;
    Some(cwd.join(named))
}

/// Did the caller choose cargo's build dir itself? Then it is theirs.
pub fn caller_names_build_dir(
    native_args: &[String],
    env: &dyn Fn(&str) -> Option<String>,
) -> bool {
    native_args
        .iter()
        .take_while(|a| a.as_str() != "--")
        .any(|a| a.contains("build.build-dir") || a.contains("build-dir="))
        || env("CARGO_BUILD_BUILD_DIR").is_some_and(|v| !v.trim().is_empty())
}

/// One class: where its descriptor copy and its build dir live.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Class {
    pub key: String,
    pub dir: PathBuf,
}

impl Class {
    pub fn build_dir(&self) -> PathBuf {
        self.dir.join(CLASS_BUILD_DIR)
    }
}

/// Key this image's configuration, write its class copy of the descriptor
/// (write-if-changed, so an unchanged class keeps its mtime and its
/// `rerun-if-changed` edge stays quiet), and re-point `path_env`'s
/// `NROS_SIZING_DESCRIPTOR` row at it.
///
/// `extra` is every other input of the settings file that is not an `[env]`
/// row but reaches the build (the board's `cargo_config` template, the target
/// triple); it is keyed verbatim.
pub fn apply(
    target_dir: &Path,
    env: &BTreeMap<String, String>,
    path_env: &mut BTreeMap<String, PathBuf>,
    extra: &[(&str, &str)],
) -> Result<Class, String> {
    let desc_key = nros_sizing_descriptor::DESCRIPTOR_ENV;
    // The class copy is rendered with a FIXED entry first, so its text — and so
    // the key — depends on the facts alone; the copy on disk then names the key.
    let class_desc = match path_env.get(desc_key) {
        Some(p) => match nros_sizing_descriptor::read(p) {
            Ok(d) => Some(d),
            // A named file that is not there yet: no class copy. The path is
            // keyed as text below, which is exactly as conservative as cargo.
            Err(e) if e.is_missing() => None,
            Err(e) => return Err(e.to_string()),
        },
        None => None,
    };
    let mut h = Sha256::new();
    let mut feed = |k: &str, v: &str| {
        h.update(k.as_bytes());
        h.update([0u8]);
        h.update(v.as_bytes());
        h.update([0u8]);
    };
    for (k, v) in env {
        feed(&format!("env:{k}"), v);
    }
    for (k, p) in path_env.iter() {
        if k == desc_key && class_desc.is_some() {
            continue;
        }
        feed(&format!("path:{k}"), &p.display().to_string());
    }
    for (k, v) in extra {
        feed(&format!("extra:{k}"), v);
    }
    let placeholder_text = match &class_desc {
        Some(d) => Some(nros_sizing_descriptor::as_build_class(d, "class")?.1),
        None => None,
    };
    if let Some(t) = &placeholder_text {
        feed("descriptor", t);
    }
    let key: String = format!("{:x}", h.finalize())[..16].to_string();
    let dir = target_dir.join(CLASS_SUBDIR).join(&key);
    if let Some(d) = &class_desc {
        let (_, text) = nros_sizing_descriptor::as_build_class(d, &format!("class-{key}"))?;
        let copy = dir.join(CLASS_DESCRIPTOR);
        write_if_changed(&copy, &text)?;
        path_env.insert(desc_key.to_string(), copy);
    }
    Ok(Class { key, dir })
}

fn write_if_changed(path: &Path, text: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
    }
    // Atomic AND write-if-changed: a concurrent reader never sees half a
    // descriptor, and an unchanged class keeps its mtime.
    crate::atomic_file::atomic_write(path, text).map_err(|e| format!("{e:#}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use nros_sizing_descriptor::{Basis, SizingDescriptor, Status};

    fn write_desc(dir: &Path, entry: &str, nodes: usize) -> PathBuf {
        let mut d = SizingDescriptor::new(entry, Status::Refused, Basis::Closure);
        d.image.set_model_nodes(Some(nodes));
        let p = nros_sizing_descriptor::descriptor_path(&dir.join(entry), entry);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, nros_sizing_descriptor::render(&d)).unwrap();
        p
    }

    fn class_for(tdir: &Path, desc: &Path) -> (Class, PathBuf) {
        let mut path_env = BTreeMap::new();
        path_env.insert(
            nros_sizing_descriptor::DESCRIPTOR_ENV.to_string(),
            desc.to_path_buf(),
        );
        let c = apply(tdir, &BTreeMap::new(), &mut path_env, &[]).unwrap();
        (c, path_env[nros_sizing_descriptor::DESCRIPTOR_ENV].clone())
    }

    /// The issue's own pair: `linux` (2 nodes) and `linux_service_server`
    /// (1 node) must NOT share; two entries with equal facts must, down to the
    /// byte of the path cargo compares.
    #[test]
    fn equal_facts_share_a_class_and_different_facts_do_not() {
        let tmp = tempfile::tempdir().unwrap();
        let tdir = tmp.path().join("target-fixtures");
        let a = write_desc(tmp.path(), "linux", 2);
        let b = write_desc(tmp.path(), "linux_service_inprocess", 2);
        let c = write_desc(tmp.path(), "linux_service_server", 1);
        let (ca, pa) = class_for(&tdir, &a);
        let (cb, pb) = class_for(&tdir, &b);
        let (cc, pc) = class_for(&tdir, &c);
        assert_eq!(ca, cb);
        assert_eq!(
            pa, pb,
            "one path, so the text edge never fires between them"
        );
        assert_ne!(ca.key, cc.key);
        assert_ne!(pa, pc);
        // Each copy states its OWN facts.
        let got = nros_sizing_descriptor::read(&pc).unwrap();
        assert_eq!(got.image, nros_sizing_descriptor::read(&c).unwrap().image);
        assert_eq!(got.meta.entry, format!("class-{}", cc.key));
        assert_eq!(
            ca.build_dir(),
            tdir.join(CLASS_SUBDIR).join(&ca.key).join("build")
        );
    }

    /// An unchanged class is not rewritten: its mtime is the descriptor's
    /// `rerun-if-changed` edge.
    #[test]
    fn an_unchanged_class_keeps_its_mtime() {
        let tmp = tempfile::tempdir().unwrap();
        let tdir = tmp.path().join("t");
        let a = write_desc(tmp.path(), "linux", 2);
        let (_, p) = class_for(&tdir, &a);
        let before = std::fs::metadata(&p).unwrap().modified().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(20));
        let b = write_desc(tmp.path(), "other", 2);
        let (_, p2) = class_for(&tdir, &b);
        assert_eq!(p, p2);
        assert_eq!(std::fs::metadata(&p).unwrap().modified().unwrap(), before);
    }

    /// Any other `[env]` value is part of the class too.
    #[test]
    fn an_env_row_is_part_of_the_key() {
        let tmp = tempfile::tempdir().unwrap();
        let mut e1 = BTreeMap::new();
        e1.insert("NROS_X".to_string(), "1".to_string());
        let mut e2 = e1.clone();
        e2.insert("NROS_X".to_string(), "2".to_string());
        let k1 = apply(tmp.path(), &e1, &mut BTreeMap::new(), &[]).unwrap();
        let k2 = apply(tmp.path(), &e2, &mut BTreeMap::new(), &[]).unwrap();
        assert_ne!(k1.key, k2.key);
    }

    #[test]
    fn the_caller_target_dir_is_read_from_the_args_then_the_env() {
        let cwd = Path::new("/ws");
        let none = |_: &str| None;
        let a = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(
            caller_target_dir(&a(&["--profile", "x", "--target-dir", "tf"]), &none, cwd),
            Some(PathBuf::from("/ws/tf"))
        );
        assert_eq!(
            caller_target_dir(&a(&["--target-dir=/abs"]), &none, cwd),
            Some(PathBuf::from("/abs"))
        );
        assert_eq!(caller_target_dir(&a(&["-j4"]), &none, cwd), None);
        // After cargo's own `--`, an argument belongs to the program.
        assert_eq!(
            caller_target_dir(&a(&["--", "--target-dir", "x"]), &none, cwd),
            None
        );
        let env = |k: &str| (k == "CARGO_TARGET_DIR").then(|| "e".to_string());
        assert_eq!(
            caller_target_dir(&[], &env, cwd),
            Some(PathBuf::from("/ws/e"))
        );
        assert!(caller_names_build_dir(
            &a(&["--config", "build.build-dir=\"x\""]),
            &none
        ));
        assert!(!caller_names_build_dir(&a(&["--target-dir", "x"]), &none));
    }
}
