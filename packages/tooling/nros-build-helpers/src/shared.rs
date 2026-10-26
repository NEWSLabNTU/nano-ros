use std::{
    collections::HashMap,
    env,
    path::{Path, PathBuf},
};

pub type SizeMap = HashMap<String, u64>;

/// The feature that compiles the `__NROS_SIZE_*` markers into `nros`.
///
/// Issue 1765 — requested HERE, for the probe's own nested build, and nowhere
/// else. It used to sit on `nros-c`'s and `nros-cpp`'s `nros` dependency, which
/// put the markers in every linked C/C++ image as zero-filled flash (111,252 B
/// of a 587,256 B FreeRTOS C zenoh talker). `nros/build.rs` now refuses it in
/// any build the probe did not start.
const SIZE_MARKER_FEATURE: &str = "ffi-size-markers";

/// The `nros` rlib the probe measured — ONE call shape for both readers, so the
/// stamp and the sizes always name the same probe directory.
fn probe_rlib() -> Result<PathBuf, nros_sizes_build::Error> {
    let mut features = vec![SIZE_MARKER_FEATURE];
    features.extend(rmw_dispatch_features(|k| env::var(k).ok()));
    nros_sizes_build::find_dep_rlib_with_probe_features("nros", "__NROS_SIZE_", &features)
}

/// Issue 2041 -- the `nros-rmw` dispatch features the OUTER graph resolved,
/// for the probe's nested build of `nros`.
///
/// They are turned on by the linked backend (`nros-rmw-zenoh` and
/// `nros-rmw-xrce-cffi` enable `nros-rmw/in-place-dispatch`), which the probe
/// does not build, so the probe's `nros-node` used to price a sizing
/// descriptor's `in_place` subscription rows at a full receive region while
/// the linked `nros-node` priced them at the entry struct alone. The sizes it
/// measures (`NROS_CPP_LEVELS_BACKING_SIZE`, the tier backings) then disagreed
/// with the executor the image links, and the larger one was the static
/// paid for. The callers (`nros-c`, `nros-cpp`) depend on `nros-rmw`
/// directly, so cargo hands their build scripts its `links` metadata.
fn rmw_dispatch_features(var: impl Fn(&str) -> Option<String>) -> Vec<&'static str> {
    let on = |k: &str| var(k).as_deref() == Some("1");
    let mut out = Vec::new();
    if on("DEP_NROS_RMW_IN_PLACE_DISPATCH") {
        out.push("nros-rmw/in-place-dispatch");
    }
    if on("DEP_NROS_RMW_BUFFERED_DISPATCH") {
        out.push("nros-rmw/buffered-dispatch");
    }
    out
}

pub fn probe_nros_sizes(crate_label: &str) -> SizeMap {
    let rlib = match probe_rlib() {
        Ok(p) => p,
        Err(e) => panic!(
            "{crate_label}: size probe could not locate the `nros` rlib: {e}\n\
             \n\
             These sizes become the opaque-storage macros C and C++ callers \n\
             allocate against, so a guess here is a short buffer, not a wrong \n\
             report. Issue 0464 removed the two fallbacks that used to hide \n\
             this: a poll of the outer target dir (which could return ANOTHER \n\
             consumer's rlib, picked by mtime) and a table of committed \n\
             constants that had already rotted ~11% below the real \n\
             `size_of::<Executor>()`.\n\
             \n\
             The probe builds `nros` itself in an isolated target dir, so this \n\
             failure means that nested build could not run — most often a \n\
             target needing `-Z build-std` that `find_dep_rlib_isolated` has no \n\
             branch for (it handles NuttX explicitly). Add the branch there \n\
             rather than reinstating a fallback."
        ),
    };
    match nros_sizes_build::extract_sizes(&rlib, "__NROS_SIZE_") {
        Ok(map) => map,
        Err(e) => panic!(
            "{crate_label}: size probe found {} but could not read \
             `__NROS_SIZE_*` from it: {e}",
            rlib.display()
        ),
    }
}

pub fn compile_c_stub(
    manifest_dir: &Path,
    rel_path: &str,
    include_dir: Option<&Path>,
    lib_name: &str,
    use_baremetal_libc: bool,
) {
    let path = manifest_dir.join(rel_path);
    println!("cargo:rerun-if-changed={}", path.display());
    let mut build = cc::Build::new();
    // issue 0383 — implicit-function-declaration / int-conversion as errors.
    nros_cc_flags::strict_decls(&mut build);
    build
        .file(&path)
        .warnings(true)
        .extra_warnings(true)
        .flag_if_supported("-Wpedantic");
    if let Some(include_dir) = include_dir {
        build.include(include_dir);
    }
    if use_baremetal_libc {
        apply_baremetal_libc(&mut build);
    }
    nros_cc_flags::header_deps::track_header_deps(&mut build);
    build.compile(lib_name);
    // issue 1580 — the stub's headers, not only the stub itself.
    nros_cc_flags::header_deps::emit_header_deps(&std::path::PathBuf::from(
        env::var("OUT_DIR").expect("OUT_DIR"),
    ));
}

pub fn apply_baremetal_libc(build: &mut cc::Build) {
    let arch = env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default();
    let os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if arch != "riscv64" || os != "none" {
        return;
    }
    // issue 0491 — `NROS_PICOLIBC_SYSROOT` names a DIRECTORY, so it is watched
    // by CONTENT rather than fingerprinted as a string: cargo compares an env
    // value textually, and a directory can reach this script under more than
    // one spelling. phase-412: nothing in the tree sets it any more (the `just`
    // export and the leaf `[env]` rows this comment once named are gone); it is
    // an operator override, and `picolibc_include()` asks the compiler when it
    // is unset.
    if let Some(include) = picolibc_include() {
        if std::path::Path::new(&include).is_dir() {
            println!("cargo:rerun-if-changed={include}");
        }
        build.flag("-isystem").flag(&include);
    }
}

pub fn picolibc_include() -> Option<String> {
    if let Ok(root) = env::var("NROS_PICOLIBC_SYSROOT") {
        let include = format!("{root}/include");
        if Path::new(&include).is_dir() {
            return Some(include);
        }
    }
    if let Ok(output) = std::process::Command::new(nros_build_paths::riscv64::tool_or_legacy("gcc"))
        .args([
            "-march=rv64gc",
            "-mabi=lp64d",
            "--specs=picolibc.specs",
            "-print-sysroot",
        ])
        .output()
    {
        let sysroot = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !sysroot.is_empty() {
            let include = format!("{sysroot}/include");
            if Path::new(&include).is_dir() {
                return Some(include);
            }
        }
    }
    let fallback = "/usr/lib/picolibc/riscv64-unknown-elf/include";
    if Path::new(fallback).is_dir() {
        return Some(fallback.to_string());
    }
    None
}

/// Render a committed cbindgen header's CONTENT, without writing anything.
///
/// Split out of `generate_cbindgen_header` so the regenerator binary and the
/// build-script comparison path run the *same* generation — a second spelling
/// here is how the two would drift and the gate would start passing for the
/// wrong reason.
///
/// Generation is a pure function of the crate's own sources plus its
/// `cbindgen.toml`: both configs set `parse_deps = false` and leave
/// `[parse.expand]` off, so no dependency graph and no `cargo expand` are
/// involved, which is exactly what lets a standalone binary reproduce what a
/// build script used to produce in place.
///
/// phase-400 W2a — behind `cbindgen-drift-check`, which the regenerator binary
/// turns on and no build script does. See the manifest for why the default is
/// off.
#[cfg(feature = "cbindgen-drift-check")]
pub fn render_cbindgen_header(manifest_dir: &Path, config_name: &str) -> Result<String, String> {
    let config_path = manifest_dir.join(config_name);
    let config = cbindgen::Config::from_file(&config_path)
        .map_err(|e| format!("failed to load {}: {e}", config_path.display()))?;

    let bindings = cbindgen::Builder::new()
        .with_crate(manifest_dir)
        .with_config(config)
        .generate()
        .map_err(|e| format!("cbindgen generation failed: {e}"))?;

    let mut buf = Vec::new();
    bindings.write(&mut buf);
    String::from_utf8(buf).map_err(|e| format!("cbindgen emitted non-UTF-8: {e}"))
}

/// Issue 0452 — COMPARE the committed header against a freshly rendered one.
/// **Never writes the source tree.**
///
/// These headers (`nros_generated.h`, `nros_cpp_ffi.h`, `zpico.h`) are
/// committed, and until now every `build.rs` rewrote them IN PLACE. Two things
/// followed from that, and both are fixed by not writing:
///
/// * **A build dirtied the worktree.** When the graph resolved a different
///   cbindgen patch release, ~36 lines flipped and `git status` showed changes
///   nobody made; committing them reverted an upstream improvement, which had
///   to be undone by hand twice during phase-338. The exact `=` requirement in
///   `[workspace.dependencies]` closes the version half; this closes the
///   "a build writes tracked source at all" half.
/// * **N concurrent build trees raced on one path**, which is why a
///   cross-process advisory lock exists below (known-issues #15). Only the
///   regenerator writes now, and it is one process.
///
/// Drift is reported as a `cargo::warning` rather than a hard error on purpose:
/// mid-edit divergence is the normal state while someone is changing an FFI
/// signature, and failing their build would teach them to bypass this. The
/// enforcement point is `check-cbindgen-headers`, which runs in `just check`.
///
/// phase-400 W2a — the COMPARISON is behind `cbindgen-drift-check` and off by
/// default; the `rerun-if-changed` on the committed header is NOT, because that
/// edge is about the build's own inputs (the C stub `#include`s the header) and
/// has nothing to do with drift reporting. Dropping it with the warning would
/// have turned an opt-in diagnostic into a missing dependency edge — issue
/// 0475's shape, one crate over.
pub fn generate_cbindgen_header(manifest_dir: &Path, config_name: &str, output_rel: &str) {
    let output_path = manifest_dir.join(output_rel);
    println!("cargo:rerun-if-changed={}", output_path.display());

    #[cfg(not(feature = "cbindgen-drift-check"))]
    let _ = config_name;

    #[cfg(feature = "cbindgen-drift-check")]
    drift_check_committed_header(manifest_dir, config_name, output_rel, &output_path);
}

/// The drift comparison itself. Split out so the unconditional
/// `rerun-if-changed` above cannot be lost behind the `cfg`.
#[cfg(feature = "cbindgen-drift-check")]
fn drift_check_committed_header(
    manifest_dir: &Path,
    config_name: &str,
    output_rel: &str,
    output_path: &Path,
) {
    let fresh = match render_cbindgen_header(manifest_dir, config_name) {
        Ok(s) => s,
        Err(e) => {
            println!("cargo:warning=cbindgen header check skipped: {e}");
            return;
        }
    };

    // Stash the fresh copy in OUT_DIR so a developer can diff it without
    // re-running cbindgen by hand. Best-effort: a missing OUT_DIR (doc builds,
    // some IDE probes) must not break the build.
    if let Ok(out_dir) = std::env::var("OUT_DIR") {
        let name = Path::new(output_rel)
            .file_name()
            .unwrap_or_else(|| std::ffi::OsStr::new("cbindgen-header"));
        let _ = std::fs::write(Path::new(&out_dir).join(name), &fresh);
    }

    match std::fs::read_to_string(output_path) {
        Ok(committed) if committed == fresh => {}
        Ok(_) => {
            println!(
                "cargo:warning={} is STALE against this crate's sources — run \
                 `just regen-c-headers` and commit the result (issue 0452). \
                 The build used the committed copy.",
                output_path.display()
            );
        }
        Err(e) => {
            println!(
                "cargo:warning=cannot read committed header {}: {e} — run \
                 `just regen-c-headers` (issue 0452)",
                output_path.display()
            );
        }
    }
}

/// The one writer: render and replace the committed header in place.
///
/// Used by the `nros-cbindgen-headers` binary (`just regen-c-headers`), never by
/// a build script.
/// Takes FINAL content rather than rendering it, because not every committed
/// header is raw cbindgen output — `zpico.h` is post-processed. Rendering here
/// would force the caller that needs the post-pass to write the file some other
/// way, and then there would be two writers again.
///
/// Returns whether the file changed.
pub fn write_committed_header(output_path: &Path, content: &str) -> bool {
    let _guard = HeaderLock::acquire(output_path);
    write_cbindgen_header_atomically(output_path, content)
}

// WHY A CROSS-PROCESS LOCK STILL EXISTS BELOW, AND WHY IT NO LONGER CARRIES
// WHAT IT USED TO (issue 0452).
//
// These headers were regenerated IN PLACE by every parallel `build.rs`
// invocation. On a cold workspace build, N independent Corrosion / Cargo trees
// (e.g. the threadx-linux C++ fixtures, which — unlike nuttx — are not
// serialized with `NROS_CARGO_FRONTENDS=1`) ran that code concurrently against
// the SAME output path. An atomic rename makes one writer's replacement safe,
// but nothing serialized N writers racing on write/compare/rename, so a
// concurrent reader (a C++ compile `#include`-ing the header) could observe an
// intermediate state — known-issues #15, transient "multiple definition /
// conflicting declaration of `nros_cpp_qos_t`".
//
// Builds no longer write these files at all: they compare and warn. The only
// writer is `just regen-c-headers`, one process. The lock is kept because that
// regenerator is still a writer to a shared path and the machinery is already
// tested — but the N-way race it was built for cannot occur any more, so it is
// no longer load-bearing for #15.

/// Cross-process advisory lock guarding regeneration of one shared header.
///
/// Holds the open lockfile for the guard's lifetime; the kernel releases the
/// `flock` when the descriptor is closed on drop.
struct HeaderLock {
    #[cfg(unix)]
    _file: Option<std::fs::File>,
}

impl HeaderLock {
    fn acquire(output_path: &Path) -> Self {
        #[cfg(unix)]
        {
            use std::{
                collections::hash_map::DefaultHasher,
                hash::{Hash, Hasher},
                os::unix::io::AsRawFd,
            };

            // Key the lock on the absolute output path so all concurrent
            // regenerators of the same header agree on one lockfile,
            // regardless of their (differing) cargo target dirs.
            let abs =
                std::fs::canonicalize(output_path).unwrap_or_else(|_| output_path.to_path_buf());
            let mut hasher = DefaultHasher::new();
            abs.hash(&mut hasher);
            let lock_path = env::temp_dir().join(format!(
                "nros-cbindgen-header-{:016x}.lock",
                hasher.finish()
            ));

            let file = match std::fs::OpenOptions::new()
                .create(true)
                .read(true)
                .write(true)
                .truncate(false)
                .open(&lock_path)
            {
                Ok(f) => f,
                // If the lockfile can't be opened, fall back to the bare
                // atomic rename (still safe against partial reads).
                Err(_) => return HeaderLock { _file: None },
            };

            // Blocking exclusive advisory lock, retrying on EINTR. Released
            // automatically when `file` is dropped (descriptor close).
            unsafe extern "C" {
                fn flock(fd: i32, op: i32) -> i32;
            }
            const LOCK_EX: i32 = 2;
            const EINTR: i32 = 4;
            loop {
                let rc = unsafe { flock(file.as_raw_fd(), LOCK_EX) };
                if rc == 0 {
                    break;
                }
                if std::io::Error::last_os_error().raw_os_error() != Some(EINTR) {
                    // Lock failed for a non-recoverable reason; proceed with
                    // the atomic rename alone rather than blocking the build.
                    return HeaderLock { _file: None };
                }
            }
            HeaderLock { _file: Some(file) }
        }
        #[cfg(not(unix))]
        {
            let _ = output_path;
            HeaderLock {}
        }
    }

    /// Whether a real exclusive advisory lock is held (false when the lock
    /// degraded to a no-op). Used only by the serialization unit test.
    #[cfg(all(test, unix))]
    fn is_real(&self) -> bool {
        self._file.is_some()
    }
}

/// Returns whether the committed file actually changed.
fn write_cbindgen_header_atomically(output_path: &Path, content: &str) -> bool {
    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    // The temp file MUST live in the header's own directory: the atomicity
    // here comes from `rename`, which is only atomic within one filesystem.
    // That directory is the SOURCE tree, so a build killed between the write
    // and the rename orphans a `.<header>.tmp.<pid>` there — where the next
    // `git add -A` will commit it (this is how `.nros_cpp_ffi.h.tmp.229021`
    // reached main). Nothing runs on SIGKILL, so cleanup cannot be RAII;
    // sweep any stale siblings on the way in instead.
    sweep_stale_header_temps(output_path);
    let tmp = output_path.with_file_name(format!(
        ".{}.tmp.{}",
        output_path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("cbindgen-header"),
        std::process::id()
    ));
    std::fs::write(&tmp, content.as_bytes()).ok();
    let differs = std::fs::read(&tmp).ok() != std::fs::read(output_path).ok();
    if differs {
        std::fs::rename(&tmp, output_path).ok();
    } else {
        std::fs::remove_file(&tmp).ok();
    }
    differs
}

/// # Write policy for generated artifacts
///
/// **A shared destination is written atomically; an `OUT_DIR` destination is
/// not.** The distinction is who else can be writing:
///
/// * `$CARGO_TARGET_DIR/nros-{c,cpp}-generated/…`, `$CORROSION_BUILD_DIR/…`
///   and the committed in-tree headers are reachable by more than one crate's
///   build script and by parallel fixture lanes sharing a workspace build dir.
///   These go through [`write_atomic`] (or
///   [`write_cbindgen_header_atomically`]): temp file in the same directory,
///   then `rename(2)`. A plain `fs::write` truncates and then fills, and a
///   reader that lands in the gap sees a spliced file — which is exactly how a
///   zephyr fixture came to compile a header ending mid-token and fail with
///   `#endif without #if` (2026-07-26).
/// * `$OUT_DIR/*.rs` (`nros_c_config.rs`, `nros_cpp_ffi_config.rs`, the
///   surface anchors) is per-crate and per-build. Exactly one writer exists by
///   construction, so a plain `fs::write` is correct and cheaper.
///
/// Adding a generated artifact? Ask which of the two it is. If a second crate
/// or a parallel lane could name the same path, it belongs in the first group.
/// Remove orphaned `.<header>.tmp.<pid>` siblings left by builds that died
/// between the atomic write and its rename.
///
/// Only files matching THIS header's temp shape are touched, and only when the
/// recorded pid is not a live process — a concurrent build's in-flight temp
/// must survive (the header lock serializes writers, but a different crate's
/// build can be mid-write on its own header in the same directory).
fn sweep_stale_header_temps(output_path: &Path) {
    let (Some(dir), Some(name)) = (output_path.parent(), output_path.file_name()) else {
        return;
    };
    let Some(name) = name.to_str() else { return };
    let prefix = format!(".{name}.tmp.");
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let file_name = entry.file_name();
        let Some(file_name) = file_name.to_str() else {
            continue;
        };
        let Some(pid) = file_name.strip_prefix(&prefix) else {
            continue;
        };
        if pid.is_empty() || !pid.bytes().all(|b| b.is_ascii_digit()) {
            continue; // not our shape — leave it alone
        }
        if pid.parse::<u32>().is_ok_and(pid_is_live) {
            continue; // a live build owns this one
        }
        std::fs::remove_file(entry.path()).ok();
    }
}

/// Whether `pid` names a live process. Non-Unix conservatively answers "yes",
/// so a temp file is kept rather than deleted out from under a running build.
///
/// `kill(pid, 0)` rather than `/proc/<pid>`: procfs is LINUX, not unix — FreeBSD
/// does not mount it by default — so under the `cfg(unix)` guard this had a
/// running build's temp file reading as abandoned on every non-Linux unix, and
/// the `cfg(not(unix))` arm could not conserve it because BSD *is* unix. Signal
/// 0 is the POSIX existence check, so one arm now covers the whole family, and
/// `EPERM` is treated as live (the process exists; we merely may not signal it),
/// which keeps the conservative direction this function promises.
fn pid_is_live(pid: u32) -> bool {
    #[cfg(unix)]
    {
        // Declared inline for the same reason as `flock` above — this crate is
        // in every C/C++ build script's graph and does not carry a `libc` dep.
        unsafe extern "C" {
            fn kill(pid: i32, sig: i32) -> i32;
        }
        // `EPERM` is 1 on every POSIX system this tree targets.
        const EPERM: i32 = 1;
        // A pid that does not fit `pid_t`, or is zero, names no process — and
        // `kill(0, …)` would address the CALLER's own process group.
        let Ok(pid) = i32::try_from(pid) else {
            return true; // unparseable: keep the file rather than guess
        };
        if pid <= 0 {
            return false;
        }
        // SAFETY: signal 0 delivers nothing; it runs the existence and
        // permission checks alone.
        if unsafe { kill(pid, 0) } == 0 {
            return true;
        }
        std::io::Error::last_os_error().raw_os_error() == Some(EPERM)
    }
    #[cfg(not(unix))]
    {
        let _ = pid;
        true
    }
}

/// Write a generated header into this build script's `$OUT_DIR` — the ONLY
/// place a per-build config header goes (issue 1824).
///
/// `$OUT_DIR` is where cargo says build-script products go: it is per-unit,
/// hashed by cargo (so two feature sets or two knob sets never collide), it
/// survives a cache hit, and cargo reports its path on the stable JSON stream —
/// `{"reason":"build-script-executed", …, "out_dir": …}` — even on a fully
/// cached run. A consumer reaches it one of two ways: a cargo dependent through
/// the `links` channel (`DEP_NROS_{C,CPP}_CONFIG_INCLUDE`), or any other road
/// through `scripts/build/cargo-out-dir-headers.py`, which reads that stream and
/// copies the tree to a directory the caller owns.
///
/// Until issue 1824 the header was ALSO written to a flat
/// `$CARGO_TARGET_DIR/nros-{c,cpp}-generated/`, with a `.h.stamp` beside it: a
/// path inside cargo's tree that cargo does not manage, shared by every unit
/// in that target dir, so the header there was whichever build wrote it last.
/// Every reader moved off it (D1–D4) before it was deleted.
pub fn write_header_to_out_dir(relative: &Path, contents: &str) {
    let Ok(out_dir) = env::var("OUT_DIR") else {
        return;
    };
    // The FULL relative path is preserved — `nros-c-generated/rclc/detail/<name>.h`,
    // not just the subdir tail — so the placer can copy several packages' trees
    // into one destination verbatim and every header lands in the directory its
    // consumers already include, whichever package produced it.
    write_to(PathBuf::from(out_dir), relative, contents);
}

fn write_to(root: PathBuf, relative: &Path, contents: &str) {
    let dest = root.join(relative);
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).expect("create per-build header dir");
    }
    write_atomic(&dest, contents);
}

/// Check that the C header nros-cpp derives agrees with the one nros-c's build
/// script wrote — issue 1824 (formerly `write_header_if_absent_or_verify`,
/// phase-308 follow-up).
///
/// `nros-cpp` probes the runtime's sizes itself and renders the c-format
/// `nros_config_generated.h` from them; nros-c OWNS that file. The claim that
/// the two always agree is the issue-0268 silent-overflow class if it is ever
/// false (one language sizes its `_opaque` storage wrong), so it is checked
/// here rather than trusted.
///
/// nros-c's copy is read from its OWN `$OUT_DIR`, through the `links` channel
/// (`DEP_NROS_C_CONFIG_INCLUDE`): nros-c is a non-optional dependency, so cargo
/// runs its build script for THIS unit graph first, and the file there belongs
/// to this build. That is what retired the `.h.stamp` sidecar and its
/// stale-vs-divergent heuristic — the shared target-dir copy it compared
/// against could be an older build's, so a mismatch had two meanings; this one
/// has one, and it stops the build.
///
/// Values, not bytes: the two writers render from different sources (nros-c
/// from a `.template`, nros-cpp from an inline string), so comments and spacing
/// differ when every probed size agrees. nros-cpp writes nothing here — its own
/// `$OUT_DIR` holds only its own header, so placing both packages into one
/// destination cannot collide.
pub fn verify_against_nros_c_header(relative: &Path, contents: &str, label: &str) {
    // Absent: nros-c published no config include — its probe declined (e.g. a
    // `cargo check` with no executor sizes). Nothing to compare against, and a
    // consumer that needs the header reaches the stub's `#error`.
    let include = env::var_os("DEP_NROS_C_CONFIG_INCLUDE").map(PathBuf::from);
    verify_against_include(include.as_deref(), relative, contents, label);
}

/// [`verify_against_nros_c_header`] with the `links` include root passed in.
fn verify_against_include(include: Option<&Path>, relative: &Path, contents: &str, label: &str) {
    let Some(include) = include else {
        return;
    };
    // `relative` is `nros-c-generated/rclc/detail/<name>.h` and the include
    // root IS `<OUT_DIR>/nros-c-generated`, so strip the first segment.
    let tail: PathBuf = relative.components().skip(1).collect();
    let theirs = include.join(tail);
    println!("cargo:rerun-if-changed={}", theirs.display());
    let Ok(existing) = std::fs::read_to_string(&theirs) else {
        return;
    };
    if defines_of(&existing) != defines_of(contents) {
        panic!(
            "{label}: nros-c's {} disagrees with the sizes this crate probed.\n\
             Both build scripts ran in THIS cargo invocation, so this is not a stale \
             file: the C and C++ halves resolved different runtime layouts, and one \
             of them would size its `_opaque` storage wrong (silent overflow at \
             runtime). Usually nros-c and nros-cpp were resolved with different \
             features — `nros-sizes-build` keys its probe directory by (rustc, target, \
             features). Disagreeing defines:\n{}",
            theirs.display(),
            disagreeing_defines(&existing, contents),
        );
    }
}

/// `#define NAME VALUE` pairs, in a comparable form.
///
/// The semantic content of a generated sizes header; everything else (comments,
/// blank lines, include guards, the opaque-struct prose) is presentation that
/// legitimately differs between the two writers.
fn defines_of(header: &str) -> Vec<(String, String)> {
    // Issue 0360 — `NROS_CONFIG_VARIANT` is deliberately NOT compared. This
    // guard exists to catch DISAGREEING SIZES: two crates resolving different
    // runtime layouts for the same `_opaque` storage. Feature sets may
    // legitimately differ while every probed size agrees — the C++ lane builds
    // nros-cpp with `rmw-zenoh-cffi` and nros-c without it — so comparing the
    // variant slug would reject a valid configuration and say "different
    // probed sizes" when no size differs. The stamp's own job (catching a
    // header/archive mismatch) is done by the linker, not here.
    let mut out: Vec<(String, String)> = header
        .lines()
        .map(str::trim)
        .filter_map(|l| l.strip_prefix("#define "))
        .filter(|rest| !rest.starts_with("NROS_CONFIG_VARIANT"))
        .filter_map(|rest| {
            let mut it = rest.splitn(2, char::is_whitespace);
            let name = it.next()?.to_string();
            Some((name, it.next().unwrap_or("").trim().to_string()))
        })
        .collect();
    out.sort();
    out
}

/// Only the `#define`s that actually DISAGREE — the useful part of a mismatch
/// report, rather than the first N, which are usually the ones that match. The
/// original report printed twelve identical lines under "on disk" and "would
/// write", which read as the tool contradicting itself.
fn disagreeing_defines(a: &str, b: &str) -> String {
    let (da, db) = (defines_of(a), defines_of(b));
    let mut lines = Vec::new();
    for (name, va) in &da {
        match db.iter().find(|(n, _)| n == name) {
            Some((_, vb)) if vb == va => {}
            Some((_, vb)) => lines.push(format!("  {name}: nros-c={va} vs this-crate={vb}")),
            None => lines.push(format!("  {name}: nros-c={va} vs this-crate=<absent>")),
        }
    }
    for (name, vb) in &db {
        if !da.iter().any(|(n, _)| n == name) {
            lines.push(format!("  {name}: nros-c=<absent> vs this-crate={vb}"));
        }
    }
    lines.join("\n")
}

pub fn write_header_to_corrosion(filename: &str, contents: &str) {
    let Ok(corrosion_dir) = env::var("CORROSION_BUILD_DIR") else {
        return;
    };
    let dest = PathBuf::from(corrosion_dir).join(filename);
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).expect("create corrosion header dir");
    }
    write_atomic(&dest, contents);
}

/// Write a generated header atomically: temp file in the SAME directory, then
/// rename.
///
/// `fs::write` truncates and then fills, so two writers targeting one path tear
/// it. These headers have TWO writers by design: `nros-c`'s build script emits
/// `nros-c-generated/rclc/detail/nros_config_generated.h`, and `nros-cpp`'s emits the
/// same file so Zephyr CPP-only builds (where nros-c never compiles) still find
/// it. Add parallel fixture lanes sharing a workspace build dir and the window
/// widens further.
///
/// Observed 2026-07-26 — a zephyr fixture compiled a header whose tail was
/// spliced mid-token:
///
/// ```text
/// #endif /* NROS_CONFIG_GENERATED_H */
/// plus            <- tail of `__cplus|plus` from the other writer
/// }
/// #endif
/// ```
///
/// and failed with `#endif without #if`. `rename(2)` within a filesystem is
/// atomic, so a reader sees the old file or the whole new one, never a splice.
/// The unchanged-content early return also avoids touching mtimes that would
/// re-trigger dependent rebuilds.
pub fn write_atomic(dest: &std::path::Path, contents: &str) {
    if std::fs::read_to_string(dest).is_ok_and(|old| old == contents) {
        return;
    }
    let dir = dest.parent().unwrap_or_else(|| std::path::Path::new("."));
    // Same directory (rename cannot cross filesystems); pid keeps concurrent
    // writers from colliding on the temp name itself.
    let tmp = dir.join(format!(
        ".{}.{}.tmp",
        dest.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("header"),
        std::process::id()
    ));
    if let Err(e) = std::fs::write(&tmp, contents) {
        let _ = std::fs::remove_file(&tmp);
        panic!("write {}: {e}", tmp.display());
    }
    if let Err(e) = std::fs::rename(&tmp, dest) {
        // Never leave the temp behind on a failed rename: some of these
        // destinations are SOURCE directories (`nros_cpp_ffi.h` is a committed
        // header regenerated in place), so litter there shows up as untracked
        // files and can be committed by accident. A hard kill can still strand
        // one, which is why `.gitignore` also covers the pattern.
        let _ = std::fs::remove_file(&tmp);
        panic!("rename {} -> {}: {e}", tmp.display(), dest.display());
    }
}

/// phase-382 W3' -- the carved parameter store's shape, from `nros-node`'s
/// `links` metadata: an input to the probed executor size.
pub const PARAM_STORE_SHAPE_DEP: &str = "DEP_NROS_NODE_PARAM_STORE_SHAPE";

/// phase-382 W3' -- read [`PARAM_STORE_SHAPE_DEP`] for its FINGERPRINT, not a
/// number. When the build carves a parameter store, the store's shape sizes
/// the executor backing. A sizes probe whose build script does not re-run when
/// the shape moves emits a museum `NROS_EXECUTOR_SIZE` (measured:
/// `NROS_PARAM_STORE=1` tripped nros-c's `EXECUTOR_OPAQUE_U64S too small`
/// assert). Watched explicitly, so the re-run does not depend on how cargo
/// treats a dependency's metadata.
pub fn watch_param_store_shape() -> String {
    println!("cargo:rerun-if-env-changed={PARAM_STORE_SHAPE_DEP}");
    env::var(PARAM_STORE_SHAPE_DEP).unwrap_or_else(|_| {
        panic!(
            "{PARAM_STORE_SHAPE_DEP} not set — is nros-node's `links = \"nros_node\"` configured?"
        )
    })
}

pub fn dep_usize(name: &str) -> usize {
    env::var(name)
        .unwrap_or_else(|_| {
            panic!("{name} not set — is nros-node's `links = \"nros_node\"` configured?")
        })
        .parse()
        .unwrap_or_else(|_| panic!("{name} is not a valid usize"))
}

pub fn target_pointer_bytes() -> usize {
    match env::var("CARGO_CFG_TARGET_POINTER_WIDTH").ok().as_deref() {
        Some("32") => 4,
        Some("64") => 8,
        _ => core::mem::size_of::<*const ()>(),
    }
}

pub fn non_zero_or(probe: usize, fallback: usize) -> usize {
    if probe != 0 { probe } else { fallback }
}

/// Issue 0360 — the cargo feature set of THIS build, as a stable slug.
///
/// Sorted and underscore-joined, which is exactly what the checked-in stub
/// headers have documented since Phase 119.3 as `<variant_slug>` — a path
/// component nothing ever implemented. It is used here for a different and
/// cheaper purpose than the documented one: stamping the variant INTO the
/// generated header (and a matching symbol into the archive) so that compiling
/// against one feature set and linking another is a LINK error instead of a
/// silent size mismatch.
///
/// Silent is the whole problem: the header carries `NROS_EXECUTOR_SIZE` and the
/// `_OPAQUE_U64S` counts, a consumer sizes its buffers from them, and the Rust
/// side writes according to whatever it was actually built with. Disagreement
/// overflows at runtime rather than failing to build — the reason issues
/// 0088 / 0114 / 0122 / 0123 / 0245 / 0268 exist as a family.
pub fn variant_slug() -> String {
    let mut feats: Vec<String> = env::vars()
        .filter_map(|(k, _)| k.strip_prefix("CARGO_FEATURE_").map(str::to_string))
        .map(|f| f.to_ascii_lowercase())
        .collect();
    feats.sort();
    if feats.is_empty() {
        "default".to_string()
    } else {
        feats.join("_")
    }
}

/// The same slug reduced to a C identifier suffix (`[a-z0-9_]`).
pub fn variant_symbol_suffix() -> String {
    variant_slug()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect()
}

/// issue 0369 — the variant ANCHOR suffix, derived from the header's own
/// size-determining VALUES rather than the cargo feature spelling.
///
/// The feature-list suffix was over-broad: a mixed C+C++ workspace builds
/// nros-c twice with legitimately different rmw-feature SPELLINGS
/// (`cffi-zenoh-cffi` vs the union with `rmw-cffi` that nros-cpp pulls in)
/// that resolve to IDENTICAL sizes — yet the shared generated header
/// (first-writer) named one build's slug and the other build's archive
/// emitted the other, an undefined reference at link (issue 0369; blocked
/// the native/threadx mixed fixtures).
///
/// The anchor's guarantee is the SIZE contract (the 0088…0268 family:
/// header sizes vs archive sizes), so the suffix hashes exactly the
/// `(name, value)` pairs the header ships. Two builds that agree on every
/// size agree on the symbol by construction; a consumer holding a header
/// with different sizes still fails to link, which is the point. The
/// wrong-backend case the feature slug also caught (phase-325 W3's
/// overwritten archive) keeps failing on its own missing backend symbols,
/// as it did before the anchor existed.
pub fn variant_suffix_from_sizes(sizes: &[(&str, usize)]) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for (name, value) in sizes {
        for b in name
            .as_bytes()
            .iter()
            .chain(b"=")
            .chain(value.to_string().as_bytes())
        {
            h ^= *b as u64;
            h = h.wrapping_mul(0x0000_0100_0000_01b3);
        }
        h ^= b';' as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("sz_{h:016x}")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Issue 2041 -- the probe asks for exactly the dispatch features the outer
    /// graph's `nros-rmw` reported, so its `nros-node` prices `in_place` rows
    /// as the linked one does.
    #[test]
    fn the_probe_inherits_the_outer_rmw_dispatch_features() {
        let env = |pairs: &'static [(&'static str, &'static str)]| {
            move |k: &str| {
                pairs
                    .iter()
                    .find(|(n, _)| *n == k)
                    .map(|(_, v)| v.to_string())
            }
        };
        assert_eq!(
            rmw_dispatch_features(env(&[
                ("DEP_NROS_RMW_IN_PLACE_DISPATCH", "1"),
                ("DEP_NROS_RMW_BUFFERED_DISPATCH", "0"),
            ])),
            vec!["nros-rmw/in-place-dispatch"]
        );
        assert_eq!(
            rmw_dispatch_features(env(&[
                ("DEP_NROS_RMW_IN_PLACE_DISPATCH", "1"),
                ("DEP_NROS_RMW_BUFFERED_DISPATCH", "1"),
            ])),
            vec!["nros-rmw/in-place-dispatch", "nros-rmw/buffered-dispatch"]
        );
        assert!(rmw_dispatch_features(env(&[])).is_empty());
    }

    /// Issue 1824 — nros-cpp's check reads nros-c's OWN `$OUT_DIR` header
    /// (the `links` include root), passes on agreeing VALUES whatever the
    /// presentation, and stops the build on a disagreeing define. Absent
    /// channel or file: nothing to compare, no panic.
    #[test]
    fn nros_cpp_check_reads_nros_c_out_dir_and_refuses_disagreement() {
        let root = std::env::temp_dir().join(format!("nros-1824-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let rel = nros_build_paths::artifact::Artifact::CConfigHeader.relative(None, "dev");
        let include = root.join(nros_build_paths::artifact::C_GENERATED_DIR);
        let theirs = root.join(&rel);
        std::fs::create_dir_all(theirs.parent().unwrap()).unwrap();
        std::fs::write(
            &theirs,
            "/* nros-c */\n#define NROS_CONFIG_VARIANT \"a\"\n#define NROS_EXECUTOR_SIZE 42\n",
        )
        .unwrap();
        let inc = Some(include.as_path());

        let same = "#define NROS_EXECUTOR_SIZE 42\n#define NROS_CONFIG_VARIANT \"b\"\n";
        verify_against_include(inc, &rel, same, "t");

        let differ = "#define NROS_EXECUTOR_SIZE 43\n";
        let r = std::panic::catch_unwind(|| verify_against_include(inc, &rel, differ, "t"));
        let msg = r.expect_err("a disagreeing define must stop the build");
        let msg = msg.downcast_ref::<String>().cloned().unwrap_or_default();
        assert!(
            msg.contains("NROS_EXECUTOR_SIZE: nros-c=42 vs this-crate=43"),
            "{msg}"
        );

        std::fs::remove_file(&theirs).unwrap();
        verify_against_include(inc, &rel, differ, "t");
        verify_against_include(None, &rel, differ, "t");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// The cbindgen temp-header sweep removes ONLY orphans of this header
    /// whose owning pid is gone. A live build's in-flight temp, another
    /// header's temp, and anything not matching the `.tmp.<digits>` shape all
    /// survive — deleting those would corrupt a concurrent build.
    #[test]
    fn sweep_removes_only_dead_orphans_of_this_header() {
        let dir = std::env::temp_dir().join(format!("nros-sweep-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let header = dir.join("nros_cpp_ffi.h");
        std::fs::write(&header, "// header\n").unwrap();

        // Orphan: a pid that cannot be live (0 is never a real user process).
        let dead = dir.join(".nros_cpp_ffi.h.tmp.0");
        // Live: this very process still owns it.
        let live = dir.join(format!(".nros_cpp_ffi.h.tmp.{}", std::process::id()));
        // A DIFFERENT header's orphan — not ours to reap.
        let other = dir.join(".other_header.h.tmp.0");
        // Right prefix, wrong shape (not all digits).
        let malformed = dir.join(".nros_cpp_ffi.h.tmp.notapid");
        for f in [&dead, &live, &other, &malformed] {
            std::fs::write(f, "x").unwrap();
        }

        sweep_stale_header_temps(&header);

        assert!(!dead.exists(), "a dead pid's orphan must be swept");
        assert!(live.exists(), "a live build's in-flight temp must survive");
        assert!(
            other.exists(),
            "another header's temp is not ours to remove"
        );
        assert!(malformed.exists(), "non-pid suffixes must be left alone");
        assert!(header.exists(), "the real header must never be touched");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn non_zero_or_prefers_probe() {
        assert_eq!(non_zero_or(24, 48), 24);
        assert_eq!(non_zero_or(0, 48), 48);
    }

    // Proves the cross-process advisory lock guarding cbindgen header
    // regeneration (known-issues #15) actually serializes the critical
    // section: many concurrent holders of `HeaderLock` keyed on the same
    // output path must never overlap. flock locks are associated with the
    // open file description, so independent `open()`s — even within one
    // process — mutually exclude, which is exactly what concurrent `build.rs`
    // invocations do.
    #[cfg(unix)]
    #[test]
    fn header_lock_serializes_concurrent_holders() {
        use std::{
            sync::{
                Arc,
                atomic::{AtomicUsize, Ordering},
            },
            thread,
        };

        // A unique-per-run synthetic output path; canonicalize() will fail and
        // fall back to the raw path, so all threads hash the same key.
        let key = env::temp_dir().join(format!(
            "nros-header-lock-test-{}-{}.h",
            std::process::id(),
            "qos"
        ));

        let in_section = Arc::new(AtomicUsize::new(0));
        let max_overlap = Arc::new(AtomicUsize::new(0));

        let handles: Vec<_> = (0..8)
            .map(|_| {
                let key = key.clone();
                let in_section = Arc::clone(&in_section);
                let max_overlap = Arc::clone(&max_overlap);
                thread::spawn(move || {
                    for _ in 0..200 {
                        let _guard = HeaderLock::acquire(&key);
                        // Skip the assertion entirely if the lock degraded to a
                        // no-op (e.g. temp dir unwritable) — we only assert the
                        // mutual-exclusion property when a real lock was taken.
                        if !_guard.is_real() {
                            continue;
                        }
                        let now = in_section.fetch_add(1, Ordering::SeqCst) + 1;
                        max_overlap.fetch_max(now, Ordering::SeqCst);
                        // Encourage interleaving if the lock failed to exclude.
                        thread::yield_now();
                        in_section.fetch_sub(1, Ordering::SeqCst);
                    }
                })
            })
            .collect();

        for h in handles {
            h.join().unwrap();
        }

        // If any real lock was taken, the section must never have had >1
        // concurrent holder.
        assert!(
            max_overlap.load(Ordering::SeqCst) <= 1,
            "HeaderLock allowed {} concurrent holders — serialization broken",
            max_overlap.load(Ordering::SeqCst)
        );

        let _ = std::fs::remove_file(&key);
    }
}
