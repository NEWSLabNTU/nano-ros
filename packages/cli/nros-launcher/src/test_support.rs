//! Helpers for tests that install an executable and then RUN it — issue 0476.
//!
//! ## Why this lives in the launcher crate
//!
//! Three test files build a fake store and exec what they put in it:
//! `nros-launcher/tests/launcher_binary.rs`,
//! `nros-cli/tests/toolchain_dispatch.rs`, and whatever the next launcher test
//! is. They need ONE spelling of "put an executable here safely", and this is
//! the lowest crate all of them can reach: `nros-cli-core` depends on
//! `nros-launcher`, so a helper there would be a dependency cycle for the
//! launcher's own tests.
//!
//! It is `#[doc(hidden)]` and two `Command` spawns; it costs the shipped binary
//! nothing worth measuring, and the alternative — a `test-support` feature that
//! only tests enable — is one more thing that can be silently off.
//!
//! ## The hazard
//!
//! ```ignore
//! std::fs::copy(src, &dst)?;              // write descriptor, open HERE
//! std::process::Command::new(&dst).status()?;   // ...can still be ETXTBSY
//! ```
//!
//! `O_CLOEXEC` (which Rust sets) closes a descriptor at **exec**, not at
//! **fork**. Between any sibling test thread's fork and its child's exec, that
//! child holds a copy of every descriptor open in this process — including the
//! write handle above. An `execve` landing in that window sees a writer and the
//! kernel returns `ETXTBSY` ("Text file busy"). Tests run as THREADS in one
//! process and this repository's test suites spawn constantly, so the window is
//! open all the time.
//!
//! Measured while finishing phase-443 W3: `nros-cli`'s
//! `an_unpinned_project_runs_in_the_launcher` failed 1 of 4 full
//! `packages/cli` runs on this machine and passed 4 of 4 solo — the shape that
//! reads as "someone else's flake" for as long as nobody looks.
//!
//! The fix is to never hold the descriptor: `cp` and `chmod` run as CHILD
//! processes, so the only write handle lives in a process our forks do not copy
//! from, and it is gone before the file is ever executed. A retry loop also
//! works and masks the race instead of removing it.

use std::path::Path;

fn must(program: &str, args: &[&std::ffi::OsStr], what: &str) {
    let ok = std::process::Command::new(program)
        .args(args)
        .status()
        .unwrap_or_else(|e| panic!("spawn {program} for {what}: {e}"))
        .success();
    assert!(ok, "{program} failed: {what}");
}

/// Copy an existing binary to `dst` and make it executable, without holding a
/// write descriptor in this process.
///
/// `cp` preserves the mode of a source that is already executable; the explicit
/// `chmod` is for a source whose mode does not survive (a `CARGO_BIN_EXE_*` on
/// a filesystem that drops the bit).
pub fn copy_executable(src: &Path, dst: &Path) {
    let what = format!("{} -> {}", src.display(), dst.display());
    must("cp", &[src.as_os_str(), dst.as_os_str()], &what);
    must(
        "chmod",
        [std::ffi::OsStr::new("755"), dst.as_os_str()].as_slice(),
        &what,
    );
}

/// Write a shell-script stub at `path` and make it executable.
///
/// The script text goes to a sidecar first. That file is never executed, so its
/// descriptor is harmless; only the `cp`'d copy is ever run.
pub fn write_executable_stub(path: &Path, script: &str) {
    let src = path.with_extension("stub-src");
    std::fs::write(&src, script).unwrap_or_else(|e| panic!("write {}: {e}", src.display()));
    copy_executable(&src, path);
    let _ = std::fs::remove_file(&src);
}
