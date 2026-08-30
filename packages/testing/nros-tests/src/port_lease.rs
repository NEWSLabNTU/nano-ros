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
//! A lock file per port under the build root, created `O_EXCL` and holding the
//! owning pid. Held for the lifetime of the [`PortLease`], removed on drop. It
//! does not stop an unrelated program from taking the port — nothing can — but
//! every colliding party here is one of our own fixtures, and that is the
//! population it makes exclusive.
//!
//! A lease whose owner is gone is reclaimed: nextest SIGKILLs test processes, so
//! `Drop` is not guaranteed to run and a leaked file must not poison a port
//! forever. This is the same hazard `zenohd_router::kill_listeners_on_port`
//! handles for the router, one layer earlier.

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
}

impl PortLease {
    /// The reserved port.
    pub fn port(&self) -> u16 {
        self.port
    }
}

impl Drop for PortLease {
    fn drop(&mut self) {
        // Best-effort: a lease we fail to remove is reclaimed by the
        // liveness check in `claim`, so it costs a later retry, never a hang.
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
        if claim(&lock)? {
            return Ok(PortLease { port, lock });
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

/// Try to take `lock`, reclaiming it when its recorded owner is gone.
///
/// Returns whether the caller now owns it.
fn claim(lock: &Path) -> io::Result<bool> {
    use std::io::Write;
    match std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(lock)
    {
        Ok(mut f) => {
            // Content is diagnostic AND load-bearing: the pid is what lets a
            // later run tell a live lease from a leaked one.
            let _ = write!(f, "{}", std::process::id());
            Ok(true)
        }
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {
            if owner_is_gone(lock) {
                // Reclaim. A racing reclaimer may remove it first, and the
                // `create_new` below is what settles that — exactly one wins.
                let _ = std::fs::remove_file(lock);
                return match std::fs::OpenOptions::new()
                    .create_new(true)
                    .write(true)
                    .open(lock)
                {
                    Ok(mut f) => {
                        let _ = write!(f, "{}", std::process::id());
                        Ok(true)
                    }
                    Err(e) if e.kind() == io::ErrorKind::AlreadyExists => Ok(false),
                    Err(e) => Err(e),
                };
            }
            Ok(false)
        }
        Err(e) => Err(e),
    }
}

/// True when the pid recorded in `lock` is not a live process.
///
/// An unreadable or malformed lease counts as gone: it cannot name an owner, so
/// keeping it would reserve a port on behalf of nobody.
fn owner_is_gone(lock: &Path) -> bool {
    let Ok(raw) = std::fs::read_to_string(lock) else {
        return true;
    };
    let Ok(pid) = raw.trim().parse::<i32>() else {
        return true;
    };
    !pid_is_live(pid)
}

#[cfg(unix)]
fn pid_is_live(pid: i32) -> bool {
    // `kill(pid, 0)` rather than `/proc/<pid>`: procfs is a LINUX filesystem,
    // not a unix one — FreeBSD does not mount it by default and the guard above
    // is `cfg(unix)`, so a `/proc` probe answered "dead" for every LIVE owner on
    // a non-Linux unix and handed its port to a second fixture. That is exactly
    // the cross-talk this module exists to prevent, and the `#[cfg(not(unix))]`
    // arm below could not catch it because BSD *is* unix. `kill` with signal 0
    // performs the existence check only and is POSIX, so one arm covers the
    // whole family.
    //
    // `EPERM` counts as LIVE: the process exists, we merely may not signal it.
    // That is the case the previous comment worried about ("no dependency on the
    // signalling permissions between two test processes") — it is answered by
    // reading the errno rather than by avoiding the call.
    //
    // A non-positive pid is rejected up front, and not as defensive noise:
    // `kill(0, …)` addresses the CALLER's process group and `kill(-n, …)` a
    // whole group, so both would report "live" for a lease that names nobody.
    // The reclaim test writes exactly `0`.
    if pid <= 0 {
        return false;
    }
    // SAFETY: signal 0 delivers nothing — it runs the permission and existence
    // checks alone, so it cannot disturb the process it asks about.
    if unsafe { libc::kill(pid, 0) } == 0 {
        return true;
    }
    std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
}

#[cfg(not(unix))]
fn pid_is_live(_pid: i32) -> bool {
    // No cheap portable check; treat leases as live and let MAX_ATTEMPTS move
    // on to another port. Over-reserving costs a retry, under-reserving costs
    // the cross-talk this module exists to prevent.
    true
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

    /// Dropping a lease frees the port for the next caller, or a long sweep
    /// would exhaust the ephemeral range.
    #[test]
    fn dropping_a_lease_releases_it() {
        let (port, lock) = {
            let l = lease_port(Transport::Udp).expect("lease");
            (l.port(), l.lock.clone())
        };
        assert!(!lock.exists(), "lease file outlived its lease");
        // The same port is claimable again.
        let again = claim(&lock).expect("claim");
        assert!(again, "a released port could not be re-leased");
        let _ = std::fs::remove_file(&lock);
        let _ = port;
    }

    /// A lease whose owner died must not reserve the port forever — nextest
    /// SIGKILLs test processes, so leaked files are expected, not exceptional.
    #[test]
    fn a_dead_owners_lease_is_reclaimed() {
        let dir = lease_dir();
        std::fs::create_dir_all(&dir).expect("lease dir");
        let lock = dir.join("udp-reclaim-probe.lock");
        let _ = std::fs::remove_file(&lock);
        // pid 0 never names a process — see `pid_is_live`.
        std::fs::write(&lock, "0").expect("write stale lease");

        assert!(
            claim(&lock).expect("claim"),
            "stale lease was not reclaimed"
        );
        // ...and now that WE hold it, a second claim fails.
        assert!(
            !claim(&lock).expect("claim"),
            "a live lease was handed out twice"
        );
        let _ = std::fs::remove_file(&lock);
    }
}
