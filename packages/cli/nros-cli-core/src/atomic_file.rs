//! Issue 0498 — temp-file + `rename(2)`, the write discipline for any file a
//! CONCURRENT process may read.
//!
//! # Why this is a module and not a habit
//!
//! `std::fs::write` truncates to zero and then fills. Between those two steps a
//! reader sees an EMPTY file, and an empty file is not a corrupt one — it parses
//! as "unexpected EOF at line 1 column 0", which reads like a producer bug in
//! whatever wrote it. `rename(2)` within one directory is atomic, so a reader
//! sees either the whole previous content or the whole new content, never the
//! gap.
//!
//! This project runs many `nros sync` processes at once — `build-test-fixtures`
//! fans out one per fixture row, and several rows of ONE leaf (its zenoh, xrce
//! and cyclonedds coordinates) sync the same directory. Any file keyed by
//! something coarser than the fixture coordinate is therefore contended by
//! construction.
//!
//! `cmd/ws.rs` already had this function, private, with a doc comment saying it
//! was "the write discipline every other sync-owned file here uses". It was not:
//! the source-metadata sidecar had three plain `fs::write` writers and died in a
//! `lane=native` sweep (issue 0498). A discipline that lives in one file's
//! private helper is a habit, and habits are what the sibling site does not
//! have. Hence one public helper, and a gate — `check-atomic-sync-writes` —
//! naming the paths that must go through it.
//!
//! Same defect, same remedy, one file over: issue 0494 (`lane-coords` written
//! with `>` while `ci-matrix` read it).

use std::path::Path;

use eyre::{Result, WrapErr};

/// Write `body` to `dst` so a concurrent reader never observes a partial file.
///
/// The temp file is a HIDDEN sibling in `dst`'s own directory — `rename(2)` is
/// only atomic within a filesystem, and a sibling is the cheap way to guarantee
/// that; `/tmp` may be a different mount and would silently degrade to a copy.
/// It carries the writing pid so two processes racing on the same `dst` cannot
/// clobber each other's temp; whichever renames last wins, and both contents
/// are complete.
pub fn atomic_write(dst: &Path, body: &str) -> Result<()> {
    atomic_write_bytes(dst, body.as_bytes())
}

/// [`atomic_write`] for content that is not valid UTF-8.
pub fn atomic_write_bytes(dst: &Path, body: &[u8]) -> Result<()> {
    let name = dst
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("nros-out");
    let tmp = dst.with_file_name(format!(".{name}.nros-tmp.{}", std::process::id()));
    std::fs::write(&tmp, body).wrap_err_with(|| format!("write {}", tmp.display()))?;
    std::fs::rename(&tmp, dst).wrap_err_with(|| {
        // Clean-up is best-effort: the rename failing usually means the
        // destination directory went away, and then so did the temp.
        let _ = std::fs::remove_file(&tmp);
        format!("rename {} -> {}", tmp.display(), dst.display())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The property the sidecar race needed: a reader either sees the OLD
    /// content or the NEW one. With `fs::write` the same sequence can expose a
    /// zero-length file; this asserts the replacement is complete.
    #[test]
    fn replaces_previous_content_wholesale() {
        let dir = tempfile::tempdir().unwrap();
        let dst = dir.path().join("sidecar.json");
        std::fs::write(&dst, "{\"old\":1}").unwrap();

        atomic_write(&dst, "{\"new\":2}").unwrap();

        assert_eq!(std::fs::read_to_string(&dst).unwrap(), "{\"new\":2}");
    }

    /// No temp file survives a successful write — a stray
    /// `.sidecar.json.nros-tmp.<pid>` in a leaf would be picked up by the
    /// `generated/`-adjacent globs and committed.
    #[test]
    fn leaves_no_temp_behind() {
        let dir = tempfile::tempdir().unwrap();
        atomic_write(&dir.path().join("f.json"), "{}").unwrap();

        let leftovers: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|n| n.contains("nros-tmp"))
            .collect();
        assert!(leftovers.is_empty(), "temp file left behind: {leftovers:?}");
    }

    /// Writing a file that does not exist yet is the common case (a first
    /// sync), and must not require the caller to pre-create it.
    #[test]
    fn creates_a_missing_destination() {
        let dir = tempfile::tempdir().unwrap();
        let dst = dir.path().join("fresh.json");
        atomic_write(&dst, "{}").unwrap();
        assert_eq!(std::fs::read_to_string(&dst).unwrap(), "{}");
    }
}
