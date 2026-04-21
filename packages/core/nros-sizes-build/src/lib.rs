//! Build-script helper for extracting Rust-side type sizes from a compiled rlib.
//!
//! The sibling `nros` crate exports sizes of its internal handle types via
//! `export_size!`, which emits `#[used] static __NROS_SIZE_FOO: [u8; size_of::<Foo>()]`.
//! This crate provides two helpers that consumer build scripts (`nros-c/build.rs`,
//! `nros-cpp/build.rs`) can call to recover those sizes at build time:
//!
//! * [`find_dep_rlib`] — locate the rlib for a direct dependency in the current
//!   cargo build, using `cargo metadata` + a newest-mtime glob.
//! * [`extract_sizes`] — parse an rlib as an `ar` archive and, for every defined
//!   symbol whose name begins with a given prefix, record its storage size.
//!
//! See [Phase 87](../../../../docs/roadmap/phase-87-nros-cpp-compile-time-sizes.md)
//! for the motivating design.

use std::collections::HashMap;
use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

use object::read::archive::ArchiveFile;
use object::{File as ObjectFile, Object, ObjectSymbol};

/// Errors returned by this crate's helpers.
#[derive(Debug)]
pub enum Error {
    /// `cargo metadata` could not be invoked or returned a non-zero exit status.
    CargoMetadata(String),
    /// The metadata JSON was missing an expected field or had the wrong shape.
    MalformedMetadata(&'static str),
    /// No rlib matching `lib<name>-*.rlib` was found in any candidate `deps/` directory.
    RlibNotFound { crate_name: String, searched: Vec<PathBuf> },
    /// I/O error reading a file from disk.
    Io(std::io::Error),
    /// The `object` crate could not parse the rlib or one of its members.
    Object(object::Error),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::CargoMetadata(msg) => write!(f, "cargo metadata failed: {msg}"),
            Error::MalformedMetadata(field) => {
                write!(f, "cargo metadata missing or malformed field: {field}")
            }
            Error::RlibNotFound { crate_name, searched } => {
                write!(
                    f,
                    "no rlib matching lib{crate_name}-*.rlib found; searched: {searched:?}"
                )
            }
            Error::Io(e) => write!(f, "io error: {e}"),
            Error::Object(e) => write!(f, "object parse error: {e}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Io(e)
    }
}

impl From<object::Error> for Error {
    fn from(e: object::Error) -> Self {
        Error::Object(e)
    }
}

/// Locate the newest rlib for `crate_name` produced by the in-progress cargo build.
///
/// Uses `cargo metadata --format-version=1 --no-deps` to find the workspace target
/// directory, then globs `{target}/<triple>/<profile>/deps/lib<crate_name>-*.rlib`
/// (and the no-triple fallback for native builds). Of all candidates, returns the
/// one with the most recent mtime — matching cargo's own incremental build
/// semantics when multiple feature-flag combinations have been compiled.
pub fn find_dep_rlib(crate_name: &str) -> Result<PathBuf, Error> {
    let target_dir = cargo_target_dir()?;
    let triple = env::var("TARGET").ok();
    let profile = env::var("PROFILE").unwrap_or_else(|_| "debug".to_string());

    let mut searched = Vec::new();
    if let Some(triple) = triple.as_deref() {
        searched.push(target_dir.join(triple).join(&profile).join("deps"));
    }
    searched.push(target_dir.join(&profile).join("deps"));

    let mut best: Option<(std::time::SystemTime, PathBuf)> = None;
    let lib_prefix = format!("lib{crate_name}-");
    for dir in &searched {
        let read_dir = match std::fs::read_dir(dir) {
            Ok(r) => r,
            Err(_) => continue,
        };
        for entry in read_dir.flatten() {
            let path = entry.path();
            let Some(fname) = path.file_name().and_then(|s| s.to_str()) else { continue };
            if !fname.starts_with(&lib_prefix) || !fname.ends_with(".rlib") {
                continue;
            }
            let meta = entry.metadata()?;
            let mtime = meta.modified()?;
            match &best {
                Some((best_time, _)) if *best_time >= mtime => {}
                _ => best = Some((mtime, path)),
            }
        }
    }

    best.map(|(_, p)| p).ok_or_else(|| Error::RlibNotFound {
        crate_name: crate_name.to_string(),
        searched,
    })
}

/// Extract the sizes of every defined symbol with the given prefix from an rlib.
///
/// An rlib is an `ar` archive of object files (plus a `lib.rmeta` metadata
/// member). This walks each object member, iterates its defined symbols, and
/// for every symbol whose name starts with `prefix`, records
/// `(name-without-prefix, ObjectSymbol::size())`.
///
/// The pattern used by the `nros` crate is:
///
/// ```ignore
/// #[used]
/// #[unsafe(no_mangle)]
/// pub static __NROS_SIZE_PUBLISHER: [u8; size_of::<RmwPublisher>()] = [0; _];
/// ```
///
/// Calling `extract_sizes(&rlib, "__NROS_SIZE_")` returns `{ "PUBLISHER" → N, ... }`.
pub fn extract_sizes(rlib: &Path, prefix: &str) -> Result<HashMap<String, u64>, Error> {
    let data = std::fs::read(rlib)?;
    let archive = ArchiveFile::parse(&*data)?;
    let mut out: HashMap<String, u64> = HashMap::new();

    for member in archive.members() {
        let member = member?;
        let name_bytes = member.name();
        // Skip rmeta and non-object members. The rustc metadata lands in
        // `lib.rmeta`; some toolchains also include a `__.SYMDEF` bookkeeping
        // member. Anything that isn't a recognised object file is silently
        // skipped.
        if name_bytes == b"lib.rmeta" || name_bytes.starts_with(b"__.SYMDEF") {
            continue;
        }

        let member_data = member.data(&*data)?;
        let object = match ObjectFile::parse(member_data) {
            Ok(o) => o,
            Err(_) => continue,
        };

        for symbol in object.symbols() {
            if !symbol.is_definition() {
                continue;
            }
            let Ok(name) = symbol.name() else { continue };
            if let Some(rest) = name.strip_prefix(prefix) {
                // Several object members may define weak copies of the same
                // static; keep the first non-zero size observed.
                let size = symbol.size();
                if size == 0 {
                    continue;
                }
                out.entry(rest.to_string()).or_insert(size);
            }
        }
    }

    Ok(out)
}

/// Parse `cargo metadata --format-version=1 --no-deps` and return `target_directory`.
fn cargo_target_dir() -> Result<PathBuf, Error> {
    // Respect an explicit override first — keeps downstream builds that set
    // CARGO_TARGET_DIR (e.g. cargo-chef) working without a metadata hop.
    if let Ok(dir) = env::var("CARGO_TARGET_DIR") {
        if !dir.is_empty() {
            return Ok(PathBuf::from(dir));
        }
    }

    let cargo = env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let manifest_dir = env::var("CARGO_MANIFEST_DIR")
        .map_err(|_| Error::MalformedMetadata("CARGO_MANIFEST_DIR"))?;

    let output = Command::new(&cargo)
        .arg("metadata")
        .arg("--format-version=1")
        .arg("--no-deps")
        .current_dir(&manifest_dir)
        .output()
        .map_err(|e| Error::CargoMetadata(e.to_string()))?;

    if !output.status.success() {
        return Err(Error::CargoMetadata(
            String::from_utf8_lossy(&output.stderr).into_owned(),
        ));
    }

    let meta: serde_json::Value = serde_json::from_slice(&output.stdout)
        .map_err(|e| Error::CargoMetadata(format!("invalid JSON: {e}")))?;

    meta.get("target_directory")
        .and_then(|v| v.as_str())
        .map(PathBuf::from)
        .ok_or(Error::MalformedMetadata("target_directory"))
}
