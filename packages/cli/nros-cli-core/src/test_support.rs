//! Issue 0455 — the ONE way a test in this crate names a scratch directory.
//!
//! Before this module there were nine hand-written spellings of
//! `std::env::temp_dir().join(...)`, and the differences between them were the
//! bug. Two shared a base name and raced each other. Four keyed only on a
//! `{n}` counter, so two processes agreed on the path. Three keyed on a
//! nanosecond `{stamp}`, which two runs in the same tick still collide on. One
//! had no discriminator at all.
//!
//! The observed failure was `ETXTBSY`: a test writes an executable stub and
//! exec's it while a concurrent run truncates the same path. The quieter mode
//! hits every site — these helpers begin with `remove_dir_all`, so one run
//! deletes another's scratch mid-test. Because the panic names whichever verb
//! was running, it reads as a codegen regression rather than a harness defect,
//! which is what made it expensive to place.
//!
//! Uniqueness here does not depend on the clock. A path is
//! `<base>/nros-cli-core-tests-<pid>/<tag>-<seq>`: the pid separates processes,
//! and the process-wide `SEQ` separates calls within one. Two runs in the same
//! nanosecond are therefore fine, which is not true of a `{stamp}`.
//!
//! `CARGO_TARGET_TMPDIR` is honoured as the base when cargo sets it, but the
//! pid segment is appended EITHER WAY. The three call sites this replaces put
//! the pid only in the fallback — and cargo hands the same
//! `CARGO_TARGET_TMPDIR` to every run of a given test binary, so two concurrent
//! runs of one integration test would still have shared a path. That hole is
//! latent today only because `check-cli-tests` runs `--lib`, where the variable
//! is absent.
//!
//! Sweep for regressions of the class:
//!
//! ```text
//! git grep -n 'temp_dir()' -- packages/cli | grep -v 'process::id()'
//! ```

use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

/// Per-call discriminator, so one process's tests never collide with each
/// other even when they pass the same tag.
static SEQ: AtomicU64 = AtomicU64::new(0);

/// This PROCESS's scratch root. Everything a run creates lives under here, so
/// a `remove_dir_all` can only ever clear our own tree.
fn scratch_root() -> PathBuf {
    std::env::var_os("CARGO_TARGET_TMPDIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join(format!("nros-cli-core-tests-{}", std::process::id()))
}

/// A unique scratch path that does NOT exist. Nothing is created — for the
/// callers that want to hand a missing directory to the code under test.
pub(crate) fn scratch_path(tag: &str) -> PathBuf {
    let n = SEQ.fetch_add(1, Ordering::Relaxed);
    scratch_root().join(format!("{tag}-{n}"))
}

/// A unique, EMPTY scratch directory, created and ready to write into.
pub(crate) fn scratch_dir(tag: &str) -> PathBuf {
    let dir = scratch_path(tag);
    // Fresh by construction (SEQ never repeats), so this only matters if a
    // previous run of THIS pid left something behind — a pid the OS reused.
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir)
        .unwrap_or_else(|e| panic!("create scratch dir {}: {e}", dir.display()));
    dir
}

/// Write an EXECUTABLE stub (a fake `idlc`, a fake toolchain binary) that the
/// code under test will then spawn.
///
/// Issue 0476 — the obvious spelling races, and unique paths do not save it:
///
/// ```ignore
/// let mut f = File::create(&p)?;   // <- write descriptor, open in THIS process
/// f.write_all(script)?;
/// set_permissions(&p, 0o755)?;
/// drop(f);                         // closed here...
/// Command::new(&p).status()?;      // ...yet this can still fail ETXTBSY
/// ```
///
/// `O_CLOEXEC` (which Rust sets) closes a descriptor at **exec**, not at
/// **fork**. Between any other test thread's fork and its child's exec, that
/// child holds a copy of every descriptor open in this process — including the
/// write handle above. If our `execve` lands inside that window the kernel sees
/// a writer and returns `ETXTBSY` ("Text file busy"). Since these tests run as
/// THREADS under `cargo test --lib` and ~47 sites spawn processes, the window
/// is open constantly.
///
/// Measured on this machine, unique path per write, 12 concurrently-spawning
/// threads: **245 of 1200 execs failed**. With this helper: **0**.
///
/// The fix is to never hold the descriptor: `cp` and `chmod` run as CHILD
/// processes, so the only write handle on the stub lives in a process our forks
/// do not copy from, and it is gone before the stub is ever executed. The
/// intermediate source file is written normally — it is never executed, so its
/// descriptor is harmless.
///
/// A retry-on-`ETXTBSY` loop also works (0 escapes, 141 backoffs in the same
/// experiment) but masks the race instead of removing it, and pays latency on
/// every hit.
pub(crate) fn write_executable_stub(path: &std::path::Path, script: &str) {
    let src = path.with_extension("stub-src");
    std::fs::write(&src, script)
        .unwrap_or_else(|e| panic!("write stub source {}: {e}", src.display()));

    let ok = std::process::Command::new("cp")
        .arg(&src)
        .arg(path)
        .status()
        .unwrap_or_else(|e| panic!("spawn cp for {}: {e}", path.display()))
        .success();
    assert!(ok, "cp failed writing stub {}", path.display());

    let ok = std::process::Command::new("chmod")
        .arg("755")
        .arg(path)
        .status()
        .unwrap_or_else(|e| panic!("spawn chmod for {}: {e}", path.display()))
        .success();
    assert!(ok, "chmod failed on stub {}", path.display());

    let _ = std::fs::remove_file(&src);
}

/// Scope model discovery to the fixture under test, once per test process.
///
/// `model_search_paths` consults ambient `$OUT_DIR`, which is right when the
/// caller IS the build script of the crate whose model is being resolved — the
/// zephyr module and the pio extra_script both shell `codegen system` that way.
/// It is wrong in a test: this crate has a build script, so the test process
/// inherits an `OUT_DIR` belonging to a DIFFERENT crate, and the build-output
/// candidate is keyed on the bringup's directory NAME. Every fixture here calls
/// its bringup `demo_bringup`, as does whatever last generated into that
/// directory, so discovery matched across two unrelated workspaces and loaded a
/// stale model — three `codegen_system` unit tests failed on it, one asserting
/// the wrong provenance and two ingesting components this fixture never had.
///
/// Pointing `OUT_DIR` at an empty per-process directory removes the collision
/// without changing the search ORDER these tests exercise.
///
/// Same reasoning as this module's own: one spelling, because the differences
/// between hand-written ones were the bug (issue 0455).
pub fn isolate_model_discovery() {
    use std::sync::Once;
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        let dir = std::env::temp_dir().join(format!("nros-cli-core-outdir-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("scratch OUT_DIR");
        // SAFETY: once, before any test body reads the environment; every
        // reader is this process's own model resolution.
        unsafe {
            std::env::set_var("OUT_DIR", &dir);
            std::env::remove_var("NROS_MODEL_DIR");
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The property the issue turns on: same tag, different path.
    #[test]
    fn same_tag_twice_yields_distinct_dirs() {
        let a = scratch_dir("dup");
        let b = scratch_dir("dup");
        assert_ne!(a, b, "two calls with one tag must not share a directory");
        assert!(a.is_dir() && b.is_dir());
    }

    /// The pid segment is what separates concurrent processes, and it must be
    /// present whether or not cargo set `CARGO_TARGET_TMPDIR`.
    #[test]
    fn path_is_scoped_to_this_process() {
        let pid = std::process::id().to_string();
        let p = scratch_path("scoped");
        assert!(
            p.components()
                .any(|c| c.as_os_str().to_string_lossy().contains(&pid)),
            "scratch path {} carries no pid segment",
            p.display()
        );
    }

    /// `scratch_path` promises a path that is not there yet.
    #[test]
    fn scratch_path_does_not_create() {
        assert!(!scratch_path("absent").exists());
    }

    /// Issue 0476 — a stub written this way survives being exec'd while
    /// sibling threads fork constantly.
    ///
    /// The loop is the test: writing the stub with `File::create` instead makes
    /// this fail within a few iterations on a loaded machine (measured ~20% of
    /// execs), because a concurrent fork inherits the still-open write
    /// descriptor. Unique paths do NOT prevent it — every iteration below uses
    /// a fresh one.
    #[test]
    fn executable_stub_survives_concurrent_forks() {
        let dir = scratch_dir("etxtbsy");
        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));

        let noise: Vec<_> = (0..4)
            .map(|_| {
                let stop = stop.clone();
                std::thread::spawn(move || {
                    while !stop.load(Ordering::Relaxed) {
                        let _ = std::process::Command::new("/bin/true").status();
                    }
                })
            })
            .collect();

        let mut failures = Vec::new();
        for i in 0..40 {
            let stub = dir.join(format!("stub-{i}"));
            write_executable_stub(&stub, "#!/bin/sh\nexit 0\n");
            match std::process::Command::new(&stub).status() {
                Ok(s) if s.success() => {}
                Ok(s) => failures.push(format!("iteration {i}: stub exited {s}")),
                Err(e) => failures.push(format!("iteration {i}: spawn failed: {e}")),
            }
        }

        stop.store(true, Ordering::Relaxed);
        for h in noise {
            let _ = h.join();
        }

        assert!(
            failures.is_empty(),
            "a stub written by `write_executable_stub` must always be \
             executable, even while sibling threads fork (issue 0476):\n{}",
            failures.join("\n")
        );
    }
}
