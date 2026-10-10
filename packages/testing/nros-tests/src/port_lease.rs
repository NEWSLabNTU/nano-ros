//! Issue 0470 — a port is not yours because the OS just told you it was free.
//!
//! # The defect this replaces
//!
//! Both fixture allocators asked the kernel for an ephemeral port like this:
//!
//! ```ignore
//! let sock = UdpSocket::bind("127.0.0.1:0")?;   // OS picks a free port
//! let port = sock.local_addr()?.port();
//! drop(sock);                                   // ...and it is free again
//! Ok(port)
//! ```
//!
//! Between the `drop` and the agent/router actually binding, the port belongs to
//! nobody, and the kernel will hand the SAME number to the next caller that asks.
//! Measured on this repo's host: 2400 allocations across 12 concurrent processes
//! produced **87 colliding ports**, several handed out three times.
//!
//! nextest runs each test in its own PROCESS, so an in-process registry cannot
//! see the collision — which is why the previous comment ("safe for nextest where
//! each test runs in a separate process") drew the wrong conclusion from a true
//! premise. Separate processes are exactly why a SHARED, on-disk reservation is
//! needed.
//!
//! # How it failed, which is why it took a sweep to see
//!
//! Two "unique" XRCE agents on one UDP port means two unrelated tests' clients
//! dial the same agent. `large_msg::test_xrce_e2e_integrity` publishes 512-byte
//! payloads and validates what it receives; a neighbour publishing 64-byte ones
//! landed in its subscription:
//!
//! ```text
//! Received: seq=0 size=64  valid=false      <- the neighbour's traffic
//! Received: seq=0 size=512 valid=true       <- this test's own, always fine
//! ```
//!
//! Delivered-but-wrong rather than absent, which is why it read as data
//! corruption rather than as cross-talk. Every one of the test's OWN samples was
//! valid in every observed failure — the signal that the payload path was never
//! the problem.
//!
//! # The reservation
//!
//! A lock file per port under the build root, held with an exclusive `flock`
//! for the lifetime of the [`PortLease`] and removed on drop. It does not stop
//! an unrelated program from taking the port — nothing can — but every
//! colliding party here is one of our own fixtures, and that is the population
//! it makes exclusive.
//!
//! The reservation is the LOCK, not the file. nextest SIGKILLs test processes,
//! so `Drop` is not guaranteed to run; the kernel drops a dead process's
//! `flock`, so a leaked file reserves nothing and the next claimer simply takes
//! it. This is the same hazard `zenohd_router::kill_listeners_on_port` handles
//! for the router, one layer earlier.
//!
//! # Why not a pid in an `O_EXCL` file (the first version)
//!
//! That design reclaimed a lease when the pid it recorded was dead, and it had
//! two races, both of which hand ONE port to TWO fixtures:
//!
//! * the file was created and the pid written in two steps, so a concurrent
//!   claimer could read it EMPTY, take "no pid" for "dead owner", delete the
//!   live lease and claim it. Seen as `concurrent_leases_are_distinct` failing
//!   once inside a loaded `test-lane-contracts` run, then passing solo (1 in
//!   640 parallel runs when reproduced);
//! * two reclaimers that both saw a dead owner could interleave
//!   remove/create, and the slower one deleted the faster one's FRESH lease.
//!
//! An `flock` has neither: there is no window where the file exists without its
//! owner holding it, and no liveness question to answer.

use std::{
    io,
    path::{Path, PathBuf},
};

/// How many ports to try before giving up. Each attempt is a fresh kernel
/// suggestion, so exhausting this means the lease directory is badly wedged
/// rather than that the machine ran out of ports.
const MAX_ATTEMPTS: usize = 64;

/// An exclusive claim on a localhost port, released on drop.
#[derive(Debug)]
pub struct PortLease {
    port: u16,
    lock: PathBuf,
    /// The open lock file; holding it IS the reservation (the `flock` lives as
    /// long as this descriptor).
    _held: std::fs::File,
}

impl PortLease {
    /// The reserved port.
    pub fn port(&self) -> u16 {
        self.port
    }
}

impl Drop for PortLease {
    fn drop(&mut self) {
        // Unlink WHILE still holding the lock, then the descriptor closes. A
        // claimer that opened this path before the unlink and wins the lock
        // after it finds the path no longer names its inode, and retries
        // (`claim`'s identity check). A file we fail to remove holds no lock,
        // so it costs nothing.
        let _ = std::fs::remove_file(&self.lock);
    }
}

/// Transport the port belongs to. UDP and TCP port spaces are independent, so
/// they get independent lease namespaces — reserving UDP 40000 must not stop a
/// router taking TCP 40000.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Transport {
    Udp,
    Tcp,
}

impl Transport {
    fn tag(self) -> &'static str {
        match self {
            Transport::Udp => "udp",
            Transport::Tcp => "tcp",
        }
    }

    /// Ask the kernel for a free port of this kind. The socket is closed before
    /// returning — that is the race this module exists to close, and it is
    /// closed by the LEASE, not by holding this socket (the agent must be able
    /// to bind the port itself).
    fn suggest(self) -> io::Result<u16> {
        match self {
            Transport::Udp => {
                let s = std::net::UdpSocket::bind("127.0.0.1:0")?;
                s.local_addr().map(|a| a.port())
            }
            Transport::Tcp => {
                let l = std::net::TcpListener::bind("127.0.0.1:0")?;
                l.local_addr().map(|a| a.port())
            }
        }
    }
}

fn lease_dir() -> PathBuf {
    crate::build_root().join("nros-tests/port-leases")
}

/// Reserve a localhost port of `transport`, exclusive against every other
/// fixture in this repo until the returned lease is dropped.
pub fn lease_port(transport: Transport) -> io::Result<PortLease> {
    let dir = lease_dir();
    std::fs::create_dir_all(&dir)?;
    let mut last_err = None;
    for _ in 0..MAX_ATTEMPTS {
        let port = match transport.suggest() {
            Ok(p) => p,
            Err(e) => {
                last_err = Some(e);
                continue;
            }
        };
        let lock = dir.join(format!("{}-{port}.lock", transport.tag()));
        if let Some(held) = claim(&lock)? {
            return Ok(PortLease {
                port,
                lock,
                _held: held,
            });
        }
    }
    Err(last_err.unwrap_or_else(|| {
        io::Error::other(format!(
            "could not lease a free {} port after {MAX_ATTEMPTS} attempts (lease dir: {})",
            transport.tag(),
            lease_dir().display(),
        ))
    }))
}

/// Try to take `lock`. `Some(file)` means the caller now owns it for as long as
/// it keeps `file` open; `None` means someone else holds it.
#[cfg(unix)]
fn claim(lock: &Path) -> io::Result<Option<std::fs::File>> {
    let f = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(lock)?;
    lock_if_current(f, lock)
}

/// Lock `f`, an open descriptor for `lock`, and keep it only if `lock` still
/// names that inode. Split from [`claim`] so the race below can be tested with
/// a descriptor opened BEFORE the previous owner's unlink.
#[cfg(unix)]
fn lock_if_current(mut f: std::fs::File, lock: &Path) -> io::Result<Option<std::fs::File>> {
    use std::{
        io::Write,
        os::unix::{fs::MetadataExt, io::AsRawFd},
    };
    // SAFETY: `flock` on a descriptor we own; LOCK_NB makes it never block.
    if unsafe { libc::flock(f.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
        let e = io::Error::last_os_error();
        return match e.raw_os_error() {
            Some(libc::EWOULDBLOCK) => Ok(None),
            _ => Err(e),
        };
    }
    // The path must still name the inode we locked: the previous owner unlinks
    // before it closes, so a descriptor opened before that unlink can win the
    // lock on a file nobody else will ever look at again — while a third
    // claimer creates and locks a NEW file at the same path. Both would then
    // "own" the port.
    let ours = f.metadata()?;
    match std::fs::metadata(lock) {
        Ok(now) if now.dev() == ours.dev() && now.ino() == ours.ino() => {}
        Ok(_) => return Ok(None),
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e),
    }
    // Diagnostic only — the lock is the reservation, not this text.
    let _ = f.set_len(0);
    let _ = write!(f, "{}", std::process::id());
    Ok(Some(f))
}

/// No `flock` here: exclusive creation, never reclaimed. A leaked file costs a
/// retry on another port (MAX_ATTEMPTS), never a shared one.
#[cfg(not(unix))]
fn claim(lock: &Path) -> io::Result<Option<std::fs::File>> {
    match std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(lock)
    {
        Ok(f) => Ok(Some(f)),
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => Ok(None),
        Err(e) => Err(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The property the whole module exists for: two leases never name one port.
    #[test]
    fn concurrent_leases_are_distinct() {
        let handles: Vec<_> = (0..8)
            .map(|_| {
                std::thread::spawn(|| {
                    (0..25)
                        .map(|_| lease_port(Transport::Udp).expect("lease"))
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        let leases: Vec<PortLease> = handles
            .into_iter()
            .flat_map(|h| h.join().expect("thread"))
            .collect();

        let mut ports: Vec<u16> = leases.iter().map(|l| l.port()).collect();
        let total = ports.len();
        ports.sort_unstable();
        ports.dedup();
        assert_eq!(
            ports.len(),
            total,
            "two leases returned the same port — the reservation is not exclusive"
        );
    }

    fn probe(name: &str) -> PathBuf {
        let dir = lease_dir();
        std::fs::create_dir_all(&dir).expect("lease dir");
        let lock = dir.join(format!("udp-{name}-{}.lock", std::process::id()));
        let _ = std::fs::remove_file(&lock);
        lock
    }

    /// The first design's race, deterministically: a lock file that exists but
    /// is EMPTY is a claim in progress (created, pid not yet written). It must
    /// not be taken from its holder. The pid design read "no pid" as "dead
    /// owner" and stole it.
    #[cfg(unix)]
    #[test]
    fn a_held_lease_with_no_pid_yet_is_not_stolen() {
        let lock = probe("in-progress");
        let holder = claim(&lock).expect("claim").expect("first claim wins");
        holder
            .set_len(0)
            .expect("empty it, as a claim mid-write looks");
        assert!(
            claim(&lock).expect("claim").is_none(),
            "an empty lock file held by a live claimer was handed out again"
        );
        drop(holder);
        let _ = std::fs::remove_file(&lock);
    }

    /// Dropping a lease frees the port for the next caller, or a long sweep
    /// would exhaust the ephemeral range.
    #[test]
    fn dropping_a_lease_releases_it() {
        let lock = {
            let l = lease_port(Transport::Udp).expect("lease");
            l.lock.clone()
        };
        assert!(!lock.exists(), "lease file outlived its lease");
        let again = claim(&lock).expect("claim");
        assert!(again.is_some(), "a released port could not be re-leased");
        let _ = std::fs::remove_file(&lock);
    }

    /// A lease whose owner died must not reserve the port forever — nextest
    /// SIGKILLs test processes, so leaked files are expected, not exceptional.
    /// A file left behind with no lock holder (any content) is free.
    #[test]
    fn a_dead_owners_lease_is_reclaimed() {
        let lock = probe("reclaim");
        std::fs::write(&lock, "0").expect("write a leaked lease");

        let held = claim(&lock).expect("claim");
        assert!(held.is_some(), "a leaked, unheld lease was not reclaimed");
        // ...and now that WE hold it, a second claim fails.
        assert!(
            claim(&lock).expect("claim").is_none(),
            "a live lease was handed out twice"
        );
        drop(held);
        let _ = std::fs::remove_file(&lock);
    }

    /// The drop-time unlink race: a descriptor opened before the owner's unlink
    /// can win the lock afterwards on an orphaned inode. It must report "not
    /// ours", since a fresh file at the path may already be held by someone else.
    #[cfg(unix)]
    #[test]
    fn a_lock_won_on_an_unlinked_file_is_not_ownership() {
        let lock = probe("unlinked");
        let first = claim(&lock).expect("claim").expect("first claim wins");
        // A racer opens the path while `first` still holds it...
        let early = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&lock)
            .expect("open before the unlink");
        // ...then `first`'s Drop unlinks and closes, releasing the lock...
        std::fs::remove_file(&lock).expect("unlink as Drop does");
        drop(first);
        // ...and a third claimer takes the NEW file at the path.
        let fresh = claim(&lock)
            .expect("claim")
            .expect("a new file at the path is free");
        // The racer's flock on the orphaned inode now SUCCEEDS — it must not
        // count as owning the port the path names.
        assert!(
            lock_if_current(early, &lock).expect("lock").is_none(),
            "a lock on an unlinked inode was taken as ownership — two owners"
        );
        drop(fresh);
        let _ = std::fs::remove_file(&lock);
    }
}
