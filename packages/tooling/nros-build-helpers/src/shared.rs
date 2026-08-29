use std::{
    collections::HashMap,
    env,
    path::{Path, PathBuf},
};

pub type SizeMap = HashMap<String, u64>;

/// Identity of the artifact the sizes were probed from, for
/// [`write_header_if_absent_or_verify`]'s stale-vs-divergent question.
///
/// The two fields carry different information, and the verify step needs both:
///
/// * the **path** says WHICH probe configuration produced the sizes.
///   `nros-sizes-build` keys its probe directory by (rustc, target, features),
///   so two crates that resolved different feature sets probe different paths —
///   that is divergence, not staleness.
/// * the **mtime/len** say WHEN that same configuration was last built. Same
///   path, different mtime ⇒ a header from an earlier build of this very
///   configuration ⇒ staleness.
///
/// Path + mtime + length rather than a content hash: the rlib is tens of
/// megabytes and this runs in every build script, while the triple already
/// changes whenever cargo relinks. Returns `None` when the probe itself could
/// not find the rlib — the caller then treats the stamp as unknown, which is
/// the conservative direction (it cannot claim a mismatch is stale).
fn probe_artifact_stamp() -> Option<String> {
    let rlib = nros_sizes_build::find_dep_rlib("nros", "__NROS_SIZE_").ok()?;
    let meta = std::fs::metadata(&rlib).ok()?;
    let mtime = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    Some(format!("{} {} {}", rlib.display(), mtime, meta.len()))
}

pub fn probe_nros_sizes(crate_label: &str) -> SizeMap {
    let rlib = match nros_sizes_build::find_dep_rlib("nros", "__NROS_SIZE_") {
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
    build.compile(lib_name);
}

pub fn apply_baremetal_libc(build: &mut cc::Build) {
    let arch = env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default();
    let os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if arch != "riscv64" || os != "none" {
        return;
    }
    // issue 0491 — `NROS_PICOLIBC_SYSROOT` names a DIRECTORY, so it is watched
    // by CONTENT rather than fingerprinted as a string: cargo compares an env
    // value textually, and one directory reaches this script with a different
    // spelling from `just`, from a leaf `.cargo/config.toml [env]`, and unset.
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
/// Split out of [`generate_cbindgen_header`] so the regenerator binary and the
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
fn pid_is_live(pid: u32) -> bool {
    #[cfg(unix)]
    {
        Path::new(&format!("/proc/{pid}")).exists()
    }
    #[cfg(not(unix))]
    {
        let _ = pid;
        true
    }
}

pub fn write_header_to_target_dir(relative: &[&str], contents: &str) {
    let root = if let Ok(target_dir) = env::var("CARGO_TARGET_DIR") {
        Some(PathBuf::from(target_dir))
    } else {
        nros_sizes_build::cargo_target_dir().ok()
    };
    let Some(root) = root else { return };
    write_to(root, relative, contents);

    // The OWNER must stamp too, or the check it owns goes toothless. This is
    // the path `nros-c` takes; `write_header_if_absent_or_verify` is the
    // verifier's. If only the verifier stamped, every subsequent build would
    // read `None` from disk, conclude "cannot prove same build", and treat a
    // genuine divergence as staleness — silently overwriting exactly the case
    // the panic exists to catch. Stamping both writers keeps "same artifact"
    // answerable no matter which crate got there first.
    if let Some(dest) = target_dir_path(relative) {
        write_stamp(
            &dest.with_extension("h.stamp"),
            probe_artifact_stamp().as_deref(),
        );
    }
}

fn write_to(root: PathBuf, relative: &[&str], contents: &str) {
    let mut dest = root;
    for segment in relative {
        dest.push(segment);
    }
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).expect("create per-build header dir");
    }
    write_atomic(&dest, contents);
}

/// Write a header only when nobody else has, and FAIL if someone else wrote a
/// different one (phase-308 follow-up).
///
/// `nros_config_generated.h` has two potential writers: `nros-c`'s build script
/// owns it, and `nros-cpp`'s emits the same file so a build without `nros-c`
/// still finds it rather than the source-tree `#error` stub. Two unconditional
/// writers to one path is the race that produced a torn header on 2026-07-26;
/// making both writes atomic removed the tearing but left the deeper problem —
/// the values are probed from each crate's own view of the runtime, and the
/// claim that they always agree was a comment, not a check. If they ever
/// disagreed, the last writer won and one language silently got wrong
/// `_opaque` sizes.
///
/// So: the owner writes; this writes only into a GAP, and otherwise verifies.
///
/// **A mismatch is one of two very different things, and this used to conflate
/// them.** The original reasoning was: `nros-c` is a non-optional dependency of
/// `nros-cpp`, so cargo runs the owner's build script first and any file present
/// is already current — therefore a difference means divergent features. That
/// confuses ORDER with EXECUTION. Cargo only *re-runs* a build script when the
/// crate's fingerprint is dirty; a fresh `nros-c` is skipped entirely, its
/// header from an earlier build stays in the (long-lived, per-example) target
/// dir, and a re-probing `nros-cpp` then compares against a file from an older
/// source state. Observed 2026-08-06 on the freertos fixtures: the guard
/// reported "different features" for a tree that had none — every half probed
/// 11025 once the build dirs were wiped, against 10940 on disk.
///
/// So ask the question directly, via the identity of the rlib the sizes came
/// from ([`probe_artifact_stamp`]):
///
/// * values agree → nothing to do;
/// * values differ, stamps differ (or either is unknown) → the file predates
///   this build. Overwrite it; that is plain staleness, not a defect;
/// * values differ, stamps MATCH → both halves read the same artifact and still
///   resolved different layouts. That is the real divergent-features case, and
///   it must stop the build rather than produce an image whose C and C++ halves
///   disagree about a struct's size.
pub fn write_header_if_absent_or_verify(relative: &[&str], contents: &str, label: &str) {
    let Some(dest) = target_dir_path(relative) else {
        return;
    };
    // Sidecar rather than a `#define` inside the header: the two writers build
    // `contents` from different sources (nros-c from a `.template`, nros-cpp
    // from an inline string), so a stamp define would have to be added to both
    // and kept in step — the same duplication that produced this bug. A sidecar
    // keeps the change in this one function.
    let stamp_path = dest.with_extension("h.stamp");
    let current_stamp = probe_artifact_stamp();
    match std::fs::read_to_string(&dest) {
        // Compare the VALUES, not the bytes. The two writers build this header
        // from different sources — nros-c from
        // `templates/nros_config_generated.h.template`, nros-cpp from an inline
        // string — so comments and spacing differ even when every probed size
        // agrees. A byte comparison called that divergence and broke EVERY
        // native C++ build; only a disagreeing `#define` is a real problem.
        Ok(existing) if defines_of(&existing) == defines_of(contents) => {}
        Ok(existing) => {
            let on_disk_stamp = std::fs::read_to_string(&stamp_path).ok();
            if !stamps_prove_staleness(on_disk_stamp.as_deref(), current_stamp.as_deref()) {
                panic!(
                    "{label}: {} was written by another crate with DIFFERENT probed sizes.\n\
                     The C and C++ halves of this build resolved different runtime layouts, \
                     so one of them would size its `_opaque` storage wrong (silent overflow \
                     at runtime). Usually this means nros-c and nros-cpp were built with \
                     different features — `nros-sizes-build` keys its probe directory by \
                     (rustc, target, features), so check whether the two probed different \
                     rlibs:\n  on disk: {}\n  current: {}\nDisagreeing defines:\n{}",
                    dest.display(),
                    on_disk_stamp.as_deref().unwrap_or("<no stamp>"),
                    current_stamp.as_deref().unwrap_or("<unknown>"),
                    disagreeing_defines(&existing, contents),
                );
            }
            // Stale: SAME probe artifact path, rebuilt since — a header left by
            // an earlier build of this same configuration.
            write_to_path(&dest, contents);
            write_stamp(&stamp_path, current_stamp.as_deref());
        }
        Err(_) => {
            write_to_path(&dest, contents);
            write_stamp(&stamp_path, current_stamp.as_deref());
        }
    }
}

/// Do the two stamps prove the value mismatch is mere STALENESS?
///
/// Only one shape does: the same probe artifact PATH with a different
/// mtime/length — i.e. the same crate, same target, same features, rebuilt
/// since. That is a header left over from an earlier build.
///
/// A different PATH is the opposite conclusion, and getting this backwards was
/// the first version of this function. `nros-sizes-build` keys the probe
/// directory by `(rustc, target, features)`, so two crates that resolved
/// DIFFERENT feature sets probe different rlibs — which is exactly the
/// divergence the guard exists to catch. phase-336 W7 records it happening for
/// real: `nros-c` and `nros-cpp` resolved `EXECUTOR_SIZE` 88680 vs 89392 in one
/// workspace. Treating "paths differ" as staleness would overwrite that and
/// restore the silent last-writer-wins this whole mechanism replaced.
///
/// Unknown on either side proves nothing and must NOT be read as staleness, for
/// the same reason: the unprovable case has to keep the guard's teeth.
fn stamps_prove_staleness(on_disk: Option<&str>, current: Option<&str>) -> bool {
    let (Some(a), Some(b)) = (on_disk, current) else {
        return false;
    };
    let (Some((path_a, rest_a)), Some((path_b, rest_b))) =
        (split_stamp_path(a), split_stamp_path(b))
    else {
        return false;
    };
    // Same artifact identity entirely: not stale, and the values still differ —
    // that is a genuine disagreement from one rlib.
    path_a == path_b && rest_a != rest_b
}

/// Split `"<path> <mtime> <len>"` into the path and the rest.
///
/// The path may contain spaces, so split from the RIGHT: the last two fields
/// are always the mtime and the length.
fn split_stamp_path(stamp: &str) -> Option<(&str, &str)> {
    let (head, len) = stamp.rsplit_once(' ')?;
    let (path, mtime) = head.rsplit_once(' ')?;
    let _ = (len, mtime);
    Some((path, &stamp[path.len() + 1..]))
}

/// Record which artifact the freshly-written header was probed from. Best
/// effort: a missing stamp only costs the next build the ability to prove
/// sameness, and that path is already handled as "assume stale".
fn write_stamp(path: &Path, stamp: Option<&str>) {
    if let Some(stamp) = stamp {
        let _ = std::fs::write(path, stamp);
    } else {
        let _ = std::fs::remove_file(path);
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
            Some((_, vb)) => lines.push(format!("  {name}: on-disk={va} vs would-write={vb}")),
            None => lines.push(format!("  {name}: on-disk={va} vs would-write=<absent>")),
        }
    }
    for (name, vb) in &db {
        if !da.iter().any(|(n, _)| n == name) {
            lines.push(format!("  {name}: on-disk=<absent> vs would-write={vb}"));
        }
    }
    lines.join("\n")
}

fn target_dir_path(relative: &[&str]) -> Option<PathBuf> {
    let root = if let Ok(d) = env::var("CARGO_TARGET_DIR") {
        PathBuf::from(d)
    } else {
        nros_sizes_build::cargo_target_dir().ok()?
    };
    let mut dest = root;
    for segment in relative {
        dest.push(segment);
    }
    Some(dest)
}

fn write_to_path(dest: &std::path::Path, contents: &str) {
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).expect("create per-build header dir");
    }
    write_atomic(dest, contents);
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
/// `nros-c-generated/nros/nros_config_generated.h`, and `nros-cpp`'s emits the
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

pub fn env_usize(name: &str, default: usize) -> usize {
    println!("cargo:rerun-if-env-changed={name}");
    env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
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

    /// The stale-vs-divergent question `write_header_if_absent_or_verify` asks
    /// when the probed sizes disagree.
    ///
    /// ONLY "same probe-artifact path, rebuilt since" is staleness. Everything
    /// else keeps the panic, and the direction matters more than it looks:
    /// `nros-sizes-build` keys the probe dir by (rustc, target, features), so
    /// two crates with DIFFERENT features probe different PATHS — which is the
    /// divergence the guard exists for (phase-336 W7 records it happening:
    /// `EXECUTOR_SIZE` 88680 vs 89392 in one workspace). Reading "paths differ"
    /// as staleness would overwrite that and restore the silent
    /// last-writer-wins this mechanism replaced.
    #[test]
    fn only_same_path_rebuilt_counts_as_stale() {
        // Same artifact, rebuilt since: staleness.
        assert!(stamps_prove_staleness(
            Some("/p/nros.rlib 41 100"),
            Some("/p/nros.rlib 42 108")
        ));

        // DIFFERENT probe path = different (rustc, target, features) key. The
        // divergence case; must NOT be called stale.
        assert!(!stamps_prove_staleness(
            Some("/p/featA/nros.rlib 41 100"),
            Some("/p/featB/nros.rlib 42 108")
        ));

        // Identical stamp: one artifact, values still disagree — genuine.
        assert!(!stamps_prove_staleness(
            Some("/p/nros.rlib 42 100"),
            Some("/p/nros.rlib 42 100")
        ));

        // Unknown proves nothing; keep the guard's teeth.
        assert!(!stamps_prove_staleness(None, Some("/p/nros.rlib 42 100")));
        assert!(!stamps_prove_staleness(Some("/p/nros.rlib 42 100"), None));
        assert!(!stamps_prove_staleness(None, None));
    }

    /// Probe paths can contain spaces, so the mtime/len split works from the
    /// RIGHT. Splitting from the left would treat the first path segment as the
    /// whole path and call two different artifacts the same one.
    #[test]
    fn stamp_split_tolerates_spaces_in_the_path() {
        let (path, rest) = split_stamp_path("/a b/c d/nros.rlib 42 100").unwrap();
        assert_eq!(path, "/a b/c d/nros.rlib");
        assert_eq!(rest, "42 100");
        assert!(split_stamp_path("no-fields").is_none());
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
