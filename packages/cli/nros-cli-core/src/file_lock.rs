//! One exclusive advisory file lock for the CLI (phase-530 W1).
//!
//! `provision_lock` (sdk_store) and `nros configure` both need a blocking
//! `flock`; this is the one spelling.

use std::{fs::File, io, path::Path};

use eyre::{Result, WrapErr};

/// An exclusive lock on a file, held until this value drops (the fd closes).
// Unused until `nros configure` (phase-530 W1 later tasks) takes the lock.
#[allow(dead_code)]
#[derive(Debug)]
pub(crate) struct ProjectLock(File); // held for its Drop

// Unused until `nros configure` (phase-530 W1 later tasks) takes the lock.
#[allow(dead_code)]
/// Create `path` (and its parent dirs) and block until we hold `LOCK_EX`.
pub(crate) fn lock_exclusive_path(path: &Path) -> Result<ProjectLock> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .wrap_err_with(|| format!("create lock dir {}", parent.display()))?;
    }
    let file = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(path)
        .wrap_err_with(|| format!("open lock {}", path.display()))?;
    lock_exclusive(&file).wrap_err_with(|| format!("lock {}", path.display()))?;
    Ok(ProjectLock(file))
}

/// A blocking exclusive `flock`, released when `file` closes. `File::lock`
/// is the std spelling, but it is stable only since 1.89 and this crate's
/// MSRV is older. Retries EINTR, which a blocking `flock` can return.
#[cfg(unix)]
pub(crate) fn lock_exclusive(file: &File) -> io::Result<()> {
    use std::os::fd::AsRawFd;
    loop {
        // SAFETY: a valid, open fd for the life of the call; flock touches no memory.
        if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX) } == 0 {
            return Ok(());
        }
        let err = io::Error::last_os_error();
        if err.kind() != io::ErrorKind::Interrupted {
            return Err(err);
        }
    }
}

#[cfg(not(unix))]
pub(crate) fn lock_exclusive(_file: &File) -> io::Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::{
        sync::{Arc, Barrier},
        thread,
        time::{Duration, Instant},
    };

    use super::*;

    #[test]
    fn second_locker_waits_for_first() {
        let dir = crate::test_support::scratch_dir("file_lock_wait");
        let path = dir.join("nested").join("x.lock");
        let start = Arc::new(Barrier::new(2));
        let (p, b) = (path.clone(), start.clone());
        let a = thread::spawn(move || {
            let guard = lock_exclusive_path(&p).unwrap();
            let got = Instant::now();
            b.wait();
            thread::sleep(Duration::from_millis(300));
            drop(guard);
            got
        });
        start.wait();
        thread::sleep(Duration::from_millis(50));
        let guard = lock_exclusive_path(&path).unwrap();
        let b_got = Instant::now();
        drop(guard);
        let a_got = a.join().unwrap();
        assert!(
            b_got.duration_since(a_got) >= Duration::from_millis(250),
            "B got the lock {:?} after A",
            b_got.duration_since(a_got)
        );
    }
}
