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

use std::{
    collections::HashMap,
    env,
    path::{Path, PathBuf},
    process::Command,
};

use object::{File as ObjectFile, Object, ObjectSymbol, read::archive::ArchiveFile};

/// Errors returned by this crate's helpers.
#[derive(Debug)]
pub enum Error {
    /// `cargo metadata` could not be invoked or returned a non-zero exit status.
    CargoMetadata(String),
    /// The metadata JSON was missing an expected field or had the wrong shape.
    MalformedMetadata(&'static str),
    /// No rlib matching `lib<name>-*.rlib` was found in any candidate `deps/` directory.
    RlibNotFound {
        crate_name: String,
        searched: Vec<PathBuf>,
    },
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
            Error::RlibNotFound {
                crate_name,
                searched,
            } => {
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

/// Locate the rlib for `crate_name` that contains Phase 87's size-probe
/// symbols (any defined symbol starting with `symbol_prefix`), falling back
/// to the newest matching rlib if none do.
///
/// Uses `cargo metadata --format-version=1 --no-deps` to find the workspace
/// target directory, then globs `{target}/<triple>/<profile>/deps/lib<crate_name>-*.rlib`
/// (plus the no-triple fallback for native builds) and ranks the matches:
///
/// 1. Among rlibs that contain at least one symbol starting with
///    `symbol_prefix`, pick the newest by mtime. This is the primary path —
///    a cargo build with `--features rmw-zenoh` (etc.) produces an rlib
///    that emits the `__NROS_SIZE_*` statics.
/// 2. If no rlib contains any probe symbol, fall back to the newest rlib
///    of any flavour. The caller's `extract_sizes` will then return an
///    empty map and emit a `cargo:warning=`. Build proceeds without probe
///    values (useful for `cargo check` without feature flags).
pub fn find_dep_rlib(crate_name: &str, symbol_prefix: &str) -> Result<PathBuf, Error> {
    let target_dir = cargo_target_dir()?;
    let triple = env::var("TARGET").ok();
    let host = env::var("HOST").ok();
    let profile = env::var("PROFILE").unwrap_or_else(|_| "debug".to_string());

    // Phase 77.25: under cross-compile, the target-triple rlib may not
    // exist yet when this build script runs (regular deps are compiled
    // in parallel with build scripts), but the host-triple rlib almost
    // certainly does (it's a build-dep — see nros-c / nros-cpp
    // Cargo.toml). We MUST NOT read host-rlib sizes when the
    // downstream consumer is building for a different target, because
    // pointer-size-dependent structs (e.g. anything holding *mut)
    // would give wrong values. Only search the host deps dir when
    // target == host.
    let mut searched = Vec::new();
    if let Some(triple) = triple.as_deref() {
        searched.push(target_dir.join(triple).join(&profile).join("deps"));
    }
    if triple.as_deref() == host.as_deref() {
        searched.push(target_dir.join(&profile).join("deps"));
    }

    let mut candidates: Vec<(std::time::SystemTime, PathBuf)> = Vec::new();
    let lib_prefix = format!("lib{crate_name}-");
    for dir in &searched {
        let read_dir = match std::fs::read_dir(dir) {
            Ok(r) => r,
            Err(_) => continue,
        };
        for entry in read_dir.flatten() {
            let path = entry.path();
            let Some(fname) = path.file_name().and_then(|s| s.to_str()) else {
                continue;
            };
            if !fname.starts_with(&lib_prefix) || !fname.ends_with(".rlib") {
                continue;
            }
            let meta = entry.metadata()?;
            candidates.push((meta.modified()?, path));
        }
    }

    candidates.sort_by_key(|(mtime, _)| std::cmp::Reverse(*mtime));

    for (_, path) in &candidates {
        if let Ok(sizes) = extract_sizes(path, symbol_prefix)
            && !sizes.is_empty()
        {
            return Ok(path.clone());
        }
    }

    candidates
        .into_iter()
        .next()
        .map(|(_, p)| p)
        .ok_or_else(|| Error::RlibNotFound {
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
    let mut saw_bitcode = false;

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
            Err(_) => {
                // Fat LTO makes rustc emit LLVM bitcode (`BC\xC0\xDE` or
                // `\xDE\xC0\x17\x0B` Mach-O embedded) instead of ELF/COFF
                // objects. `object` can't read bitcode. Flag for the v0
                // fallback below.
                if member_data.starts_with(b"BC\xC0\xDE") {
                    saw_bitcode = true;
                }
                continue;
            }
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

    // Phase 77.25: if nothing came out of the ELF path and the rlib
    // contains bitcode members, fall back to rustc's bundled `llvm-nm`
    // which *can* read bitcode symbol names. The nros sizes module
    // emits `__nros_size_NAME<const N: usize>` monomorphisations —
    // their v0-mangled symbol names contain both the NAME and the
    // const-generic value N (the `size_of::<T>()` result). A single
    // regex captures `NAME` and `N` from the demangled output.
    if out.is_empty()
        && saw_bitcode
        && let Ok(from_bitcode) = extract_sizes_via_llvm_nm(rlib)
    {
        return Ok(from_bitcode);
    }

    Ok(out)
}

/// Phase 77.25: bitcode-aware extraction via `rustc`-bundled `llvm-nm`.
///
/// Invokes `$(rustc --print sysroot)/lib/rustlib/$TRIPLE/bin/llvm-nm
/// --demangle` against the rlib, then regex-matches lines like
/// `nros::sizes::rmw_sizes::__nros_size_PUBLISHER_SIZE::<48>` — the
/// capture groups are the NAME and the const-generic SIZE value.
/// Returns an empty map on any failure (probe consumers treat that
/// the same as a probe miss — 77.24's stopgap covers it).
fn extract_sizes_via_llvm_nm(rlib: &Path) -> Result<HashMap<String, u64>, Error> {
    let sysroot = rustc_sysroot()?;
    let triple = rustc_host_triple()?;
    let llvm_nm = sysroot
        .join("lib/rustlib")
        .join(&triple)
        .join("bin/llvm-nm");
    if !llvm_nm.exists() {
        return Err(Error::CargoMetadata(format!(
            "llvm-nm not found at {}",
            llvm_nm.display()
        )));
    }

    let output = Command::new(&llvm_nm)
        .arg("--demangle")
        .arg(rlib)
        .output()
        .map_err(|e| Error::CargoMetadata(e.to_string()))?;
    if !output.status.success() {
        return Err(Error::CargoMetadata(
            String::from_utf8_lossy(&output.stderr).into_owned(),
        ));
    }
    let text = String::from_utf8_lossy(&output.stdout);

    // Match `::__nros_size_<NAME>::<<SIZE>>` near the end of a line.
    // Example: "nros::sizes::rmw_sizes::__nros_size_PUBLISHER_SIZE::<48>"
    let mut out = HashMap::new();
    for line in text.lines() {
        let Some(marker_idx) = line.find("::__nros_size_") else {
            continue;
        };
        let after = &line[marker_idx + "::__nros_size_".len()..];
        // `after` now looks like "PUBLISHER_SIZE::<48>"
        let Some((name, tail)) = after.split_once("::<") else {
            continue;
        };
        let Some(num_str) = tail.strip_suffix('>') else {
            continue;
        };
        let Ok(size) = num_str.trim().parse::<u64>() else {
            continue;
        };
        out.entry(name.to_string()).or_insert(size);
    }
    Ok(out)
}

fn rustc_sysroot() -> Result<PathBuf, Error> {
    let output = Command::new(env::var_os("RUSTC").unwrap_or_else(|| "rustc".into()))
        .args(["--print", "sysroot"])
        .output()
        .map_err(|e| Error::CargoMetadata(e.to_string()))?;
    if !output.status.success() {
        return Err(Error::CargoMetadata(
            String::from_utf8_lossy(&output.stderr).into_owned(),
        ));
    }
    Ok(PathBuf::from(
        String::from_utf8_lossy(&output.stdout).trim(),
    ))
}

fn rustc_host_triple() -> Result<String, Error> {
    // Prefer the triple the current build is compiling *for*
    // (matters for cross-compile); fall back to rustc -vV.
    if let Ok(t) = env::var("TARGET") {
        return Ok(t);
    }
    let output = Command::new(env::var_os("RUSTC").unwrap_or_else(|| "rustc".into()))
        .arg("-vV")
        .output()
        .map_err(|e| Error::CargoMetadata(e.to_string()))?;
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        if let Some(rest) = line.strip_prefix("host: ") {
            return Ok(rest.trim().to_string());
        }
    }
    Err(Error::CargoMetadata(
        "could not determine rustc host triple".into(),
    ))
}

/// Parse `cargo metadata --format-version=1 --no-deps` and return `target_directory`.
fn cargo_target_dir() -> Result<PathBuf, Error> {
    // Respect an explicit override first — keeps downstream builds that set
    // CARGO_TARGET_DIR (e.g. cargo-chef) working without a metadata hop.
    if let Ok(dir) = env::var("CARGO_TARGET_DIR")
        && !dir.is_empty()
    {
        return Ok(PathBuf::from(dir));
    }

    // Corrosion (CMake) invokes cargo with `--target-dir <custom>` which
    // doesn't export `CARGO_TARGET_DIR`, and `cargo metadata` returns the
    // workspace default rather than the active `--target-dir`. Derive the
    // real target dir from `OUT_DIR`, which cargo sets to
    // `<target>/<triple>/<profile>/build/<pkg>-<hash>/out` for every build
    // script. `ancestors().nth(5)` steps back through out → <pkg-hash> →
    // build → <profile> → <triple> → <target>.
    if let Ok(out) = env::var("OUT_DIR") {
        let out = PathBuf::from(out);
        if let Some(target) = out.ancestors().nth(5)
            && target.join("CACHEDIR.TAG").exists()
        {
            return Ok(target.to_path_buf());
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
