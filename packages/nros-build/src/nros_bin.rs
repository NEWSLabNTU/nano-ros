//! Resolve the `nros` CLI binary.
//!
//! Resolution order (mirrors `scripts/build/cargo.sh::nros_cli_bin`,
//! but uses `$NROS_BIN` per the Phase 212.C spec):
//!   1. `$NROS_BIN` (must exist + be executable)
//!   2. `nros` on `$PATH`
//!   3. `~/.nros/bin/nros` (default `$NROS_HOME/bin/nros`)
//!
//! Missing → hard fail with an install pointer.

use std::{env, path::PathBuf};

/// Error returned when the `nros` binary cannot be resolved.
#[derive(Debug)]
pub struct MissingNrosBinary {
    pub tried: Vec<String>,
}

impl std::fmt::Display for MissingNrosBinary {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(
            f,
            "nros CLI not found. Tried: {}.\n\
             Install it via `scripts/install-nros.sh` (or `just setup base`),\n\
             or set `NROS_BIN=/path/to/nros`.\n\
             Source: https://github.com/NEWSLabNTU/nros-cli",
            self.tried.join(", ")
        )
    }
}

impl std::error::Error for MissingNrosBinary {}

/// Locate the `nros` binary. See module-level doc for resolution order.
pub fn find_nros_binary() -> Result<PathBuf, MissingNrosBinary> {
    let mut tried = Vec::new();

    if let Ok(p) = env::var("NROS_BIN") {
        let path = PathBuf::from(&p);
        if is_executable(&path) {
            return Ok(path);
        }
        tried.push(format!("$NROS_BIN={p}"));
    }

    if let Some(p) = which_on_path("nros") {
        return Ok(p);
    }
    tried.push("PATH".into());

    let home_root = env::var("NROS_HOME").ok().map(PathBuf::from).or_else(|| {
        env::var("HOME")
            .ok()
            .map(|h| PathBuf::from(h).join(".nros"))
    });
    if let Some(root) = home_root {
        let candidate = root.join("bin").join("nros");
        if is_executable(&candidate) {
            return Ok(candidate);
        }
        tried.push(format!("{}", candidate.display()));
    }

    Err(MissingNrosBinary { tried })
}

fn is_executable(p: &std::path::Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        match std::fs::metadata(p) {
            Ok(m) => m.is_file() && (m.permissions().mode() & 0o111) != 0,
            Err(_) => false,
        }
    }
    #[cfg(not(unix))]
    {
        p.is_file()
    }
}

fn which_on_path(name: &str) -> Option<PathBuf> {
    let path = env::var_os("PATH")?;
    for dir in env::split_paths(&path) {
        let candidate = dir.join(name);
        if is_executable(&candidate) {
            return Some(candidate);
        }
    }
    None
}
