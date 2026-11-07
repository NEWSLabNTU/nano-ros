//! Package / executable discovery from Cargo and CMake metadata.
//!
//! R-code (phase-296 R4): the launch-XML resolution/synthesis that lived
//! here is DELETED — the canonical config path is the play_launch-resolved
//! SystemModel (convention-discovered `config/system_model.yaml`,
//! `--model`, or a pre-baked `--record`). phase-531 W7 step 2 deleted the
//! launchless-package detection and the in-memory 1-node model
//! (~~`synthesise_self_model`~~) that `nros plan <pkg-dir>` planned from. What
//! remains is [`discover_pkg_name`] / [`discover_exec_target`], read by the
//! single-package leaf's launch synthesis (`cmd/ws.rs`), which retires with
//! the leaf road (W7 step 5).

use std::{
    fs,
    path::{Path, PathBuf},
};

use eyre::{Result, eyre};

/// Errors from package / executable discovery. Wrapped in `eyre` at the
/// callsite; declared as a real enum so tests can match on the failure
/// mode without parsing strings.
#[derive(Debug, thiserror::Error)]
pub enum LaunchResolveError {
    #[error(
        "package {pkg} declares multiple executables ({candidates:?}); \
         name the executable explicitly"
    )]
    AmbiguousExec {
        pkg: PathBuf,
        candidates: Vec<String>,
    },

    #[error(
        "could not determine pkg name for {pkg} (no Cargo.toml [package].name, no project() in CMakeLists.txt)"
    )]
    UnknownPkgName { pkg: PathBuf },

    #[error(
        "could not determine executable for {pkg} (no [[bin]] in Cargo.toml, no add_executable in CMakeLists.txt)"
    )]
    UnknownExec { pkg: PathBuf },
}

/// Read `pkg-name`:
/// * Rust: `Cargo.toml` `[package].name`
/// * C++: `project(<name> …)` in `CMakeLists.txt`
pub fn discover_pkg_name(pkg_dir: &Path) -> Result<String> {
    let cargo = pkg_dir.join("Cargo.toml");
    if cargo.is_file()
        && let Some(name) = read_cargo_pkg_name(&cargo)?
    {
        return Ok(name);
    }
    let cmake = pkg_dir.join("CMakeLists.txt");
    if cmake.is_file()
        && let Some(name) = read_cmake_project_name(&cmake)?
    {
        return Ok(name);
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

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_pkg(name: &str) -> PathBuf {
        crate::test_support::scratch_dir(&format!("nros-launch-synth-{name}"))
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
}
