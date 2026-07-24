//! Self-bringup pkg discovery + 1-node model synthesis.
//!
//! R-code (phase-296 R4): the launch-XML resolution/synthesis that lived
//! here is DELETED — the canonical config path is the play_launch-resolved
//! SystemModel (convention-discovered `config/system_model.yaml`,
//! `--model`, or a pre-baked `--record`). What remains is the self-bringup
//! kernel: pkg/exec discovery from Cargo/CMake metadata, launch-file
//! enumeration (used to detect "launchless" pkgs), and
//! [`synthesise_self_model`] — the in-memory 1-node SystemModel that
//! replaces the old 1-node `<launch>` XML synth for the L.7 dev loop.

use std::{
    fs,
    path::{Path, PathBuf},
};

use eyre::{Result, eyre};

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

/// True when `pkg_dir` is a component or application pkg eligible for
/// synthesis: has a `Cargo.toml` or `CMakeLists.txt`. Path A bringup
/// pkgs (no Cargo.toml + no CMakeLists.txt + has system.toml) are
/// disqualified.
pub fn is_self_bringup_eligible(pkg_dir: &Path) -> bool {
    let has_cargo = pkg_dir.join("Cargo.toml").is_file();
    let has_cmake = pkg_dir.join("CMakeLists.txt").is_file();
    has_cargo || has_cmake
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
/// R-code precondition #3 — the 1-node model synth for the self-bringup
/// shape (same
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
}
