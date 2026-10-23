//! What a generated message package was generated FROM — RFC-0104 D5, issue 1780.
//!
//! Generated message code is the LOCAL half of an image (RFC-0104 D1): it is a
//! function of the host's ament install, and that install moves underneath it
//! (`apt install --only-upgrade 'ros-humble-*'`, a sourced overlay). Before this
//! module nothing recorded which inputs a generated tree came from, so nothing
//! could tell it was stale: issue 1780 measured `nros build` compiling a
//! byte-identical binary after `std_msgs/String` gained a field, and only an
//! explicit `nros sync` noticed.
//!
//! mtimes cannot answer the question. `dpkg` installs a file with the mtime it
//! has INSIDE the `.deb` — the package's build time, routinely older than a
//! generated tree written last week — and an overlay that starts to shadow a
//! package changes which file is read without making any file newer. So the
//! record is a CONTENT hash, and it is compared, never ordered.
//!
//! One spelling, three roads:
//!
//! * `nros sync` (the cargo road's generator) writes a [`Stamp`] into each
//!   generated crate it emits, and `nros build` recomputes and regenerates the
//!   packages whose stamp no longer matches;
//! * `nros codegen --args-file` (the cmake and west roads' generator) writes the
//!   same stamp beside the C/C++ output, and `nros codegen --ament-witness`
//!   turns the CURRENT fingerprint into a write-if-changed file both cmake
//!   generators hang their configure-time freshness predicate on.
//!
//! The section shape is the one RFC-0104 D4 gives the project lock
//! (`[generated.<pkg>] ament = …, codegen = …, emitter = …`), so W3 can copy it
//! there.
//!
//! **The EMITTER is an input too** (issue 1825). The codegen VERSION is an
//! integer bumped when the emitted code's contract with the runtime changes, so
//! it catches an incompatible emitter and not one that merely moved. Two release
//! toolchains can carry the same version and emit different bytes, and a user
//! project switches between them with nothing but a pin edit (RFC-0095 D9):
//! keyed on `{ament, codegen}` alone, `nros build` under the new pin reused the
//! old pin's output as current. `emitter` is [`crate::codegen_fingerprint`],
//! the hash of every byte this build of the emitters produces for the corpus,
//! so any emitted-byte change regenerates and nothing else does.

use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    io,
    path::{Path, PathBuf},
};

/// The record's file name, inside the generated package's own directory.
pub const STAMP_FILE: &str = "nros-generated.toml";

/// Bumped when the hashing rule below changes, so a record written under the
/// old rule reads as moved rather than as a false match.
const SCHEME: &str = "nros-ament-fingerprint/1";

/// The subdirectories of a package that hold interface definitions.
const INTERFACE_DIRS: [&str; 3] = ["msg", "srv", "action"];

/// The inputs one package contributes: `package.xml` and every file under
/// `msg/`, `srv/`, `action/`, keyed by their path RELATIVE to the package root.
///
/// Relative, so a byte-identical copy of a package in another prefix
/// fingerprints the same: what is compared is what codegen reads, not where it
/// happened to be installed.
fn package_inputs(share_dir: &Path) -> io::Result<Vec<(String, PathBuf)>> {
    let mut out = Vec::new();
    let pxml = share_dir.join("package.xml");
    if pxml.is_file() {
        out.push(("package.xml".to_string(), pxml));
    }
    for sub in INTERFACE_DIRS {
        let dir = share_dir.join(sub);
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for e in entries {
            let e = e?;
            let p = e.path();
            if p.is_file() {
                out.push((format!("{sub}/{}", e.file_name().to_string_lossy()), p));
            }
        }
    }
    out.sort();
    Ok(out)
}

/// The package names a `package.xml` depends on, any dependency kind, with XML
/// comments removed first (a commented-out `<depend>` is not a dependency —
/// phase-348 W1's rule, same as the cmake reader).
pub fn package_xml_deps(body: &str) -> Vec<String> {
    let mut text = String::with_capacity(body.len());
    let mut rest = body;
    while let Some(start) = rest.find("<!--") {
        text.push_str(&rest[..start]);
        match rest[start..].find("-->") {
            Some(end) => rest = &rest[start + end + 3..],
            None => {
                rest = "";
                break;
            }
        }
    }
    text.push_str(rest);

    let mut deps = BTreeSet::new();
    for tag in [
        "depend",
        "build_depend",
        "exec_depend",
        "build_export_depend",
        "run_depend",
    ] {
        let open = format!("<{tag}");
        let close = format!("</{tag}>");
        let mut cur = text.as_str();
        while let Some(i) = cur.find(&open) {
            let after = &cur[i + open.len()..];
            // `<depend>` or `<depend condition="…">`, never `<dependx>`.
            if !after.starts_with('>') && !after.starts_with(char::is_whitespace) {
                cur = after;
                continue;
            }
            let Some(gt) = after.find('>') else { break };
            let body = &after[gt + 1..];
            let Some(end) = body.find(&close) else { break };
            let name = body[..end].trim();
            if !name.is_empty() {
                deps.insert(name.to_string());
            }
            cur = &body[end..];
        }
    }
    deps.into_iter().collect()
}

/// The ament fingerprint of `name` at `share_dir`: a hash over its own inputs
/// AND those of every interface package it reaches through `package.xml`
/// dependencies that `lookup` resolves.
///
/// The closure, not the package alone, because a dependent's generated code is
/// a function of its dependencies too — a nested type's fields and serialized
/// size bound are baked into `std_msgs/Header`'s output from
/// `builtin_interfaces/Time`. A dependency `lookup` does not resolve (not an
/// interface package, or not installed) contributes nothing, exactly as it
/// contributes nothing to generation.
pub fn fingerprint(
    name: &str,
    share_dir: &Path,
    lookup: &dyn Fn(&str) -> Option<PathBuf>,
) -> io::Result<String> {
    // name -> share dir, in a stable order; the root is hashed first under its
    // own name so two packages with identical closures still differ.
    let mut closure: BTreeMap<String, PathBuf> = BTreeMap::new();
    let mut queue = vec![(name.to_string(), share_dir.to_path_buf())];
    while let Some((pkg, dir)) = queue.pop() {
        if closure.contains_key(&pkg) {
            continue;
        }
        let body = std::fs::read_to_string(dir.join("package.xml")).unwrap_or_default();
        closure.insert(pkg, dir);
        for dep in package_xml_deps(&body) {
            if !closure.contains_key(&dep)
                && let Some(d) = lookup(&dep)
            {
                queue.push((dep, d));
            }
        }
    }

    let mut h = Sha256::new();
    h.update(SCHEME.as_bytes());
    h.update([0]);
    h.update(name.as_bytes());
    h.update([0]);
    for (pkg, dir) in &closure {
        h.update(b"pkg\0");
        h.update(pkg.as_bytes());
        h.update([0]);
        for (rel, path) in package_inputs(dir)? {
            let bytes = std::fs::read(&path)?;
            h.update(rel.as_bytes());
            h.update([0]);
            h.update((bytes.len() as u64).to_le_bytes());
            h.update(&bytes);
        }
    }
    let digest = h.finalize();
    let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
    Ok(format!("sha256:{hex}"))
}

/// `[generated.<package>]` — what one generated package came from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Stamp {
    pub package: String,
    /// [`fingerprint`] of the ament inputs.
    pub ament: String,
    /// The codegen version that emitted it (`EMITTED_CODEGEN_VERSION`).
    pub codegen: u32,
    /// [`crate::codegen_fingerprint`] of the emitters that wrote it (issue
    /// 1825). EMPTY in a record written before the field existed, which never
    /// equals a current stamp, so such a tree regenerates once.
    pub emitter: String,
}

/// [`crate::codegen_fingerprint`], computed once per process: it renders the
/// whole emit corpus (~20 ms), and a sync stamps every package it generates.
fn emitter_fingerprint() -> String {
    static F: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    F.get_or_init(crate::codegen_fingerprint).clone()
}

/// How a generated package's record compares with its inputs now.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Freshness {
    /// Recorded, and the inputs still hash to the record.
    Fresh,
    /// No record: generated before RFC-0104 D5, or by something else. Nothing
    /// says it is current, so it is treated as moved.
    Unrecorded,
    /// The inputs (or the codegen version) moved since the record was written.
    Moved { recorded: Stamp },
}

impl Freshness {
    pub fn is_fresh(&self) -> bool {
        matches!(self, Self::Fresh)
    }
}

impl Stamp {
    /// The stamp `name` at `share_dir` would be generated with NOW.
    pub fn current(
        name: &str,
        share_dir: &Path,
        lookup: &dyn Fn(&str) -> Option<PathBuf>,
    ) -> io::Result<Self> {
        Ok(Self {
            package: name.to_string(),
            ament: fingerprint(name, share_dir, lookup)?,
            codegen: crate::EMITTED_CODEGEN_VERSION,
            emitter: emitter_fingerprint(),
        })
    }

    /// The record file inside a generated package's directory.
    pub fn path_in(generated_pkg_dir: &Path) -> PathBuf {
        generated_pkg_dir.join(STAMP_FILE)
    }

    pub fn to_toml(&self) -> String {
        let mut inner = toml::Table::new();
        inner.insert("ament".into(), toml::Value::String(self.ament.clone()));
        inner.insert("codegen".into(), toml::Value::Integer(self.codegen.into()));
        inner.insert("emitter".into(), toml::Value::String(self.emitter.clone()));
        let mut pkgs = toml::Table::new();
        pkgs.insert(self.package.clone(), toml::Value::Table(inner));
        let mut doc = toml::Table::new();
        doc.insert("generated".into(), toml::Value::Table(pkgs));
        format!(
            "# Written by nros codegen (RFC-0104 D5): the ament inputs this package was\n\
             # generated FROM. A build regenerates the package when they move.\n{}",
            toml::to_string(&doc).unwrap_or_default()
        )
    }

    /// Parse a record; `None` for anything that is not exactly one
    /// `[generated.<pkg>]` with `ament` and `codegen`. A missing `emitter` reads
    /// as empty rather than as malformed: such a record predates issue 1825, and
    /// "moved" (regenerate, and say why) is a truer answer than "unrecorded".
    pub fn parse(text: &str) -> Option<Self> {
        let doc: toml::Table = text.parse().ok()?;
        let pkgs = doc.get("generated")?.as_table()?;
        if pkgs.len() != 1 {
            return None;
        }
        let (package, inner) = pkgs.iter().next()?;
        let inner = inner.as_table()?;
        Some(Self {
            package: package.clone(),
            ament: inner.get("ament")?.as_str()?.to_string(),
            codegen: u32::try_from(inner.get("codegen")?.as_integer()?).ok()?,
            emitter: match inner.get("emitter") {
                None => String::new(),
                Some(v) => v.as_str()?.to_string(),
            },
        })
    }

    /// Read a record file; `None` when absent or unreadable.
    pub fn read(path: &Path) -> Option<Self> {
        Self::parse(&std::fs::read_to_string(path).ok()?)
    }

    /// Write the record — only when its content changed, so an unchanged
    /// stamp is never a newer input to anything keyed on its mtime.
    /// Returns whether the file was (re)written.
    pub fn write(&self, path: &Path) -> io::Result<bool> {
        let text = self.to_toml();
        if std::fs::read_to_string(path).is_ok_and(|cur| cur == text) {
            return Ok(false);
        }
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, text)?;
        Ok(true)
    }

    /// Compare this (current) stamp with the record at `path`.
    pub fn freshness_against(&self, path: &Path) -> Freshness {
        match Self::read(path) {
            None => Freshness::Unrecorded,
            Some(r) if &r == self => Freshness::Fresh,
            Some(recorded) => Freshness::Moved { recorded },
        }
    }

    /// One line saying why a package is being regenerated, for `nros build`
    /// and the cmake configure log alike.
    pub fn moved_reason(&self, f: &Freshness) -> String {
        let short = |s: &str| {
            s.trim_start_matches("sha256:")
                .chars()
                .take(12)
                .collect::<String>()
        };
        match f {
            Freshness::Fresh => format!("{} is current", self.package),
            Freshness::Unrecorded => format!(
                "{} has no record of the ament inputs it was generated from (RFC-0104 D5)",
                self.package
            ),
            Freshness::Moved { recorded } if recorded.codegen != self.codegen => format!(
                "{} was generated by codegen version {}, this emitter is {}",
                self.package, recorded.codegen, self.codegen
            ),
            Freshness::Moved { recorded } if recorded.emitter.is_empty() => format!(
                "{}'s record predates the emitter fingerprint (issue 1825), so nothing says \
                 this emitter wrote it",
                self.package
            ),
            Freshness::Moved { recorded } if recorded.emitter != self.emitter => format!(
                "{} was generated by a different emitter (fingerprint {}, this one {}) at the \
                 same codegen version {}",
                self.package,
                short(&recorded.emitter),
                short(&self.emitter),
                self.codegen
            ),
            Freshness::Moved { recorded } => format!(
                "{}'s ament inputs moved since it was generated (recorded {}, now {})",
                self.package,
                short(&recorded.ament),
                short(&self.ament)
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn pkg(root: &Path, name: &str, deps: &[&str], msgs: &[(&str, &str)]) -> PathBuf {
        let dir = root.join(name);
        fs::create_dir_all(dir.join("msg")).unwrap();
        let depends: String = deps
            .iter()
            .map(|d| format!("  <depend>{d}</depend>\n"))
            .collect();
        fs::write(
            dir.join("package.xml"),
            format!(
                "<package format=\"3\">\n  <name>{name}</name>\n  <version>1.0.0</version>\n{depends}</package>\n"
            ),
        )
        .unwrap();
        for (file, body) in msgs {
            fs::write(dir.join("msg").join(file), body).unwrap();
        }
        dir
    }

    fn lookup_in(root: &Path) -> impl Fn(&str) -> Option<PathBuf> + '_ {
        move |n: &str| {
            let d = root.join(n);
            d.join("package.xml").is_file().then_some(d)
        }
    }

    #[test]
    fn deps_ignore_comments_and_similar_tags() {
        let body = "<depend>a</depend>\n<!-- <depend>gone</depend> -->\n\
                    <build_depend condition=\"$X\">b</build_depend>\n<dependx>no</dependx>\n\
                    <exec_depend> c </exec_depend>";
        assert_eq!(package_xml_deps(body), vec!["a", "b", "c"]);
    }

    #[test]
    fn fingerprint_is_location_independent_and_content_sensitive() {
        let t = tempfile::tempdir().unwrap();
        let a = pkg(
            &t.path().join("a"),
            "std_msgs",
            &[],
            &[("String.msg", "string data\n")],
        );
        let b = pkg(
            &t.path().join("b"),
            "std_msgs",
            &[],
            &[("String.msg", "string data\n")],
        );
        let none = |_: &str| None;
        let fa = fingerprint("std_msgs", &a, &none).unwrap();
        // A byte-identical copy elsewhere is the same input.
        assert_eq!(fa, fingerprint("std_msgs", &b, &none).unwrap());
        // A field added (the issue-1780 scenario) moves it.
        fs::write(b.join("msg/String.msg"), "string data\nint32 m2_extra\n").unwrap();
        assert_ne!(fa, fingerprint("std_msgs", &b, &none).unwrap());
    }

    #[test]
    fn package_xml_version_bump_moves_the_fingerprint() {
        let t = tempfile::tempdir().unwrap();
        let a = pkg(
            t.path(),
            "std_msgs",
            &[],
            &[("String.msg", "string data\n")],
        );
        let none = |_: &str| None;
        let before = fingerprint("std_msgs", &a, &none).unwrap();
        let pxml = a.join("package.xml");
        let body = fs::read_to_string(&pxml).unwrap().replace("1.0.0", "1.0.1");
        fs::write(&pxml, body).unwrap();
        assert_ne!(before, fingerprint("std_msgs", &a, &none).unwrap());
    }

    #[test]
    fn a_dependency_change_moves_the_dependent() {
        let t = tempfile::tempdir().unwrap();
        let bi = pkg(
            t.path(),
            "builtin_interfaces",
            &[],
            &[("Time.msg", "int32 sec\n")],
        );
        pkg(
            t.path(),
            "std_msgs",
            &["builtin_interfaces"],
            &[("Header.msg", "builtin_interfaces/Time stamp\n")],
        );
        let look = lookup_in(t.path());
        let std_dir = t.path().join("std_msgs");
        let before = fingerprint("std_msgs", &std_dir, &look).unwrap();
        fs::write(bi.join("msg/Time.msg"), "int32 sec\nuint32 nanosec\n").unwrap();
        assert_ne!(before, fingerprint("std_msgs", &std_dir, &look).unwrap());
        // Negative control: with the dependency unresolvable, the same edit is
        // invisible — the closure is what carries it.
        let none = |_: &str| None;
        let a = fingerprint("std_msgs", &std_dir, &none).unwrap();
        fs::write(bi.join("msg/Time.msg"), "int32 sec\n").unwrap();
        assert_eq!(a, fingerprint("std_msgs", &std_dir, &none).unwrap());
    }

    #[test]
    fn unrelated_files_do_not_move_it() {
        let t = tempfile::tempdir().unwrap();
        let a = pkg(
            t.path(),
            "std_msgs",
            &[],
            &[("String.msg", "string data\n")],
        );
        let none = |_: &str| None;
        let before = fingerprint("std_msgs", &a, &none).unwrap();
        fs::create_dir_all(a.join("cmake")).unwrap();
        fs::write(a.join("cmake/std_msgsConfig.cmake"), "# noise").unwrap();
        fs::write(a.join("local_setup.bash"), "# noise").unwrap();
        assert_eq!(before, fingerprint("std_msgs", &a, &none).unwrap());
    }

    #[test]
    fn stamp_round_trips_and_detects_moves() {
        let t = tempfile::tempdir().unwrap();
        let share = pkg(
            &t.path().join("share"),
            "std_msgs",
            &[],
            &[("String.msg", "string data\n")],
        );
        let out = t.path().join("generated/std_msgs");
        let none = |_: &str| None;
        let path = Stamp::path_in(&out);

        let s = Stamp::current("std_msgs", &share, &none).unwrap();
        assert_eq!(s.freshness_against(&path), Freshness::Unrecorded);
        assert!(s.write(&path).unwrap());
        // Write-if-changed: an identical record is not rewritten.
        assert!(!s.write(&path).unwrap());
        assert_eq!(Stamp::read(&path).as_ref(), Some(&s));
        assert!(s.freshness_against(&path).is_fresh());

        fs::write(
            share.join("msg/String.msg"),
            "string data\nint32 m2_extra\n",
        )
        .unwrap();
        let now = Stamp::current("std_msgs", &share, &none).unwrap();
        let f = now.freshness_against(&path);
        assert_eq!(
            f,
            Freshness::Moved {
                recorded: s.clone()
            }
        );
        assert!(now.moved_reason(&f).contains("ament inputs moved"));

        // A different codegen version is a move too, and says so.
        let other = Stamp {
            codegen: s.codegen + 1,
            ..s.clone()
        };
        let f = other.freshness_against(&path);
        assert!(other.moved_reason(&f).contains("codegen version"));
    }

    /// Issue 1825 — the pin-bump case. Same ament inputs, same codegen VERSION,
    /// different emitter: the shape of two release toolchains that both say
    /// version N and emit different bytes. Before the `emitter` field this read
    /// as Fresh.
    #[test]
    fn a_different_emitter_at_the_same_codegen_version_is_a_move() {
        let t = tempfile::tempdir().unwrap();
        let share = pkg(
            &t.path().join("share"),
            "std_msgs",
            &[],
            &[("String.msg", "string data\n")],
        );
        let path = Stamp::path_in(&t.path().join("generated/std_msgs"));
        let none = |_: &str| None;

        let now = Stamp::current("std_msgs", &share, &none).unwrap();
        assert_eq!(
            now.emitter,
            crate::codegen_fingerprint(),
            "stamped with this build's emitters"
        );
        let by_a = Stamp {
            emitter: "0".repeat(64),
            ..now.clone()
        };
        by_a.write(&path).unwrap();

        let f = now.freshness_against(&path);
        assert_eq!(
            f,
            Freshness::Moved {
                recorded: by_a.clone()
            }
        );
        let why = now.moved_reason(&f);
        assert!(
            why.contains("different emitter") && why.contains(&now.codegen.to_string()),
            "{why}"
        );
        // Negative control: the same emitter, everything else equal, is current.
        now.write(&path).unwrap();
        assert!(now.freshness_against(&path).is_fresh());
    }

    /// A record written before the field existed regenerates once and says why,
    /// instead of reading as unrecorded (which would claim no record exists).
    #[test]
    fn a_record_without_an_emitter_reads_as_moved() {
        let t = tempfile::tempdir().unwrap();
        let share = pkg(
            &t.path().join("share"),
            "std_msgs",
            &[],
            &[("String.msg", "string data\n")],
        );
        let path = Stamp::path_in(&t.path().join("generated/std_msgs"));
        let now = Stamp::current("std_msgs", &share, &|_: &str| None).unwrap();
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(
            &path,
            format!(
                "[generated.std_msgs]\nament = \"{}\"\ncodegen = {}\n",
                now.ament, now.codegen
            ),
        )
        .unwrap();
        let f = now.freshness_against(&path);
        assert!(
            matches!(&f, Freshness::Moved { recorded } if recorded.emitter.is_empty()),
            "{f:?}"
        );
        assert!(
            now.moved_reason(&f)
                .contains("predates the emitter fingerprint")
        );
    }

    #[test]
    fn malformed_records_read_as_unrecorded() {
        assert_eq!(Stamp::parse("nonsense = ["), None);
        assert_eq!(Stamp::parse("[generated.a]\nament = \"x\"\n"), None);
        assert_eq!(
            Stamp::parse(
                "[generated.a]\nament = \"x\"\ncodegen = 3\n[generated.b]\nament = \"y\"\ncodegen = 3\n"
            ),
            None
        );
    }
}
