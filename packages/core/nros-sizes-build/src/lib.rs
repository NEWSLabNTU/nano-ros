//! Build-script helper for extracting Rust-side type sizes from a compiled rlib.
//!
//! The sibling `nros` crate exports sizes of its internal handle types via
//! `export_size!`, which emits `#[used] static __NROS_SIZE_FOO: [u8; size_of::<Foo>()]`.
//! This crate provides two helpers that consumer build scripts (`nros-c/build.rs`,
//! `nros-cpp/build.rs`) can call to recover those sizes at build time:
//!
//! * [`find_dep_rlib`] — locate the rlib for a direct dependency by spawning
//!   a nested `cargo build --message-format=json` and parsing the artifact event.
//! * [`extract_sizes`] — parse an rlib as an `ar` archive and, for every defined
//!   symbol whose name begins with a given prefix, record its storage size.
//!
//! See [Phase 87](../../../../docs/roadmap/phase-87-nros-cpp-compile-time-sizes.md)
//! for the motivating design; [Phase 118.E](../../../../docs/roadmap/phase-118-E-size-probe-rigorization.md)
//! for the race-hardening rewrite.
//!
//! # Probe mechanism
//!
//! Two layered paths. The first that succeeds wins.
//!
//! 1. **Isolated nested cargo (primary).** Spawns `cargo build -p <crate>
//!    --target=<triple> --no-default-features --features=<resolved>
//!    --message-format=json` against a probe-only target dir
//!    (`$OUT_DIR/sizes-probe-target-<rustc-slug>/` by default; override
//!    via `NROS_SIZES_PROBE_TARGET_DIR`). The probe-only target dir
//!    sidesteps the outer cargo's exclusive flock on its target dir —
//!    same-dir nested invocations deadlock because cargo holds the lock
//!    for the entire outer build, including time waiting on build-script
//!    subprocesses. The `compiler-artifact` JSON event reports the
//!    canonical rlib path deterministically on completion. Cost: one
//!    duplicate compile of the probed crate per (target, features) on a
//!    cold probe cache; warm-cache reruns are sub-second.
//!
//! 2. **Filesystem watch (fallback).** Used when the isolated path can't
//!    reproduce the outer build's environment — typically custom-target
//!    JSON specs (`armv7a-nuttx-eabihf`) that need `[unstable] build-std`
//!    configs which don't propagate cleanly across `CARGO_TARGET_DIR`
//!    boundaries. Watches `<target>/<triple>/<profile>/deps/` for the
//!    rlib produced by the outer build, with a configurable timeout
//!    (`NROS_SIZES_PROBE_TIMEOUT_SECS`, default 60s).
//!
//! Force the fallback path with `NROS_SIZES_PROBE_MODE=filesystem` (e.g.
//! when nested cargo is undesirable on slow filesystems / CI runners
//! where the extra compile cost outweighs the determinism benefit).
//!
//! # Corrosion / cross-toolchain compatibility
//!
//! Cross-build env (corrosion-driven CMake, etc.) leaks target-side
//! `RUSTFLAGS` into every rustc invocation, which breaks host-side
//! proc-macro compiles inside the nested cargo. The probe strips
//! `RUSTFLAGS`, `CARGO_BUILD_RUSTFLAGS`, `CARGO_ENCODED_RUSTFLAGS`,
//! `CARGO_BUILD_TARGET`, and `CARGO_BUILD_TARGET_DIR` from the nested
//! env. Safe because:
//!
//! * Rlibs don't link → link-args don't apply.
//! * `size_of::<T>()` depends on the target *triple*'s data layout, not on
//!   `-C target-cpu` / `-C target-feature` (those affect codegen, not layout).
//!
//! `--no-default-features` is mandatory on the nested invocation: most
//! nros crates default to `std`, which would auto-link on bare-metal
//! targets and fail. The explicit `--features` arg below restores
//! whatever the consumer activated (including `std` when target=host).
//!
//! When even the env scrubbing isn't enough (e.g. custom-target JSON
//! specs that don't resolve in the nested invocation), the filesystem
//! fallback takes over.

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

/// Locate the rlib for `crate_name` containing Phase 87's size-probe
/// symbols (any defined symbol starting with `symbol_prefix`).
///
/// Tries the deterministic nested-cargo path first; on failure falls
/// back to watching the outer cargo's target dir for the rlib (the
/// pre-118.E behaviour, retained for cases where nested cargo can't
/// reproduce the build environment — typically custom-target JSON
/// specs with `[unstable] build-std` configs that don't propagate
/// across `CARGO_TARGET_DIR` boundaries).
///
/// Disable the isolated path entirely with
/// `NROS_SIZES_PROBE_MODE=filesystem`.
pub fn find_dep_rlib(crate_name: &str, symbol_prefix: &str) -> Result<PathBuf, Error> {
    let force_fs = env::var("NROS_SIZES_PROBE_MODE")
        .ok()
        .is_some_and(|s| s.eq_ignore_ascii_case("filesystem"));
    if !force_fs {
        match find_dep_rlib_isolated(crate_name, symbol_prefix) {
            Ok(p) => return Ok(p),
            Err(e) => {
                println!(
                    "cargo:warning=nros-sizes-build: isolated probe failed; \
                     falling back to filesystem watch. cause: {e}"
                );
            }
        }
    }
    find_dep_rlib_filesystem(crate_name, symbol_prefix)
}

/// Filesystem-watch fallback path. Used when the nested-cargo probe
/// fails (custom-target JSON specs, `build-std` configs, etc.) or
/// when `NROS_SIZES_PROBE_MODE=filesystem` is set explicitly.
///
/// Resolves the workspace target dir via `cargo metadata` (or
/// `CARGO_TARGET_DIR` / OUT_DIR walking) and polls
/// `<target>/<triple>/<profile>/deps/` for the rlib. Timeout via
/// `NROS_SIZES_PROBE_TIMEOUT_SECS` (default 60s).
fn find_dep_rlib_filesystem(crate_name: &str, symbol_prefix: &str) -> Result<PathBuf, Error> {
    let target_dir = cargo_target_dir()?;
    let triple = env::var("TARGET").ok();
    let host = env::var("HOST").ok();
    let profile = env::var("PROFILE").unwrap_or_else(|_| "debug".to_string());

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
                 (elapsed {}s of {}s timeout)",
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

fn find_dep_rlib_isolated(crate_name: &str, symbol_prefix: &str) -> Result<PathBuf, Error> {
    let cargo = env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let target = env::var("TARGET").map_err(|_| Error::MalformedMetadata("TARGET"))?;
    let profile = env::var("PROFILE").unwrap_or_else(|_| "debug".to_string());

    // Phase 118.E.2 (rustc isolation): include a slug derived from
    // `rustc -V` in the probe target dir name. Without it, switching
    // toolchains (e.g. rustup nightly → stable, or a corrosion build
    // that overrides CARGO_BUILD_RUSTC) leaves rmeta files in the
    // probe dir compiled by the previous rustc; the next cargo run
    // explodes with E0514 "found crate `X` compiled by an incompatible
    // version of rustc" instead of recompiling from scratch.
    let probe_target_dir = if let Ok(dir) = env::var("NROS_SIZES_PROBE_TARGET_DIR") {
        PathBuf::from(dir)
    } else {
        let out_dir = env::var("OUT_DIR").map_err(|_| Error::MalformedMetadata("OUT_DIR"))?;
        let rustc_slug = rustc_version_slug();
        PathBuf::from(out_dir).join(format!("sizes-probe-target-{rustc_slug}"))
    };

    let mut cmd = Command::new(&cargo);
    cmd.env("CARGO_TARGET_DIR", &probe_target_dir)
        .arg("build")
        .arg("-p")
        .arg(crate_name)
        .arg("--target")
        .arg(&target)
        // Phase 118.E.2: must disable default features so the nested
        // invocation matches the outer's intent. Most nros crates
        // default to `std`; on bare-metal targets (`thumbv7m-none-eabi`
        // etc.) auto-enabling `std` makes `nros-serdes` and friends
        // emit `extern crate std` and fail with E0463 "can't find crate
        // for `std`". The explicit `--features` arg below restores
        // whatever the consumer actually activated (including `std`
        // when target=host).
        .arg("--no-default-features")
        .arg("--message-format=json-render-diagnostics");
    if profile == "release" {
        cmd.arg("--release");
    }

    // Phase 118.E.2 (corrosion compat): scrub env vars that the outer
    // cross-build (typically corrosion-driven CMake) injects globally
    // and which break the nested cargo's host-side proc-macro compiles.
    // `RUSTFLAGS` applies to every rustc invocation under cargo,
    // including host crates like `proc-macro2`; cross-target link-args
    // (`-C link-arg=...`, `-C linker=...`) make those fail. Stripping
    // them is safe for size-probing because:
    //   * rlibs don't link, so link-args don't matter;
    //   * `size_of::<T>()` depends on the target *triple*'s data
    //     layout, not on `-C target-cpu` / `-C target-feature` (those
    //     control codegen, not layout).
    // We keep `CARGO_TARGET_<TRIPLE>_RUSTFLAGS` because it's already
    // target-scoped and won't poison host builds.
    // ISSUE 0022 — strip the make jobserver from the nested probe cargo. This is
    // the SOURCE fix for the cyclone fixture deadlock (every platform's cyclone
    // build goes through `nros`, hence this probe). When the outer build runs
    // under a GNU make jobserver (the fixture builder uses `make
    // --jobserver-style=fifo`), the outer cargo holds jobserver tokens and then
    // BLOCKS in this build script waiting for the nested probe cargo below; if
    // the probe inherited the same jobserver it would wait for a token the outer
    // cargo holds → circular wait (cargo does not release its tokens before
    // blocking on a child cargo — a known recursive-cargo jobserver hazard).
    // Removing `MAKEFLAGS` / `CARGO_MAKEFLAGS` makes the probe use its OWN job
    // budget, so it never competes for the parent's tokens and the deadlock
    // cannot form — on ANY platform, without disabling jobserver coordination
    // for the outer build. DO NOT drop these two without restoring an
    // equivalent jobserver strip (see the issue-0022 box in
    // scripts/build/fixture-make-driver.sh).
    for var in [
        "RUSTFLAGS",
        "CARGO_BUILD_RUSTFLAGS",
        "CARGO_ENCODED_RUSTFLAGS",
        "CARGO_BUILD_TARGET",
        "CARGO_BUILD_TARGET_DIR",
        "MAKEFLAGS",
        "CARGO_MAKEFLAGS",
        "MAKELEVEL",
    ] {
        cmd.env_remove(var);
    }

    // Phase 118.E.2: derive the feature set for the nested invocation
    // by intersecting the consumer's active features (CARGO_FEATURE_*
    // env vars) with the probed crate's declared features (queried via
    // `cargo metadata --no-deps`). This filter is necessary because
    // consumer crates may carry features the probed crate doesn't
    // (e.g. `unstable-zenoh-api` is exposed by nros-cpp but not by
    // nros), and `cargo build --features <unknown>` errors out.
    let forwarded = resolved_features_for(crate_name).unwrap_or_else(|e| {
        println!(
            "cargo:warning=nros-sizes-build: feature-set resolution \
             failed ({e}); falling back to identity forwarding"
        );
        forwarded_features()
    });
    if !forwarded.is_empty() {
        cmd.arg("--features").arg(forwarded.join(","));
    }

    let output = cmd
        .output()
        .map_err(|e| Error::CargoMetadata(e.to_string()))?;
    if !output.status.success() {
        // Write full stderr to a debug log next to the probe target dir
        // so the user can inspect the actual rustc error message; the
        // `cargo:warning=` carries only a path pointer + short summary.
        let log_path = probe_target_dir.join("nested-cargo-stderr.log");
        if let Some(parent) = log_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(&log_path, &output.stderr);

        let stderr = String::from_utf8_lossy(&output.stderr);
        let last = stderr
            .lines()
            .filter(|l| l.starts_with("error") || l.starts_with("  --> ") || l.starts_with("note:"))
            .take(6)
            .collect::<Vec<_>>()
            .join(" | ");
        return Err(Error::CargoMetadata(format!(
            "nested cargo build failed (full log: {}): {}",
            log_path.display(),
            if last.is_empty() {
                "(no error-prefixed lines captured)"
            } else {
                &last
            }
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

/// Phase 118.E.2: intersect consumer's active features with the probed
/// crate's declared features.
///
/// Algorithm:
///
/// 1. Read consumer's `CARGO_FEATURE_<NAME>=1` env vars (via
///    [`forwarded_features`]) — the names the outer cargo activated on
///    the consumer crate.
/// 2. Run `cargo metadata --format-version=1 --no-deps` from
///    `CARGO_MANIFEST_DIR` to list workspace packages. Walk packages
///    for one named `crate_name` (the probed crate) and read its
///    `features` table (the full feature universe declared in its
///    `Cargo.toml`).
/// 3. Return the intersection — features the consumer activated AND
///    the probed crate actually declares. Anything else would cause
///    `cargo build --features <unknown>` to error.
///
/// Returns an empty Vec (not an error) if the probed crate isn't
/// listed in the workspace metadata; isolated-mode callers fall back
/// to identity forwarding in that case via [`forwarded_features`].
fn resolved_features_for(crate_name: &str) -> Result<Vec<String>, Error> {
    use std::collections::HashSet;

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

    let packages = meta
        .get("packages")
        .and_then(|p| p.as_array())
        .ok_or(Error::MalformedMetadata("packages"))?;

    let declared: HashSet<String> = packages
        .iter()
        .find(|p| p.get("name").and_then(|n| n.as_str()) == Some(crate_name))
        .and_then(|p| p.get("features"))
        .and_then(|f| f.as_object())
        .map(|m| m.keys().cloned().collect())
        .unwrap_or_default();

    if declared.is_empty() {
        // Crate not listed in `--no-deps` workspace metadata (e.g. a
        // git or registry dep). Caller's fallback handles it.
        return Ok(Vec::new());
    }

    Ok(forwarded_features()
        .into_iter()
        .filter(|f| declared.contains(f))
        .collect())
}

/// Produce a path-safe slug from `rustc -V` (or `$CARGO_BUILD_RUSTC -V`
/// when set) for use as a probe target dir suffix. Keeps probe
/// artefacts from different rustc versions / channels from colliding.
fn rustc_version_slug() -> String {
    let rustc = env::var_os("CARGO_BUILD_RUSTC")
        .or_else(|| env::var_os("RUSTC"))
        .unwrap_or_else(|| "rustc".into());
    let output = Command::new(&rustc).arg("-V").output();
    let version = output
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
        .unwrap_or_else(|| "unknown".to_string());
    // Sanitize: keep [A-Za-z0-9._-], replace others with '-'.
    let mut slug = String::with_capacity(version.len());
    for c in version.trim().chars() {
        if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_') {
            slug.push(c);
        } else {
            slug.push('-');
        }
    }
    if slug.is_empty() {
        slug.push_str("unknown");
    }
    slug
}

/// Phase 119.1: merge `new_values` against any matching `#define NAME N`
/// already present in `header_path`, taking the max of each pair. Returns
/// the merged map.
///
/// `header_prefix` is prepended to each `new_values` key when matching
/// against the header's `#define NAME N` lines — e.g. probed key
/// `EXECUTOR_SIZE` matches header define `NROS_EXECUTOR_SIZE` when
/// `header_prefix = "NROS_"`. Pass an empty string for an exact match.
///
/// Rationale: each consumer crate (`nros-c`, `nros-cpp`) writes its
/// generated header into the package source tree. Multiple cmake builds
/// (posix/zenoh, posix/xrce, freertos, threadx-riscv, ...) all run in
/// sequence against the same source tree and overwrite the header. The
/// installed library variants then have target-specific sizes that
/// don't match the last-write-wins header → memory corruption when the
/// C/C++ wrapper allocates opaque storage smaller than what the linked
/// Rust runtime actually writes.
///
/// Taking the max across all variants makes the shared header a safe
/// upper bound — every variant fits. Wastes a few bytes per executor
/// on variants whose actual size is smaller; correctness > frugality
/// for the public include path.
pub fn merge_header_max_values(
    header_path: &Path,
    header_prefix: &str,
    new_values: &HashMap<String, u64>,
) -> HashMap<String, u64> {
    let existing = read_header_defines(header_path).unwrap_or_default();
    let mut merged = new_values.clone();
    // Pull existing header values into the merged map (stripping the
    // header_prefix). Also covers the case where the current probe
    // returned an empty map (e.g. `cargo check --no-default-features`):
    // we still preserve the prior header values.
    for (full_key, &old_val) in &existing {
        let Some(key) = full_key.strip_prefix(header_prefix) else {
            continue;
        };
        let entry = merged.entry(key.to_string()).or_insert(0);
        if old_val > *entry {
            *entry = old_val;
        }
    }
    merged
}

/// Parse `#define NAME N` lines from a C header. Used by
/// [`merge_header_max_values`] to recover prior probe results before
/// overwriting the file.
pub fn read_header_defines(header_path: &Path) -> Result<HashMap<String, u64>, Error> {
    let text = std::fs::read_to_string(header_path)?;
    let mut out = HashMap::new();
    for line in text.lines() {
        let trimmed = line.trim();
        let Some(rest) = trimmed.strip_prefix("#define") else {
            continue;
        };
        let mut parts = rest.split_whitespace();
        let Some(name) = parts.next() else { continue };
        let Some(value_str) = parts.next() else {
            continue;
        };
        let Ok(value) = value_str.parse::<u64>() else {
            continue;
        };
        out.insert(name.to_string(), value);
    }
    Ok(out)
}

/// Phase 119.3: derive a deterministic variant slug from the consumer
/// crate's active cargo features. Used by `nros-c`/`nros-cpp` build
/// scripts to scope per-build generated headers under
/// `$CARGO_TARGET_DIR/nros-{c,cpp}-generated/<slug>/`.
///
/// The slug is the sorted, underscore-joined list of all features
/// (lowercase-with-dashes). Example with rmw-zenoh + platform-posix +
/// ros-humble + std:
///   `platform-posix_rmw-zenoh_ros-humble_std`
///
/// Sorting makes the slug independent of cargo's iteration order.
/// Returns `"default"` when no features are set (workspace-default
/// `cargo check`).
pub fn variant_slug_from_env() -> String {
    let mut features = forwarded_features();
    if features.is_empty() {
        return "default".to_string();
    }
    features.sort();
    features.join("_")
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

/// Resolve the workspace target directory for the filesystem-fallback path.
///
/// Order: `CARGO_TARGET_DIR` env override → walk `OUT_DIR` for the
/// `<target>/[triple]/<profile>/build/` ancestor → `cargo metadata`.
pub fn cargo_target_dir() -> Result<PathBuf, Error> {
    if let Ok(dir) = env::var("CARGO_TARGET_DIR")
        && !dir.is_empty()
    {
        return Ok(PathBuf::from(dir));
    }

    if let Ok(out) = env::var("OUT_DIR") {
        let out = PathBuf::from(out);
        let mut p = out.as_path();
        while let Some(parent) = p.parent() {
            if parent.file_name().and_then(|s| s.to_str()) == Some("build")
                && let Some(profile_dir) = parent.parent()
                && let Some(triple_or_target) = profile_dir.parent()
                && let Some(name) = triple_or_target.file_name().and_then(|s| s.to_str())
            {
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
