//! Which executables a CMake build defines, asked of CMake itself through its
//! file API (`cmake-file-api(7)`, codemodel v2).
//!
//! A NAMED owned entry (RFC-0071 D19) is the user's own package: its
//! executable is whatever its own `CMakeLists.txt` calls it, so nothing
//! nano-ros writes knows the name. `nros build` drops a client query into the
//! binary dir before the configure ([`request_codemodel`]); after the build,
//! [`entry_executable`] reads CMake's reply and names the one EXECUTABLE
//! target whose source dir is the entry's.

use std::path::{Path, PathBuf};

use serde_json::Value;

/// The client name CMake files nano-ros's query and reply under.
const CLIENT: &str = "client-nros";

fn api_dir(build_dir: &Path) -> PathBuf {
    build_dir.join(".cmake").join("api").join("v1")
}

/// Ask the next configure of `build_dir` to write a codemodel reply.
///
/// # Errors
/// The query file could not be written.
pub fn request_codemodel(build_dir: &Path) -> std::io::Result<()> {
    let q = api_dir(build_dir).join("query").join(CLIENT);
    std::fs::create_dir_all(&q)?;
    let f = q.join("codemodel-v2");
    if !f.is_file() {
        std::fs::write(f, b"")?;
    }
    Ok(())
}

fn read_json(p: &Path) -> Result<Value, String> {
    let text =
        std::fs::read_to_string(p).map_err(|e| format!("cannot read {}: {e}", p.display()))?;
    serde_json::from_str(&text).map_err(|e| format!("{} is not JSON: {e}", p.display()))
}

/// An EXECUTABLE target and the file it builds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Executable {
    /// The CMake target name.
    pub name: String,
    /// The program's name: the built file's name, its `OUTPUT_NAME`. A
    /// registered component's carrier is the target `<pkg>__<exe>` and the
    /// program `<exe>` (RFC-0071 D20), so this is the name a user picks it by.
    pub program: String,
    /// The built file, absolute.
    pub path: PathBuf,
}

/// Every EXECUTABLE target of the configure in `build_dir` whose source dir
/// is `source_dir` or below it.
///
/// # Errors
/// No reply (the build dir was configured without the query), or a reply
/// that does not have the codemodel's shape.
pub fn executables_under(build_dir: &Path, source_dir: &Path) -> Result<Vec<Executable>, String> {
    let reply = api_dir(build_dir).join("reply");
    let mut indexes: Vec<PathBuf> = std::fs::read_dir(&reply)
        .map_err(|_| no_reply(build_dir))?
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("index-") && n.ends_with(".json"))
        })
        .collect();
    // CMake names each index by its time; the newest sorts last.
    indexes.sort();
    let index = read_json(indexes.last().ok_or_else(|| no_reply(build_dir))?)?;
    let codemodel_file = index
        .pointer(&format!("/reply/{CLIENT}/codemodel-v2/jsonFile"))
        .and_then(Value::as_str)
        .ok_or_else(|| no_reply(build_dir))?;
    let codemodel = read_json(&reply.join(codemodel_file))?;
    let shape = |what: &str| format!("{}: the codemodel reply has no {what}", build_dir.display());
    let top_source = codemodel
        .pointer("/paths/source")
        .and_then(Value::as_str)
        .map(PathBuf::from)
        .ok_or_else(|| shape("paths.source"))?;
    let top_build = codemodel
        .pointer("/paths/build")
        .and_then(Value::as_str)
        .map(PathBuf::from)
        .ok_or_else(|| shape("paths.build"))?;
    // A single-config generator (Ninja, Makefiles) has exactly one.
    let targets = codemodel
        .pointer("/configurations/0/targets")
        .and_then(Value::as_array)
        .ok_or_else(|| shape("configurations[0].targets"))?;
    let want = canonical(source_dir);
    let mut out = Vec::new();
    for t in targets {
        let Some(file) = t.get("jsonFile").and_then(Value::as_str) else {
            continue;
        };
        let target = read_json(&reply.join(file))?;
        if target.get("type").and_then(Value::as_str) != Some("EXECUTABLE") {
            continue;
        }
        let Some(src) = target.pointer("/paths/source").and_then(Value::as_str) else {
            continue;
        };
        if !canonical(&top_source.join(src)).starts_with(&want) {
            continue;
        }
        let Some(artifact) = target.pointer("/artifacts/0/path").and_then(Value::as_str) else {
            continue;
        };
        let path = top_build.join(artifact);
        out.push(Executable {
            name: target
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            program: path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default()
                .to_string(),
            path,
        });
    }
    out.sort_by(|a, b| a.program.cmp(&b.program));
    Ok(out)
}

/// The program a named owned entry runs for an image: its one executable, or
/// the one `exec` names when it builds several (RFC-0071 D20: the choice
/// belongs to the run, as `ros2 run <pkg> <exe>`).
///
/// # Errors
/// No reply; an entry that defines no executable; an `exec` naming none of
/// its programs; or several programs and no `exec`.
pub fn entry_executable(
    build_dir: &Path,
    entry_dir: &Path,
    exec: Option<&str>,
) -> Result<PathBuf, String> {
    let found = executables_under(build_dir, entry_dir)?;
    let programs = || {
        found
            .iter()
            .map(|e| format!("`{}`", e.program))
            .collect::<Vec<_>>()
            .join(", ")
    };
    if found.is_empty() {
        return Err(format!(
            "the owned entry at {} defines no executable (no `add_executable()` in its \
             CMakeLists.txt), so there is nothing to run",
            entry_dir.display()
        ));
    }
    if let Some(want) = exec {
        return found
            .iter()
            .find(|e| e.program == want)
            .map(|e| e.path.clone())
            .ok_or_else(|| {
                format!(
                    "the owned entry at {} has no program `{want}`; it builds {}",
                    entry_dir.display(),
                    programs()
                )
            });
    }
    match found.as_slice() {
        [one] => Ok(one.path.clone()),
        several => Err(format!(
            "the owned entry at {} builds {} programs ({}), and an image runs one. Name it \
             with `nros run <image> --exec <name>`, or in the bringup's launch file as \
             `<node pkg=\"{}\" exec=\"<name>\">`",
            entry_dir.display(),
            several.len(),
            programs(),
            entry_dir
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("<entry>")
        )),
    }
}

fn no_reply(build_dir: &Path) -> String {
    format!(
        "{} has no CMake file-API reply naming its executables; run `nros build` for this \
         image, which asks CMake for one",
        build_dir.display()
    )
}

fn canonical(p: &Path) -> PathBuf {
    std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A reply in CMake's shape: one executable and one library under the
    /// entry, one executable elsewhere.
    fn reply(dir: &Path, entry_execs: &[&str]) -> (PathBuf, PathBuf) {
        let src = dir.join("ws");
        let entry = src.join("src").join("pkg_a");
        std::fs::create_dir_all(&entry).unwrap();
        std::fs::create_dir_all(src.join("other")).unwrap();
        let build = dir.join("build");
        let r = api_dir(&build).join("reply");
        std::fs::create_dir_all(&r).unwrap();
        let mut targets = Vec::new();
        let mut add = |name: &str, ty: &str, source: &str| {
            let file = format!("target-{name}.json");
            std::fs::write(
                r.join(&file),
                serde_json::json!({
                    "name": name, "type": ty,
                    "paths": {"source": source, "build": "pkg/pkg_a"},
                    "artifacts": [{"path": format!("pkg/pkg_a/{name}")}],
                })
                .to_string(),
            )
            .unwrap();
            targets.push(serde_json::json!({"name": name, "jsonFile": file}));
        };
        for e in entry_execs {
            add(e, "EXECUTABLE", "src/pkg_a");
        }
        add("helper", "STATIC_LIBRARY", "src/pkg_a");
        add("elsewhere", "EXECUTABLE", "other");
        std::fs::write(
            r.join("codemodel-v2-1.json"),
            serde_json::json!({
                "paths": {"source": src, "build": build},
                "configurations": [{"targets": targets}],
            })
            .to_string(),
        )
        .unwrap();
        std::fs::write(
            r.join("index-2026-11-20T00-00-00-0000.json"),
            serde_json::json!({"reply": {CLIENT: {"codemodel-v2": {"jsonFile": "codemodel-v2-1.json"}}}})
                .to_string(),
        )
        .unwrap();
        (build, entry)
    }

    #[test]
    fn one_executable_under_the_entry_is_the_program() {
        let t = tempfile::tempdir().unwrap();
        let (build, entry) = reply(t.path(), &["c_talker"]);
        assert_eq!(
            entry_executable(&build, &entry, None).unwrap(),
            build.join("pkg/pkg_a/c_talker")
        );
    }

    #[test]
    fn several_executables_are_refused_by_name() {
        let t = tempfile::tempdir().unwrap();
        let (build, entry) = reply(t.path(), &["test_a", "test_b"]);
        let e = entry_executable(&build, &entry, None).unwrap_err();
        assert!(e.contains("`test_a`, `test_b`"), "{e}");
        assert!(e.contains("--exec <name>"), "{e}");
        assert!(e.contains("exec=\"<name>\""), "{e}");
    }

    /// phase-583 W2 -- `exec` picks one program of several, by its program
    /// name, and a name that is none of them is refused, listing them.
    #[test]
    fn exec_picks_one_program_of_several() {
        let t = tempfile::tempdir().unwrap();
        let (build, entry) = reply(t.path(), &["test_a", "test_b"]);
        assert_eq!(
            entry_executable(&build, &entry, Some("test_b")).unwrap(),
            build.join("pkg/pkg_a/test_b")
        );
        let e = entry_executable(&build, &entry, Some("test_c")).unwrap_err();
        assert!(
            e.contains("no program `test_c`") && e.contains("`test_a`, `test_b`"),
            "{e}"
        );
    }

    /// phase-583 W1/W2 -- a registered component's carrier is the target
    /// `<pkg>__<exe>` and the program `<exe>`: the program is the artifact's
    /// name, so that is what a refusal lists and what `exec` matches.
    #[test]
    fn the_program_is_the_artifact_name_not_the_target() {
        let t = tempfile::tempdir().unwrap();
        let (build, entry) = reply(t.path(), &[]);
        let r = api_dir(&build).join("reply");
        let src = t.path().join("ws");
        for exe in ["talker", "listener"] {
            std::fs::write(
                r.join(format!("target-pkg_a__{exe}.json")),
                serde_json::json!({
                    "name": format!("pkg_a__{exe}"), "type": "EXECUTABLE",
                    "paths": {"source": "src/pkg_a", "build": "pkg/pkg_a"},
                    "artifacts": [{"path": format!("pkg/pkg_a/{exe}")}],
                })
                .to_string(),
            )
            .unwrap();
        }
        let cm = r.join("codemodel-v2-1.json");
        std::fs::write(
            &cm,
            serde_json::json!({
                "paths": {"source": src, "build": build},
                "configurations": [{"targets": [
                    {"jsonFile": "target-pkg_a__talker.json"},
                    {"jsonFile": "target-pkg_a__listener.json"},
                ]}],
            })
            .to_string(),
        )
        .unwrap();
        let e = entry_executable(&build, &entry, None).unwrap_err();
        assert!(e.contains("(`listener`, `talker`)"), "{e}");
        assert!(
            !e.contains("pkg_a__"),
            "the refusal lists programs, not targets: {e}"
        );
        assert_eq!(
            entry_executable(&build, &entry, Some("listener")).unwrap(),
            build.join("pkg/pkg_a/listener")
        );
    }

    #[test]
    fn no_executable_and_no_reply_are_each_refused() {
        let t = tempfile::tempdir().unwrap();
        let (build, entry) = reply(t.path(), &[]);
        assert!(
            entry_executable(&build, &entry, None)
                .unwrap_err()
                .contains("defines no executable")
        );
        let e = entry_executable(&t.path().join("unconfigured"), &entry, None).unwrap_err();
        assert!(e.contains("no CMake file-API reply"), "{e}");
    }

    #[test]
    fn the_query_is_where_cmake_reads_it() {
        let t = tempfile::tempdir().unwrap();
        request_codemodel(t.path()).unwrap();
        assert!(
            t.path()
                .join(".cmake/api/v1/query/client-nros/codemodel-v2")
                .is_file()
        );
    }
}
