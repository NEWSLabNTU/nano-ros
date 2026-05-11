//! Build-script helper for extracting Rust-side type sizes from a compiled rlib.
//!
//! The sibling `nros` crate exports sizes of its internal handle types via
//! `export_size!`, which emits `#[used] static __NROS_SIZE_FOO: [u8; size_of::<Foo>()]`.
//! This crate provides two helpers that consumer build scripts (`nros-c/build.rs`,
//! `nros-cpp/build.rs`) can call to recover those sizes at build time:
//!
//! * [`find_dep_rlib`] — locate the rlib for a direct dependency in the current
//!   cargo build.
//! * [`extract_sizes`] — parse an rlib as an `ar` archive and, for every defined
//!   symbol whose name begins with a given prefix, record its storage size.
//!
//! See [Phase 87](../../../../docs/roadmap/phase-87-nros-cpp-compile-time-sizes.md)
//! for the motivating design; [Phase 118.E](../../../../docs/roadmap/phase-118-E-size-probe-rigorization.md)
//! for the current race-hardening approach.
//!
//! # Probe modes
//!
//! Selected via the `NROS_SIZES_PROBE_MODE` env var:
//!
//! * `filesystem` (default) — use `cargo metadata` to locate the workspace
//!   target dir deterministically, then watch
//!   `<target>/<triple>/<profile>/deps/` for the rlib, with bounded retry. This
//!   relies on the outer cargo's build-graph ordering (which guarantees `nros`
//!   lib compile completes before `nros-c` lib compile) but races with the
//!   parallel scheduler at build-script time.
//!
//! * `isolated` — spawn a nested `cargo build -p <crate> --target=<triple>
//!   --message-format=json` against a probe-only target dir
//!   (`$OUT_DIR/sizes-probe-target`). Deterministic — the artifact event
//!   reports the canonical rlib path on completion. Costs a duplicate
//!   compile of the probed crate on cold cache; subsequent runs hit the
//!   probe cache and are sub-second. Useful for strict-determinism builds
//!   (CI, release tagging) where the retry-based filesystem path's tail
//!   latency is unacceptable.
//!
//! # Timeouts
//!
//! `NROS_SIZES_PROBE_TIMEOUT_SECS` (default `60`) bounds the filesystem-mode
//! wait. Set higher on cold-cache CI runners that compile `nros` from
//! scratch in parallel; set lower for fast-fail in interactive development.

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

/// Probe-strategy selector. See module docs for the env-var contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProbeMode {
    /// Watch `<target>/<triple>/<profile>/deps/` for the rlib emitted by
    /// the outer cargo build. Fast on a warm cache; races against the
    /// outer cargo's parallel scheduler so requires bounded retry.
    Filesystem,
    /// Spawn a nested `cargo build -p <crate>` against
    /// `$OUT_DIR/sizes-probe-target/` and parse the JSON artifact event.
    /// Deterministic; pays one extra compile per (target, features) on a
    /// cold cache.
    Isolated,
}

impl ProbeMode {
    /// Read `NROS_SIZES_PROBE_MODE` (case-insensitive) — `isolated` selects
    /// the nested-cargo path; anything else (including unset) selects the
    /// filesystem path.
    pub fn from_env() -> Self {
        match env::var("NROS_SIZES_PROBE_MODE").as_deref().map(str::trim) {
            Ok(s) if s.eq_ignore_ascii_case("isolated") => Self::Isolated,
            _ => Self::Filesystem,
        }
    }
}

/// Locate the rlib for `crate_name` that contains Phase 87's size-probe
/// symbols (any defined symbol starting with `symbol_prefix`).
///
/// Dispatches on [`ProbeMode::from_env`]. See module docs for the two paths.
pub fn find_dep_rlib(crate_name: &str, symbol_prefix: &str) -> Result<PathBuf, Error> {
    match ProbeMode::from_env() {
        ProbeMode::Filesystem => find_dep_rlib_filesystem(crate_name, symbol_prefix),
        ProbeMode::Isolated => find_dep_rlib_isolated(crate_name, symbol_prefix),
    }
}

/// Default filesystem-watch path. Uses `cargo metadata` to resolve the
/// workspace target dir, then waits for the rlib to appear at
/// `<target>/<triple>/<profile>/deps/lib<crate>-*.rlib`, polling at 200 ms
/// ticks. Timeout configurable via `NROS_SIZES_PROBE_TIMEOUT_SECS`
/// (default 60s). After the rlib appears, retries `extract_sizes` for up
/// to 5s for the symbol table to flush.
fn find_dep_rlib_filesystem(crate_name: &str, symbol_prefix: &str) -> Result<PathBuf, Error> {
    let target_dir = cargo_target_dir()?;
    let triple = env::var("TARGET").ok();
    let host = env::var("HOST").ok();
    let profile = env::var("PROFILE").unwrap_or_else(|_| "debug".to_string());

    // Phase 77.25: under cross-compile, only the target-triple rlib has
    // correct pointer-size-dependent sizes. The host-triple rlib (a
    // build-dep artefact) must NOT be read when target != host.
    let mut searched = Vec::new();
    if let Some(triple) = triple.as_deref() {
        searched.push(target_dir.join(triple).join(&profile).join("deps"));
    }
    if triple.as_deref() == host.as_deref() {
        searched.push(target_dir.join(&profile).join("deps"));
    }

    let timeout_secs = env::var("NROS_SIZES_PROBE_TIMEOUT_SECS")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(60);
    let timeout = std::time::Duration::from_secs(timeout_secs);
    let lib_prefix = format!("lib{crate_name}-");

    let mut candidates: Vec<(std::time::SystemTime, PathBuf)> = Vec::new();
    let start = std::time::Instant::now();
    let mut last_progress = std::time::Instant::now();

    loop {
        candidates.clear();
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
                if let Ok(meta) = entry.metadata() {
                    candidates.push((
                        meta.modified().unwrap_or(std::time::SystemTime::UNIX_EPOCH),
                        path,
                    ));
                }
            }
        }

        if !candidates.is_empty() || start.elapsed() >= timeout {
            break;
        }
        if last_progress.elapsed() >= std::time::Duration::from_secs(10) {
            println!(
                "cargo:warning=nros-sizes-build: waiting for lib{crate_name}-*.rlib \
                 (elapsed {}s of {}s timeout) — outer cargo still building?",
                start.elapsed().as_secs(),
                timeout_secs
            );
            last_progress = std::time::Instant::now();
        }
        std::thread::sleep(std::time::Duration::from_millis(200));
    }

    candidates.sort_by_key(|(mtime, _)| std::cmp::Reverse(*mtime));

    let probe_start = std::time::Instant::now();
    let probe_timeout = std::time::Duration::from_secs(5);
    loop {
        for (_, path) in &candidates {
            if let Ok(sizes) = extract_sizes(path, symbol_prefix)
                && !sizes.is_empty()
            {
                return Ok(path.clone());
            }
        }
        if probe_start.elapsed() >= probe_timeout {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(200));
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

/// Phase 118.E: deterministic nested-cargo probe.
///
/// Spawns `cargo build -p <crate> --target=<triple> [--release]
/// --message-format=json` against a probe-only target dir to avoid
/// flock contention with the outer cargo (cargo holds an exclusive
/// flock on the target dir for the entire build; nested invocations
/// against the same dir deadlock). Parses `compiler-artifact` events
/// for the requested crate name; returns the first rlib filename.
///
/// Probe target dir defaults to `$OUT_DIR/sizes-probe-target` so it
/// lives alongside the consumer's build artefacts and cleans up with
/// `cargo clean`. Override via `NROS_SIZES_PROBE_TARGET_DIR`.
fn find_dep_rlib_isolated(crate_name: &str, symbol_prefix: &str) -> Result<PathBuf, Error> {
    let cargo = env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let target = env::var("TARGET").map_err(|_| Error::MalformedMetadata("TARGET"))?;
    let profile = env::var("PROFILE").unwrap_or_else(|_| "debug".to_string());

    let probe_target_dir = if let Ok(dir) = env::var("NROS_SIZES_PROBE_TARGET_DIR") {
        PathBuf::from(dir)
    } else {
        let out_dir = env::var("OUT_DIR").map_err(|_| Error::MalformedMetadata("OUT_DIR"))?;
        PathBuf::from(out_dir).join("sizes-probe-target")
    };

    let mut cmd = Command::new(&cargo);
    cmd.env("CARGO_TARGET_DIR", &probe_target_dir)
        .arg("build")
        .arg("-p")
        .arg(crate_name)
        .arg("--target")
        .arg(&target)
        .arg("--message-format=json-render-diagnostics");
    if profile == "release" {
        cmd.arg("--release");
    }

    // Forward the consumer's active feature set. The build script's
    // `CARGO_FEATURE_<NAME>` env vars describe the consumer crate's
    // features, not the probed crate's — but downstream forwarding via
    // a `--features` arg works if the names match. Consumer crates
    // (`nros-c`, `nros-cpp`) re-export `nros`'s RMW backend features
    // by the same names, so this is a safe identity forward.
    let forwarded = forwarded_features();
    if !forwarded.is_empty() {
        cmd.arg("--features").arg(forwarded.join(","));
    }

    let output = cmd
        .output()
        .map_err(|e| Error::CargoMetadata(e.to_string()))?;
    if !output.status.success() {
        return Err(Error::CargoMetadata(format!(
            "nested cargo build failed: {}",
            String::from_utf8_lossy(&output.stderr)
        )));
    }

    for line in output.stdout.split(|&b| b == b'\n') {
        if line.is_empty() {
            continue;
        }
        let Ok(json) = serde_json::from_slice::<serde_json::Value>(line) else {
            continue;
        };
        if json.get("reason").and_then(|r| r.as_str()) != Some("compiler-artifact") {
            continue;
        }
        let Some(target_obj) = json.get("target") else {
            continue;
        };
        if target_obj.get("name").and_then(|n| n.as_str()) != Some(crate_name) {
            continue;
        }
        let Some(filenames) = json.get("filenames").and_then(|f| f.as_array()) else {
            continue;
        };
        for fname in filenames {
            let Some(s) = fname.as_str() else { continue };
            if s.ends_with(".rlib") {
                let path = PathBuf::from(s);
                // Validate symbols are present before returning. If the
                // rlib was compiled without RMW features (e.g. workspace
                // default `cargo check`), the probe symbols won't exist
                // and the consumer should fall through to its
                // `unwrap_or(0)` path.
                if let Ok(sizes) = extract_sizes(&path, symbol_prefix)
                    && !sizes.is_empty()
                {
                    return Ok(path);
                }
                // Symbol-less rlib — still return the path so callers
                // can emit a warning and fall back.
                return Ok(path);
            }
        }
    }

    Err(Error::RlibNotFound {
        crate_name: crate_name.to_string(),
        searched: vec![probe_target_dir],
    })
}

/// Collect feature names the consumer build script was invoked with.
///
/// Cargo exposes them as `CARGO_FEATURE_<NAME>=1` env vars with name
/// upper-cased and `-` replaced by `_`. Reverse the transform so the
/// nested invocation sees the original lowercase-with-dashes form.
fn forwarded_features() -> Vec<String> {
    let mut out = Vec::new();
    for (k, v) in env::vars() {
        if v != "1" {
            continue;
        }
        let Some(rest) = k.strip_prefix("CARGO_FEATURE_") else {
            continue;
        };
        out.push(rest.to_ascii_lowercase().replace('_', "-"));
    }
    out
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
    let exe_suffix = if cfg!(windows) { ".exe" } else { "" };
    let llvm_nm = sysroot
        .join("lib/rustlib")
        .join(&triple)
        .join("bin")
        .join(format!("llvm-nm{exe_suffix}"));
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

/// Resolve the rustc *host* triple (the triple of the toolchain itself).
///
/// `llvm-nm` is bundled at `<sysroot>/lib/rustlib/<host>/bin/`; cross-target
/// directories don't carry the host tools, so this must return the host
/// triple, not the build's `TARGET`. Phase 118.E fixes the prior behavior
/// which preferred `TARGET` and broke on cross-builds.
fn rustc_host_triple() -> Result<String, Error> {
    let output = Command::new(env::var_os("RUSTC").unwrap_or_else(|| "rustc".into()))
        .arg("-vV")
        .output()
        .map_err(|e| Error::CargoMetadata(e.to_string()))?;
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        if let Some(rest) = line.strip_prefix("host: ") {
            return Ok(rest.trim().to_string());
        }
    }
    // Last-resort fallback: cargo's `HOST` env (set in build scripts).
    if let Ok(t) = env::var("HOST") {
        return Ok(t);
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
    // `<target>/[triple]/<profile>/build/<pkg>-<hash>/out` for every build
    // script.
    if let Ok(out) = env::var("OUT_DIR") {
        let out = PathBuf::from(out);
        let mut p = out.as_path();
        while let Some(parent) = p.parent() {
            if parent.file_name().and_then(|s| s.to_str()) == Some("build")
                && let Some(profile_dir) = parent.parent()
                && let Some(triple_or_target) = profile_dir.parent()
                && let Some(name) = triple_or_target.file_name().and_then(|s| s.to_str())
            {
                // Structure above "build" is .../<target>/[triple]/<profile>/build.
                // If the dir name looks like a target triple (contains '-'),
                // the target directory is one level higher; otherwise this IS
                // the target directory.
                if name.contains('-') {
                    if let Some(target) = triple_or_target.parent() {
                        return Ok(target.to_path_buf());
                    }
                } else {
                    return Ok(triple_or_target.to_path_buf());
                }
            }
            p = parent;
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Each test mutates process-global env state, so they share a mutex to
    /// avoid clobbering each other under `cargo test`'s default parallelism.
    fn env_lock() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
        LOCK.lock().unwrap_or_else(|e| e.into_inner())
    }

    #[test]
    fn probe_mode_default_is_filesystem() {
        let _g = env_lock();
        // SAFETY: protected by env_lock; no concurrent reads/writes
        unsafe {
            env::remove_var("NROS_SIZES_PROBE_MODE");
        }
        assert_eq!(ProbeMode::from_env(), ProbeMode::Filesystem);
    }

    #[test]
    fn probe_mode_isolated_via_env() {
        let _g = env_lock();
        unsafe {
            env::set_var("NROS_SIZES_PROBE_MODE", "isolated");
        }
        assert_eq!(ProbeMode::from_env(), ProbeMode::Isolated);
        unsafe {
            env::set_var("NROS_SIZES_PROBE_MODE", "ISOLATED");
        }
        assert_eq!(ProbeMode::from_env(), ProbeMode::Isolated);
        unsafe {
            env::remove_var("NROS_SIZES_PROBE_MODE");
        }
    }

    #[test]
    fn probe_mode_unknown_value_falls_back_to_filesystem() {
        let _g = env_lock();
        unsafe {
            env::set_var("NROS_SIZES_PROBE_MODE", "garbage");
        }
        assert_eq!(ProbeMode::from_env(), ProbeMode::Filesystem);
        unsafe {
            env::remove_var("NROS_SIZES_PROBE_MODE");
        }
    }

    #[test]
    fn forwarded_features_reverses_cargo_feature_transform() {
        let _g = env_lock();
        // Clear any pre-existing CARGO_FEATURE_* the test runner may have set.
        let prior: Vec<String> = env::vars()
            .filter(|(k, _)| k.starts_with("CARGO_FEATURE_"))
            .map(|(k, _)| k)
            .collect();
        for k in &prior {
            unsafe {
                env::remove_var(k);
            }
        }
        unsafe {
            env::set_var("CARGO_FEATURE_RMW_ZENOH", "1");
            env::set_var("CARGO_FEATURE_PLATFORM_POSIX", "1");
            env::set_var("CARGO_FEATURE_SOMETHING_ELSE", "0"); // value != "1" → filtered
        }
        let mut got = forwarded_features();
        got.sort();
        assert_eq!(got, vec!["platform-posix", "rmw-zenoh"]);
        unsafe {
            env::remove_var("CARGO_FEATURE_RMW_ZENOH");
            env::remove_var("CARGO_FEATURE_PLATFORM_POSIX");
            env::remove_var("CARGO_FEATURE_SOMETHING_ELSE");
        }
    }
}
