//! phase-559 W10 — the checks that make a SHARED cargo `build.build-dir` safe.
//!
//! The metadata probe builds every C/C++ workspace's generated image root
//! (`nros-{c,cpp}-image`) with its own Corrosion target dir but ONE cargo
//! build-dir per shared-cargo key (`nros_image_share_build_dir` in
//! `cmake/NanoRosImageCrate.cmake`). Two ways that can go wrong, both measured
//! on cargo 1.99 before this was written, and each with its own check:
//!
//! 1. **Two units of one crate, differing only in path spelling** (issue 0616).
//!    A member is spelled relative to its root and an external path dep
//!    absolutely, so a crate reached both ways becomes two units in one
//!    `deps/`, and a transitive `-L dependency=` lookup may bind either. The
//!    [`census`] groups the build-dir's fingerprints by everything but the path
//!    and refuses a group holding two paths.
//! 2. **Two ROOTS sharing one unit.** Every generated root is named the same,
//!    and cargo spells a root relative to itself, so without a distinct
//!    identity two build dirs give their roots ONE unit hash: the second build
//!    found the unit fresh and uplifted the first build's archive, no error.
//!    The generator now gives each root a version suffix and a marker string
//!    from its build dir; [`markers`] reads every uplifted archive and refuses
//!    one that does not carry exactly its own marker. A file fingerprint cannot
//!    catch this: its dep-info spells the source relative to the package too.
//!
//! Both run after every probe build. A build dir that does not share (no
//! `nros-image-roots/` record) has nothing to check.

use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

use eyre::{Result, WrapErr, bail};

/// The marker prefix the generated root embeds, followed by 16 hex digits.
pub const MARKER_PREFIX: &[u8] = b"nros-image-root:";
const ID_LEN: usize = 16;

/// Run both checks for one probe build dir.
pub fn check(build_dir: &Path) -> Result<()> {
    let roots = build_dir.join("nros-image-roots");
    if !roots.is_dir() {
        return Ok(());
    }
    let bdir = build_dir.join("cargo").join(".build-dir");
    let mut problems = Vec::new();
    if bdir.is_dir() {
        problems.extend(census(&bdir)?);
    }
    problems.extend(markers(&roots)?);
    if problems.is_empty() {
        return Ok(());
    }
    bail!(
        "the shared cargo build-dir is not safe to use for this build:\n  {}\n\
         Remove {} and rebuild. Each probe then gets its own intermediates again \
         until the cause is fixed.",
        problems.join("\n  "),
        bdir.display()
    )
}

/// Every fingerprint group (same unit in every respect but its path) that holds
/// more than one path, as one line each.
pub fn census(build_dir: &Path) -> Result<Vec<String>> {
    // (fingerprint dir, package, unit file, the fingerprint minus path/local) -> paths
    let mut groups: BTreeMap<(PathBuf, String, String, String), BTreeSet<String>> = BTreeMap::new();
    for fp_root in fingerprint_dirs(build_dir) {
        let Ok(units) = std::fs::read_dir(&fp_root) else {
            continue;
        };
        for unit in units.flatten() {
            let unit_dir = unit.path();
            let Some(dir_name) = unit_dir.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            let Some((pkg, hash)) = dir_name.rsplit_once('-') else {
                continue;
            };
            let Ok(files) = std::fs::read_dir(&unit_dir) else {
                continue;
            };
            for f in files.flatten() {
                let p = f.path();
                if p.extension().and_then(|e| e.to_str()) != Some("json") {
                    continue;
                }
                let text = std::fs::read_to_string(&p)
                    .wrap_err_with(|| format!("reading {}", p.display()))?;
                let Ok(serde_json::Value::Object(mut obj)) = serde_json::from_str(&text) else {
                    continue;
                };
                let path = obj
                    .remove("path")
                    .map(|v| v.to_string())
                    .unwrap_or_default();
                obj.remove("local");
                let file = p.file_name().unwrap().to_string_lossy().into_owned();
                // The fingerprint JSON carries no VERSION, so two versions of
                // one registry crate (thiserror-impl 1.x and 2.x, measured in
                // the first in-place probe run) differ only in `path` too. The
                // source identity separates them; see `source_identity`.
                let identity = source_identity(&fp_root, &file, hash);
                let rest = format!("{identity}|{}", serde_json::Value::Object(obj));
                groups
                    .entry((fp_root.clone(), pkg.to_owned(), file, rest))
                    .or_default()
                    .insert(path);
            }
        }
    }
    Ok(groups
        .into_iter()
        .filter(|(_, paths)| paths.len() > 1)
        .map(|((dir, pkg, file, _), paths)| {
            format!(
                "crate `{pkg}` ({file}) is built {} times in {} with only its path \
                 spelling differing — one crate reached by two spellings, the shape that \
                 links a duplicate symbol",
                paths.len(),
                dir.display()
            )
        })
        .collect())
}

/// Where a unit's source came from, as far as it decides "one crate or two".
///
/// Read from `deps/<target>-<hash>.d`, whose paths are absolute. A crate from
/// a registry or a git checkout is identified by its versioned package dir
/// (`registry:thiserror-impl-1.0.69`), so two versions are two crates. Any
/// other crate is `local`: 0616's shape is ONE local crate reached by a
/// relative and an absolute spelling, and both spellings must land in the
/// same group to be caught. A unit with no `.d` (a build script's run, a
/// missing file) is `local` too, which errs toward refusing.
fn source_identity(fp_root: &Path, fingerprint_file: &str, hash: &str) -> String {
    const LOCAL: &str = "local";
    // `lib-<target>.json` → `<target>`; other unit kinds have no `.d` here.
    let Some(target) = fingerprint_file
        .strip_suffix(".json")
        .and_then(|f| f.split_once('-'))
        .map(|(_, t)| t)
    else {
        return LOCAL.into();
    };
    let Some(profile_dir) = fp_root.parent() else {
        return LOCAL.into();
    };
    let d = profile_dir.join("deps").join(format!("{target}-{hash}.d"));
    let Ok(text) = std::fs::read_to_string(&d) else {
        return LOCAL.into();
    };
    let Some(first) = text
        .lines()
        .next()
        .and_then(|l| l.split_once(": "))
        .and_then(|(_, srcs)| srcs.split_whitespace().next())
    else {
        return LOCAL.into();
    };
    for (marker, kind, parts) in [
        ("/registry/src/", "registry", 2),
        ("/git/checkouts/", "git", 3),
    ] {
        if let Some((_, after)) = first.split_once(marker) {
            // registry: <index>/<name>-<ver>/…   git: <repo>/<rev>/…
            let id: Vec<&str> = after.split('/').take(parts).collect();
            if id.len() == parts {
                return format!("{kind}:{}", id[1..].join("/"));
            }
        }
    }
    LOCAL.into()
}

/// `.fingerprint` dirs under a build-dir: `<profile>/.fingerprint` (host) and
/// `<triple>/<profile>/.fingerprint` (target).
fn fingerprint_dirs(build_dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for e in walkdir::WalkDir::new(build_dir)
        .min_depth(2)
        .max_depth(3)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        if e.file_type().is_dir() && e.file_name() == ".fingerprint" {
            out.push(e.into_path());
        }
    }
    out.sort();
    out
}

/// Every recorded root whose uplifted archive does not carry exactly its own
/// marker, as one line each. A root whose archive was not built is skipped: an
/// archive nobody built ships nothing.
pub fn markers(roots_dir: &Path) -> Result<Vec<String>> {
    let mut problems = Vec::new();
    let mut entries: Vec<PathBuf> = std::fs::read_dir(roots_dir)
        .wrap_err_with(|| format!("reading {}", roots_dir.display()))?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("txt"))
        .collect();
    entries.sort();
    for rec in entries {
        let id = rec
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or_default()
            .to_owned();
        let archive =
            std::fs::read_to_string(&rec).wrap_err_with(|| format!("reading {}", rec.display()))?;
        let archive = PathBuf::from(archive.trim());
        let Ok(bytes) = std::fs::read(&archive) else {
            continue;
        };
        let found = markers_in(&bytes);
        let want: BTreeSet<String> = [id.clone()].into();
        if found != want {
            let found: Vec<&str> = found.iter().map(String::as_str).collect();
            problems.push(format!(
                "{} should carry image root `{id}` and carries [{}] — another build's \
                 root unit was uplifted in its place",
                archive.display(),
                found.join(", ")
            ));
        }
    }
    Ok(problems)
}

/// The distinct root ids embedded in an archive.
pub fn markers_in(bytes: &[u8]) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let mut i = 0;
    while let Some(off) = find(&bytes[i..], MARKER_PREFIX) {
        let start = i + off + MARKER_PREFIX.len();
        if let Some(id) = bytes.get(start..start + ID_LEN) {
            if id.iter().all(u8::is_ascii_hexdigit) {
                out.insert(String::from_utf8_lossy(id).into_owned());
            }
        }
        i = start;
    }
    out
}

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    let first = needle[0];
    let mut i = 0;
    while i + needle.len() <= hay.len() {
        let rel = hay[i..].iter().position(|&b| b == first)?;
        i += rel;
        if hay.get(i..i + needle.len()) == Some(needle) {
            return Some(i);
        }
        i += 1;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fingerprint(root: &Path, unit: &str, file: &str, path: u64, features: &str) {
        let d = root.join("release/.fingerprint").join(unit);
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(
            d.join(file),
            format!(
                r#"{{"rustc":1,"features":"{features}","target":7,"profile":2,"path":{path},"deps":[],"local":[{{"Precalculated":"x"}}],"rustflags":[],"config":0,"compile_kind":0}}"#
            ),
        )
        .unwrap();
    }

    #[test]
    fn census_refuses_one_crate_reached_by_two_spellings() {
        // The measured issue-0616 shape: `dep` as a member of one root and an
        // external path dep of another, one build-dir.
        let t = tempfile::tempdir().unwrap();
        fingerprint(t.path(), "dep-1111", "lib-dep.json", 1700, "[]");
        fingerprint(t.path(), "dep-2222", "lib-dep.json", 1722, "[]");
        let p = census(t.path()).unwrap();
        assert_eq!(p.len(), 1, "{p:?}");
        assert!(p[0].contains("crate `dep`"), "{p:?}");
    }

    #[test]
    fn census_accepts_two_roots_with_one_path_and_a_crate_built_with_two_feature_sets() {
        // Distinct image roots share the relative `src/lib.rs` path field and
        // are separate units; a dep built with two feature sets is two units
        // that differ in more than their path. Neither is the 0616 shape.
        let t = tempfile::tempdir().unwrap();
        fingerprint(
            t.path(),
            "nros-cpp-image-aaaa",
            "lib-nros_cpp.json",
            99,
            "[]",
        );
        fingerprint(
            t.path(),
            "nros-cpp-image-bbbb",
            "lib-nros_cpp.json",
            99,
            "[]",
        );
        fingerprint(t.path(), "dep-1111", "lib-dep.json", 5, "[\"a\"]");
        fingerprint(t.path(), "dep-2222", "lib-dep.json", 5, "[\"b\"]");
        assert_eq!(census(t.path()).unwrap(), Vec::<String>::new());
    }

    #[test]
    fn census_accepts_two_versions_of_one_registry_crate() {
        // Measured on the first in-place probe run: thiserror-impl 1.0.69 and
        // 2.0.x share every fingerprint field but `path`, because the JSON
        // carries no version. Their `.d` files name versioned registry dirs.
        let t = tempfile::tempdir().unwrap();
        fingerprint(
            t.path(),
            "thiserror-impl-1111",
            "lib-thiserror_impl.json",
            7,
            "[]",
        );
        fingerprint(
            t.path(),
            "thiserror-impl-2222",
            "lib-thiserror_impl.json",
            8,
            "[]",
        );
        let deps = t.path().join("release/deps");
        std::fs::create_dir_all(&deps).unwrap();
        for (h, v) in [("1111", "1.0.69"), ("2222", "2.0.18")] {
            std::fs::write(
                deps.join(format!("thiserror_impl-{h}.d")),
                format!(
                    "{}/thiserror_impl-{h}.d: /h/.cargo/registry/src/index.crates.io-x/thiserror-impl-{v}/src/lib.rs\n",
                    deps.display()
                ),
            )
            .unwrap();
        }
        assert_eq!(census(t.path()).unwrap(), Vec::<String>::new());
        // …and ONE version reached twice is still refused.
        std::fs::write(
            deps.join("thiserror_impl-2222.d"),
            format!(
                "{}/thiserror_impl-2222.d: /h/.cargo/registry/src/index.crates.io-x/thiserror-impl-1.0.69/src/lib.rs\n",
                deps.display()
            ),
        )
        .unwrap();
        assert_eq!(census(t.path()).unwrap().len(), 1);
    }

    fn record(roots: &Path, id: &str, archive: &Path) {
        std::fs::create_dir_all(roots).unwrap();
        std::fs::write(
            roots.join(format!("{id}.txt")),
            format!("{}\n", archive.display()),
        )
        .unwrap();
    }

    #[test]
    fn markers_refuse_an_archive_carrying_another_roots_code() {
        // The measured collision: root a's target dir holds b's archive.
        let t = tempfile::tempdir().unwrap();
        let a = t.path().join("a.a");
        std::fs::write(&a, b"junk nros-image-root:bbbbbbbbbbbbbbbb junk").unwrap();
        record(&t.path().join("roots"), "aaaaaaaaaaaaaaaa", &a);
        let p = markers(&t.path().join("roots")).unwrap();
        assert_eq!(p.len(), 1, "{p:?}");
        assert!(p[0].contains("bbbbbbbbbbbbbbbb"), "{p:?}");
    }

    #[test]
    fn markers_refuse_an_archive_with_no_marker_and_skip_one_not_built() {
        let t = tempfile::tempdir().unwrap();
        let roots = t.path().join("roots");
        let bare = t.path().join("bare.a");
        std::fs::write(&bare, b"no marker here").unwrap();
        record(&roots, "aaaaaaaaaaaaaaaa", &bare);
        record(&roots, "cccccccccccccccc", &t.path().join("never-built.a"));
        let p = markers(&roots).unwrap();
        assert_eq!(p.len(), 1, "{p:?}");
    }

    #[test]
    fn markers_accept_an_archive_with_exactly_its_own_marker() {
        let t = tempfile::tempdir().unwrap();
        let a = t.path().join("a.a");
        // Twice is still exactly one id: an archive holds the object once per
        // codegen unit that keeps the static.
        std::fs::write(
            &a,
            b"x nros-image-root:aaaaaaaaaaaaaaaa y nros-image-root:aaaaaaaaaaaaaaaa",
        )
        .unwrap();
        record(&t.path().join("roots"), "aaaaaaaaaaaaaaaa", &a);
        assert_eq!(
            markers(&t.path().join("roots")).unwrap(),
            Vec::<String>::new()
        );
    }

    #[test]
    fn check_is_silent_on_a_build_dir_that_does_not_share() {
        let t = tempfile::tempdir().unwrap();
        check(t.path()).unwrap();
    }
}
