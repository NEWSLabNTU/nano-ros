//! Phase 212.L.6 — launch file synthesis + multi-launch resolution.
//!
//! **DEPRECATED (phase-296 R3, removed in R4).** This module is the heart of
//! the legacy launch-XML bake path. The canonical config path is the
//! play_launch-resolved SystemModel (`--model` / `MODEL` /
//! `nros::main!(model = …)`), which never synthesises or parses launch XML.
//! Do not add new callers; migrate the remaining ones (`plan`,
//! `codegen-system` without `--model`) to the model path. Left un-attributed
//! (no `#[deprecated]`) only because its in-crate callers compile under
//! `-D warnings` until R4 deletes both.
//!
//! Every nros verb that needs a launch file (`plan`, `launch`,
//! `codegen-system`) shares the same resolution policy:
//!
//! 1. Honour an explicit `--file <path>` argument.
//! 2. Pick `<pkg>/launch/<pkg-name>.launch.xml` if it exists.
//! 3. Pick `<pkg>/launch/system.launch.xml` if it exists.
//! 4. Pick the single `*.launch.xml` under `<pkg>/launch/` when there is
//!    exactly one.
//! 5. For component / application pkgs (Cargo.toml or CMakeLists.txt
//!    present) with no launch file, **synthesise** a minimal
//!    `<launch><node pkg="…" exec="…" /></launch>` body in memory.
//! 6. Otherwise (Path A bringup pkg with no launch file, or multi-launch
//!    ambiguity), hard-error with a candidate list.
//!
//! The synthesised XML is returned as a string — callers materialise it
//! to a temp file when they need to hand it to the external
//! `play_launch_parser` binary. It is never persisted alongside the
//! bringup.

use std::{
    fs,
    path::{Path, PathBuf},
};

use eyre::{Result, eyre};

/// One launch input — either an on-disk file or an in-memory XML body
/// synthesised by [`resolve_launch`] for self-bringup pkgs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LaunchInput {
    /// Pre-existing file on disk (resolved by steps 1-4 of the policy).
    File(PathBuf),
    /// In-memory XML body synthesised for a self-bringup pkg (step 5).
    Synth(String),
}

impl LaunchInput {
    /// Materialise the launch input to a real filesystem path so it can be
    /// fed to the external `play_launch_parser` binary. For an on-disk
    /// [`LaunchInput::File`] this is a no-op; for [`LaunchInput::Synth`]
    /// it writes a uniquely-named temp file and returns an RAII guard
    /// that removes the file on drop.
    pub fn materialise(&self) -> Result<MaterialisedLaunch> {
        match self {
            LaunchInput::File(p) => Ok(MaterialisedLaunch {
                path: p.clone(),
                _guard: None,
            }),
            LaunchInput::Synth(body) => {
                let dir = std::env::temp_dir();
                let stamp = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_nanos())
                    .unwrap_or(0);
                let path = dir.join(format!(
                    "nros-launch-synth-{}-{stamp}.launch.xml",
                    std::process::id()
                ));
                fs::write(&path, body)
                    .map_err(|e| eyre!("write synthesised launch to {}: {e}", path.display()))?;
                Ok(MaterialisedLaunch {
                    path: path.clone(),
                    _guard: Some(TempPathGuard(path)),
                })
            }
        }
    }
}

/// On-disk handle to a launch file. Owns a temp-file guard when the
/// underlying input was synthesised — dropping cleans up the temp file.
pub struct MaterialisedLaunch {
    pub path: PathBuf,
    _guard: Option<TempPathGuard>,
}

/// RAII: deletes the wrapped path on drop. Used by [`LaunchInput::materialise`]
/// to clean up the synthesised temp file once the parser returns.
struct TempPathGuard(PathBuf);

impl Drop for TempPathGuard {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

/// Errors specific to launch resolution. Wrapped in `eyre` at the
/// callsite; declared as a real enum so tests can match on the failure
/// mode without parsing strings.
#[derive(Debug, thiserror::Error)]
pub enum LaunchResolveError {
    #[error(
        "no launch file resolved under {pkg}/launch and pkg is a Path A bringup \
         (no Cargo.toml / CMakeLists.txt) — synthesis is disallowed for multi-node bringups"
    )]
    PathABringupNoLaunch { pkg: PathBuf },

    #[error(
        "multiple launch files under {pkg}/launch ({candidates:?}) — pass `--file <name>` \
         to pick one (try one of: {candidates:?})"
    )]
    AmbiguousMultiLaunch {
        pkg: PathBuf,
        candidates: Vec<String>,
    },

    #[error(
        "package {pkg} declares multiple executables ({candidates:?}); \
         pass `--exec <name>` to pick one"
    )]
    AmbiguousExec {
        pkg: PathBuf,
        candidates: Vec<String>,
    },

    #[error(
        "--file {file:?} does not resolve to an existing launch file (tried \
         {pkg}/launch/{file}, ./{file}, and absolute)"
    )]
    FileArgUnresolved { pkg: PathBuf, file: String },

    #[error(
        "could not determine pkg name for {pkg} (no Cargo.toml [package].name, no project() in CMakeLists.txt)"
    )]
    UnknownPkgName { pkg: PathBuf },

    #[error(
        "could not determine executable for {pkg} (no [[bin]] in Cargo.toml, no add_executable in CMakeLists.txt)"
    )]
    UnknownExec { pkg: PathBuf },
}

/// Spec resolver — see module docs.
///
/// `pkg_dir` is the bringup / component / application pkg directory.
/// `file_arg` is the user's `--file` value (passes step 1).
/// `exec_arg` is the user's `--exec` value (consumed only by synth).
pub fn resolve_launch(
    pkg_dir: &Path,
    file_arg: Option<&str>,
    exec_arg: Option<&str>,
) -> Result<LaunchInput> {
    // Step 1 — explicit --file.
    if let Some(file) = file_arg {
        // <pkg>/launch/<file>
        let in_launch = pkg_dir.join("launch").join(file);
        if in_launch.is_file() {
            return Ok(LaunchInput::File(in_launch));
        }
        // ./<file> (cwd-relative)
        let cwd_rel = std::env::current_dir().ok().map(|c| c.join(file));
        if let Some(p) = cwd_rel
            && p.is_file()
        {
            return Ok(LaunchInput::File(p));
        }
        // absolute
        let as_abs = Path::new(file);
        if as_abs.is_absolute() && as_abs.is_file() {
            return Ok(LaunchInput::File(as_abs.to_path_buf()));
        }
        return Err(LaunchResolveError::FileArgUnresolved {
            pkg: pkg_dir.to_path_buf(),
            file: file.to_string(),
        }
        .into());
    }

    // Need pkg name for steps 2 + 5; derive it now (used in error
    // messages too). Allow failure here: a Path A bringup may have
    // neither Cargo.toml nor CMakeLists.txt, in which case we still need
    // to walk steps 2-4 against the directory name.
    let pkg_name = discover_pkg_name(pkg_dir).unwrap_or_else(|_| {
        pkg_dir
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string()
    });

    // Step 2 — pkg-named convention.
    if !pkg_name.is_empty() {
        let pkg_named = pkg_dir
            .join("launch")
            .join(format!("{pkg_name}.launch.xml"));
        if pkg_named.is_file() {
            return Ok(LaunchInput::File(pkg_named));
        }
    }

    // Step 3 — system.launch.xml convention.
    let system_launch = pkg_dir.join("launch").join("system.launch.xml");
    if system_launch.is_file() {
        return Ok(LaunchInput::File(system_launch));
    }

    // Step 4 — single *.launch.xml.
    let candidates = enumerate_launch_files(pkg_dir);
    match candidates.len() {
        0 => {
            // Steps 5/6: synth eligibility check.
            if is_self_bringup_eligible(pkg_dir) {
                let exec_name = match exec_arg {
                    Some(e) => e.to_string(),
                    None => discover_exec_target(pkg_dir, &pkg_name)?,
                };
                Ok(LaunchInput::Synth(synthesise_xml(&pkg_name, &exec_name)))
            } else {
                Err(LaunchResolveError::PathABringupNoLaunch {
                    pkg: pkg_dir.to_path_buf(),
                }
                .into())
            }
        }
        1 => Ok(LaunchInput::File(
            pkg_dir.join("launch").join(&candidates[0]),
        )),
        _ => Err(LaunchResolveError::AmbiguousMultiLaunch {
            pkg: pkg_dir.to_path_buf(),
            candidates,
        }
        .into()),
    }
}

/// True when `pkg_dir` is a component or application pkg eligible for
/// synthesis: has a `Cargo.toml` or `CMakeLists.txt`. Path A bringup
/// pkgs (no Cargo.toml + no CMakeLists.txt + has system.toml) are
/// disqualified.
pub fn is_self_bringup_eligible(pkg_dir: &Path) -> bool {
    let has_cargo = pkg_dir.join("Cargo.toml").is_file();
    let has_cmake = pkg_dir.join("CMakeLists.txt").is_file();
    has_cargo || has_cmake
}

/// Phase 212.L.7 strict self-entry detection.
///
/// Returns `true` when `<pkg-dir>/Cargo.toml` declares BOTH
/// `[package.metadata.nros.node]` (or the canonical `.component` alias)
/// AND `[package.metadata.nros.entry]` — the L.7 single-pkg dev-loop
/// shape where a Node pkg eats its own Entry role (`cargo run`
/// convenience).
///
/// This is a stricter check than [`is_self_bringup_eligible`] above:
/// L.6 synthesis accepts any pkg with a Cargo.toml / CMakeLists.txt;
/// the L.7 self-entry hook only kicks in when both role markers are
/// present. Callers ([`super::planner`] and `cmd::codegen_system`) use
/// it to short-circuit launch-file resolution to the L.6 resolver
/// (real launch.xml file if present, synth XML otherwise) without
/// requiring a sibling bringup pkg.
///
/// Returns `false` (no error) when the Cargo.toml is missing or fails
/// to parse — the planner/codegen-system fall back to their normal
/// resolution path in those cases, and the strict parser errors
/// surface later when the user explicitly invokes the loader. Errors
/// from this fn would forbid a legitimate dual-purpose pkg from being
/// passed to `nros plan`.
pub fn is_self_entry_pkg(pkg_dir: &Path) -> bool {
    let cargo_toml = pkg_dir.join("Cargo.toml");
    if !cargo_toml.is_file() {
        return false;
    }
    let raw = match fs::read_to_string(&cargo_toml) {
        Ok(s) => s,
        Err(_) => return false,
    };
    let doc: toml::Value = match toml::from_str(&raw) {
        Ok(v) => v,
        Err(_) => return false,
    };
    let Some(nros) = doc
        .get("package")
        .and_then(|p| p.get("metadata"))
        .and_then(|m| m.get("nros"))
    else {
        return false;
    };
    let has_node = nros.get("node").is_some() || nros.get("component").is_some();
    let has_entry = nros.get("entry").is_some();
    has_node && has_entry
}

/// Phase 212.L.7 helper — resolve a launch input for a self-entry
/// (`[package.metadata.nros.node]` + `[package.metadata.nros.entry]`)
/// pkg dir using the standard L.6 resolution policy. Thin wrapper
/// around [`resolve_launch`] that the planner + codegen-system both
/// share so the L.7 self-entry behaviour is centralised in one place.
///
/// Callers MUST check [`is_self_entry_pkg`] first; this fn does not
/// re-verify the role markers and will happily resolve a launch for
/// any pkg dir.
pub fn resolve_self_entry_launch(
    pkg_dir: &Path,
    file_arg: Option<&str>,
    exec_arg: Option<&str>,
) -> Result<LaunchInput> {
    resolve_launch(pkg_dir, file_arg, exec_arg)
}

/// Enumerate `*.launch.xml` files directly under `<pkg>/launch/` (no
/// recursion). Returns sorted basenames.
pub fn enumerate_launch_files(pkg_dir: &Path) -> Vec<String> {
    let dir = pkg_dir.join("launch");
    let mut out = Vec::new();
    let Ok(rd) = fs::read_dir(&dir) else {
        return out;
    };
    for entry in rd.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|s| s.to_str()) else {
            continue;
        };
        if name.ends_with(".launch.xml") {
            out.push(name.to_string());
        }
    }
    out.sort();
    out
}

/// Read `pkg-name`:
/// * Rust: `Cargo.toml` `[package].name`
/// * C++: `project(<name> …)` in `CMakeLists.txt`
pub fn discover_pkg_name(pkg_dir: &Path) -> Result<String> {
    let cargo = pkg_dir.join("Cargo.toml");
    if cargo.is_file() {
        if let Some(name) = read_cargo_pkg_name(&cargo)? {
            return Ok(name);
        }
    }
    let cmake = pkg_dir.join("CMakeLists.txt");
    if cmake.is_file() {
        if let Some(name) = read_cmake_project_name(&cmake)? {
            return Ok(name);
        }
    }
    Err(LaunchResolveError::UnknownPkgName {
        pkg: pkg_dir.to_path_buf(),
    }
    .into())
}

fn read_cargo_pkg_name(cargo_toml: &Path) -> Result<Option<String>> {
    let raw =
        fs::read_to_string(cargo_toml).map_err(|e| eyre!("read {}: {e}", cargo_toml.display()))?;
    let doc: toml::Value =
        toml::from_str(&raw).map_err(|e| eyre!("parse {}: {e}", cargo_toml.display()))?;
    Ok(doc
        .get("package")
        .and_then(|t| t.get("name"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string()))
}

fn read_cmake_project_name(cmake: &Path) -> Result<Option<String>> {
    let raw = fs::read_to_string(cmake).map_err(|e| eyre!("read {}: {e}", cmake.display()))?;
    // Minimal scan: `project(<name> …)` — first `project(` call wins.
    // Whitespace-tolerant; strips a trailing version token if present.
    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('#') {
            continue;
        }
        if let Some(rest) = trimmed
            .strip_prefix("project(")
            .or_else(|| trimmed.strip_prefix("project ("))
        {
            // first token up to whitespace, ')', or 'VERSION'
            let mut name = String::new();
            for ch in rest.chars() {
                if ch.is_whitespace() || ch == ')' {
                    break;
                }
                name.push(ch);
            }
            if !name.is_empty() {
                return Ok(Some(name));
            }
        }
    }
    Ok(None)
}

/// Resolve the executable target name for synthesis:
///
/// * `--exec <name>` arg wins.
/// * Single `[[bin]]` (Rust) → that bin's `name`.
/// * Single `add_executable(<name> …)` (C++) → that target.
/// * Component pkg w/ no bins (only `[lib]`) → the pkg name (codegen
///   synth-main convention).
/// * Multiple candidates → hard error.
pub fn discover_exec_target(pkg_dir: &Path, pkg_name: &str) -> Result<String> {
    // Rust path.
    let cargo = pkg_dir.join("Cargo.toml");
    if cargo.is_file() {
        let bins = read_cargo_bins(&cargo)?;
        match bins.len() {
            0 => {
                // Component pkg with no bins → use pkg name (codegen
                // synthesises a main bin named after the pkg).
                return Ok(pkg_name.to_string());
            }
            1 => return Ok(bins.into_iter().next().unwrap()),
            _ => {
                return Err(LaunchResolveError::AmbiguousExec {
                    pkg: pkg_dir.to_path_buf(),
                    candidates: bins,
                }
                .into());
            }
        }
    }

    // C++ path.
    let cmake = pkg_dir.join("CMakeLists.txt");
    if cmake.is_file() {
        let execs = read_cmake_add_executables(&cmake)?;
        match execs.len() {
            0 => {
                return Err(LaunchResolveError::UnknownExec {
                    pkg: pkg_dir.to_path_buf(),
                }
                .into());
            }
            1 => return Ok(execs.into_iter().next().unwrap()),
            _ => {
                return Err(LaunchResolveError::AmbiguousExec {
                    pkg: pkg_dir.to_path_buf(),
                    candidates: execs,
                }
                .into());
            }
        }
    }

    Err(LaunchResolveError::UnknownExec {
        pkg: pkg_dir.to_path_buf(),
    }
    .into())
}

/// Read `[[bin]]` names from a `Cargo.toml`. An implicit bin
/// (`src/main.rs` w/o `[[bin]]`) defaults to the pkg name — captured by
/// the no-bin → pkg-name fallback in [`discover_exec_target`].
fn read_cargo_bins(cargo_toml: &Path) -> Result<Vec<String>> {
    let raw =
        fs::read_to_string(cargo_toml).map_err(|e| eyre!("read {}: {e}", cargo_toml.display()))?;
    let doc: toml::Value =
        toml::from_str(&raw).map_err(|e| eyre!("parse {}: {e}", cargo_toml.display()))?;
    let mut out = Vec::new();
    if let Some(bin_array) = doc.get("bin").and_then(|v| v.as_array()) {
        for b in bin_array {
            if let Some(name) = b.get("name").and_then(|v| v.as_str()) {
                out.push(name.to_string());
            }
        }
    }
    Ok(out)
}

/// Scan `add_executable(<name> …)` calls in a CMakeLists.txt and return
/// the named targets. Minimal text scan — sufficient for the in-tree
/// canonical example shapes; non-trivial CMake-driven exec discovery is
/// out of scope for synthesis.
fn read_cmake_add_executables(cmake: &Path) -> Result<Vec<String>> {
    let raw = fs::read_to_string(cmake).map_err(|e| eyre!("read {}: {e}", cmake.display()))?;
    let mut out = Vec::new();
    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('#') {
            continue;
        }
        if let Some(rest) = trimmed
            .strip_prefix("add_executable(")
            .or_else(|| trimmed.strip_prefix("add_executable ("))
        {
            let mut name = String::new();
            for ch in rest.chars() {
                if ch.is_whitespace() || ch == ')' {
                    break;
                }
                name.push(ch);
            }
            if !name.is_empty() {
                out.push(name);
            }
        }
    }
    Ok(out)
}

/// Render the synth XML body for a single-node bringup.
/// R-code precondition #3 — the model twin of [`synthesise_xml`]: a
/// 1-node in-memory SystemModel for the self-bringup shape (same
/// `discover_pkg_name`/`discover_exec_target` inputs). The planner consumes
/// it via `plan_record_from_model`, so the XML synth (and its parser round
/// trip) dies with this module.
pub fn synthesise_self_model(
    pkg_name: &str,
    exec_name: &str,
) -> ros_launch_manifest_model::SystemModel {
    let mut m = ros_launch_manifest_model::SystemModel::default();
    m.meta.version = ros_launch_manifest_model::SCHEMA_VERSION;
    m.structure.nodes.insert(
        format!("/{exec_name}"),
        ros_launch_manifest_model::NodeInstance {
            pkg: Some(pkg_name.to_string()),
            exec: Some(exec_name.to_string()),
            ..Default::default()
        },
    );
    m
}

pub fn synthesise_xml(pkg_name: &str, exec_name: &str) -> String {
    format!(
        "<?xml version=\"1.0\"?>\n\
         <!-- synthesised by nros (Phase 212.L.6) — not persisted to disk -->\n\
         <launch>\n  <node pkg=\"{}\" exec=\"{}\" />\n</launch>\n",
        xml_escape(pkg_name),
        xml_escape(exec_name),
    )
}

fn xml_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '&' => out.push_str("&amp;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            c => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_pkg(name: &str) -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let p = std::env::temp_dir().join(format!(
            "nros-launch-synth-{name}-{}-{stamp}",
            std::process::id()
        ));
        fs::create_dir_all(&p).unwrap();
        p
    }

    fn write_cargo_with_bin(dir: &Path, pkg: &str, bin: &str) {
        fs::write(
            dir.join("Cargo.toml"),
            format!(
                r#"[package]
name = "{pkg}"
version = "0.1.0"
edition = "2021"

[[bin]]
name = "{bin}"
path = "src/main.rs"
"#
            ),
        )
        .unwrap();
    }

    fn write_cargo_lib_only(dir: &Path, pkg: &str) {
        fs::write(
            dir.join("Cargo.toml"),
            format!(
                r#"[package]
name = "{pkg}"
version = "0.1.0"
edition = "2021"

[lib]
path = "src/lib.rs"
"#
            ),
        )
        .unwrap();
    }

    #[test]
    fn resolve_picks_pkg_named_default_when_present() {
        let p = temp_pkg("pkg-named");
        write_cargo_with_bin(&p, "talker_pkg", "talker_pkg");
        fs::create_dir_all(p.join("launch")).unwrap();
        let target = p.join("launch/talker_pkg.launch.xml");
        fs::write(&target, "<launch/>").unwrap();
        // Also write system.launch.xml to prove pkg-named wins.
        fs::write(p.join("launch/system.launch.xml"), "<launch/>").unwrap();
        let r = resolve_launch(&p, None, None).unwrap();
        match r {
            LaunchInput::File(path) => assert_eq!(path, target),
            other => panic!("expected File, got {other:?}"),
        }
    }

    #[test]
    fn resolve_picks_system_launch_xml_when_no_pkg_name_match() {
        let p = temp_pkg("system-default");
        write_cargo_with_bin(&p, "alpha", "alpha");
        fs::create_dir_all(p.join("launch")).unwrap();
        // No alpha.launch.xml.
        let target = p.join("launch/system.launch.xml");
        fs::write(&target, "<launch/>").unwrap();
        // Extra unrelated file — system.launch.xml still wins by name.
        fs::write(p.join("launch/other.launch.xml"), "<launch/>").unwrap();
        let r = resolve_launch(&p, None, None).unwrap();
        match r {
            LaunchInput::File(path) => assert_eq!(path, target),
            other => panic!("expected File, got {other:?}"),
        }
    }

    #[test]
    fn resolve_picks_single_file_when_only_one() {
        let p = temp_pkg("single-file");
        write_cargo_with_bin(&p, "alpha", "alpha");
        fs::create_dir_all(p.join("launch")).unwrap();
        let target = p.join("launch/only.launch.xml");
        fs::write(&target, "<launch/>").unwrap();
        let r = resolve_launch(&p, None, None).unwrap();
        match r {
            LaunchInput::File(path) => assert_eq!(path, target),
            other => panic!("expected File, got {other:?}"),
        }
    }

    #[test]
    fn resolve_synthesises_for_self_bringup_no_launch() {
        let p = temp_pkg("synth-rust");
        write_cargo_with_bin(&p, "alpha", "alpha");
        // No launch/ dir at all.
        let r = resolve_launch(&p, None, None).unwrap();
        match r {
            LaunchInput::Synth(body) => {
                assert!(body.contains("pkg=\"alpha\""), "body={body}");
                assert!(body.contains("exec=\"alpha\""), "body={body}");
            }
            other => panic!("expected Synth, got {other:?}"),
        }
    }

    #[test]
    fn resolve_refuses_path_a_bringup_with_no_launch() {
        let p = temp_pkg("path-a-bringup");
        // Path A: has package.xml + system.toml but no Cargo.toml /
        // CMakeLists.txt.
        fs::write(p.join("package.xml"), "<package format=\"3\"/>").unwrap();
        fs::write(
            p.join("system.toml"),
            "[system]\nname=\"x\"\nrmw=\"zenoh\"\ndomain_id=0\n",
        )
        .unwrap();
        let err = resolve_launch(&p, None, None).unwrap_err();
        let s = format!("{err:#}");
        assert!(
            s.contains("Path A bringup") || s.contains("synthesis is disallowed"),
            "{s}"
        );
    }

    #[test]
    fn resolve_refuses_ambiguous_multi_launch_no_file_arg() {
        let p = temp_pkg("multi-launch");
        write_cargo_with_bin(&p, "alpha", "alpha");
        fs::create_dir_all(p.join("launch")).unwrap();
        fs::write(p.join("launch/foo.launch.xml"), "<launch/>").unwrap();
        fs::write(p.join("launch/bar.launch.xml"), "<launch/>").unwrap();
        let err = resolve_launch(&p, None, None).unwrap_err();
        let s = format!("{err:#}");
        assert!(s.contains("multiple launch files"), "{s}");
        // Candidates should mention both names somewhere.
        assert!(
            s.contains("foo.launch.xml") && s.contains("bar.launch.xml"),
            "{s}"
        );
    }

    #[test]
    fn synth_xml_uses_pkg_name_for_pkg_attr() {
        let body = synthesise_xml("my_pkg", "my_exec");
        assert!(body.contains("pkg=\"my_pkg\""), "{body}");
    }

    #[test]
    fn synth_xml_uses_single_bin_for_exec_attr() {
        let p = temp_pkg("single-bin");
        // pkg name "outer", bin name "inner_bin" — exec must come from
        // the [[bin]], not the pkg name.
        write_cargo_with_bin(&p, "outer", "inner_bin");
        let r = resolve_launch(&p, None, None).unwrap();
        match r {
            LaunchInput::Synth(body) => {
                assert!(body.contains("exec=\"inner_bin\""), "{body}");
                assert!(body.contains("pkg=\"outer\""), "{body}");
            }
            other => panic!("expected Synth, got {other:?}"),
        }
    }

    #[test]
    fn synth_xml_requires_exec_arg_when_multiple_bins() {
        let p = temp_pkg("multi-bin");
        fs::write(
            p.join("Cargo.toml"),
            r#"[package]
name = "outer"
version = "0.1.0"
edition = "2021"

[[bin]]
name = "bin_a"
path = "src/a.rs"

[[bin]]
name = "bin_b"
path = "src/b.rs"
"#,
        )
        .unwrap();
        // Without --exec → error.
        let err = resolve_launch(&p, None, None).unwrap_err();
        let s = format!("{err:#}");
        assert!(
            s.contains("multiple executables") || s.contains("--exec"),
            "{s}"
        );
        // With --exec arg → synth picks it.
        let r = resolve_launch(&p, None, Some("bin_b")).unwrap();
        match r {
            LaunchInput::Synth(body) => assert!(body.contains("exec=\"bin_b\""), "{body}"),
            other => panic!("expected Synth, got {other:?}"),
        }
    }

    #[test]
    fn resolve_lib_only_component_synth_uses_pkg_name_as_exec() {
        let p = temp_pkg("lib-component");
        write_cargo_lib_only(&p, "comp_alpha");
        let r = resolve_launch(&p, None, None).unwrap();
        match r {
            LaunchInput::Synth(body) => {
                assert!(body.contains("pkg=\"comp_alpha\""), "{body}");
                assert!(body.contains("exec=\"comp_alpha\""), "{body}");
            }
            other => panic!("expected Synth, got {other:?}"),
        }
    }

    #[test]
    fn resolve_file_arg_finds_under_launch_dir() {
        let p = temp_pkg("file-arg-launch");
        write_cargo_with_bin(&p, "alpha", "alpha");
        fs::create_dir_all(p.join("launch")).unwrap();
        let target = p.join("launch/custom.launch.xml");
        fs::write(&target, "<launch/>").unwrap();
        let r = resolve_launch(&p, Some("custom.launch.xml"), None).unwrap();
        match r {
            LaunchInput::File(path) => assert_eq!(path, target),
            other => panic!("expected File, got {other:?}"),
        }
    }

    #[test]
    fn materialise_synth_writes_temp_file_with_xml_body() {
        let body = synthesise_xml("p", "e");
        let input = LaunchInput::Synth(body.clone());
        let m = input.materialise().unwrap();
        let on_disk = fs::read_to_string(&m.path).unwrap();
        assert_eq!(on_disk, body);
        // Drop the guard → temp file is removed.
        let path = m.path.clone();
        drop(m);
        assert!(!path.exists(), "temp file {} not cleaned", path.display());
    }

    #[test]
    fn discover_pkg_name_reads_cmake_project_directive() {
        let p = temp_pkg("cmake-project");
        fs::write(
            p.join("CMakeLists.txt"),
            "# top of file\ncmake_minimum_required(VERSION 3.20)\nproject(my_cpp_pkg VERSION 1.2.3)\n",
        )
        .unwrap();
        let name = discover_pkg_name(&p).unwrap();
        assert_eq!(name, "my_cpp_pkg");
    }

    // -----------------------------------------------------------------
    // Phase 212.L.7 — strict self-entry detection + planner hook
    // -----------------------------------------------------------------

    fn write_self_entry_cargo(dir: &Path, pkg: &str, board: &str) {
        fs::write(
            dir.join("Cargo.toml"),
            format!(
                r#"[package]
name = "{pkg}"
version = "0.1.0"
edition = "2021"

[[bin]]
name = "{pkg}"
path = "src/main.rs"

[package.metadata.nros.node]
class = "{pkg}::Node"
name  = "{pkg}"

[package.metadata.nros.entry]
deploy = "{board}"
"#
            ),
        )
        .unwrap();
    }

    #[test]
    fn is_self_entry_pkg_true_when_both_node_and_entry_present() {
        let p = temp_pkg("l7-self-entry-true");
        write_self_entry_cargo(&p, "alpha_pkg", "freertos");
        assert!(is_self_entry_pkg(&p));
    }

    #[test]
    fn is_self_entry_pkg_false_when_only_node_present() {
        let p = temp_pkg("l7-self-entry-node-only");
        fs::write(
            p.join("Cargo.toml"),
            r#"[package]
name = "alpha_pkg"
version = "0.1.0"
edition = "2021"

[package.metadata.nros.node]
class = "alpha_pkg::Node"
name  = "alpha"
"#,
        )
        .unwrap();
        assert!(!is_self_entry_pkg(&p));
    }

    #[test]
    fn is_self_entry_pkg_false_when_only_entry_present() {
        let p = temp_pkg("l7-self-entry-entry-only");
        fs::write(
            p.join("Cargo.toml"),
            r#"[package]
name = "alpha_entry"
version = "0.1.0"
edition = "2021"

[package.metadata.nros.entry]
deploy = "freertos"
"#,
        )
        .unwrap();
        assert!(!is_self_entry_pkg(&p));
    }

    #[test]
    fn is_self_entry_pkg_accepts_deprecated_component_alias() {
        let p = temp_pkg("l7-self-entry-component-alias");
        fs::write(
            p.join("Cargo.toml"),
            r#"[package]
name = "alpha_pkg"
version = "0.1.0"
edition = "2021"

[package.metadata.nros.component]
class = "alpha_pkg::Node"
name  = "alpha"

[package.metadata.nros.entry]
deploy = "freertos"
"#,
        )
        .unwrap();
        assert!(
            is_self_entry_pkg(&p),
            "deprecated `.component` alias should also activate self-entry"
        );
    }

    #[test]
    fn is_self_entry_pkg_false_when_cargo_toml_missing() {
        let p = temp_pkg("l7-self-entry-nocargo");
        // No Cargo.toml at all → false (planner falls back to its
        // normal Path A bringup resolution).
        assert!(!is_self_entry_pkg(&p));
    }

    /// Self-entry pkg w/ no launch dir → resolver synthesises a 1-node
    /// `<launch>` body in memory (the "1-node plan from Cargo metadata"
    /// shape per the L.7 spec).
    #[test]
    fn nros_plan_self_entry_synthesises_single_node_plan() {
        let p = temp_pkg("l7-synth-no-launch");
        write_self_entry_cargo(&p, "alpha_pkg", "freertos");
        assert!(is_self_entry_pkg(&p));
        let r = resolve_self_entry_launch(&p, None, None).unwrap();
        match r {
            LaunchInput::Synth(body) => {
                assert!(body.contains("pkg=\"alpha_pkg\""), "body={body}");
                assert!(body.contains("exec=\"alpha_pkg\""), "body={body}");
            }
            other => panic!("expected Synth for self-entry pkg, got {other:?}"),
        }
    }

    /// Self-entry pkg with a sibling `launch/<pkg>.launch.xml` → resolver
    /// picks that file instead of synthesising.
    #[test]
    fn nros_plan_self_entry_uses_synth_launch_when_absent() {
        // Two sub-cases — proves the resolver covers both branches of
        // the spec ("try real launch file first, synth otherwise").

        // Branch 1: launch file present.
        let p1 = temp_pkg("l7-real-launch");
        write_self_entry_cargo(&p1, "alpha_pkg", "freertos");
        fs::create_dir_all(p1.join("launch")).unwrap();
        let target = p1.join("launch/alpha_pkg.launch.xml");
        fs::write(&target, "<launch/>").unwrap();
        let r1 = resolve_self_entry_launch(&p1, None, None).unwrap();
        match r1 {
            LaunchInput::File(path) => assert_eq!(path, target),
            other => panic!("expected File for present launch, got {other:?}"),
        }

        // Branch 2: no launch file → synth.
        let p2 = temp_pkg("l7-synth-launch");
        write_self_entry_cargo(&p2, "beta_pkg", "freertos");
        // No launch dir at all.
        let r2 = resolve_self_entry_launch(&p2, None, None).unwrap();
        match r2 {
            LaunchInput::Synth(body) => {
                assert!(body.contains("pkg=\"beta_pkg\""), "body={body}");
                assert!(body.contains("exec=\"beta_pkg\""), "body={body}");
            }
            other => panic!("expected Synth fallback, got {other:?}"),
        }
    }
}
