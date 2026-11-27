//! Issue 2457 — a port that ACCEPTS is not a port YOUR daemon listens on.
//!
//! A fixture's readiness wait used to be "a TCP connect to the port succeeds".
//! That answers "does something listen there", which is not the question. The
//! port lease (`crate::port_lease`) makes a port exclusive among OUR fixtures
//! only: "it does not stop an unrelated program from taking the port". On a
//! host shared with other tenants that happens. Measured on newslab-118,
//! 2026-11-15, with another tenant's ROS 2 / CARLA processes on the host:
//!
//! ```text
//! zenohd: accepting on tcp/127.0.0.1:34831 0 ms after spawn
//! nros: zenoh open: no session with tcp/127.0.0.1:34831 in one attempt (...)
//! ```
//!
//! A real `rmw_zenohd` takes 15-20 ms from spawn to its first accept (measured
//! 6 times on the same host). 0 ms is a listener that was there BEFORE the
//! router: the router could not bind, and the session opened onto a stranger.
//!
//! So a readiness wait asks who holds the listening socket: the LISTEN rows of
//! `/proc/net/tcp{,6}` give the socket's inode, and the daemon's process tree
//! holds it as an `fd -> socket:[inode]` link. `Holder::Other` is a port taken
//! by another program; `Holder::Unknown` (no `/proc`, or no listener at all)
//! leaves the caller's old answer standing.

use std::path::Path;

/// Who holds the listening socket on a port.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Holder {
    /// `pid` or one of its descendants.
    Ours,
    /// A listener exists, and nothing in `pid`'s tree holds it.
    Other,
    /// No listener, or this host cannot say (no `/proc/net/tcp`).
    Unknown,
}

/// The inodes of every LISTEN socket on `port`, any address, IPv4 and IPv6.
/// `None` when neither table can be read.
pub fn listener_inodes(port: u16) -> Option<Vec<u64>> {
    let mut found = None::<Vec<u64>>;
    for table in ["/proc/net/tcp", "/proc/net/tcp6"] {
        let Ok(text) = std::fs::read_to_string(table) else {
            continue;
        };
        let list = found.get_or_insert_with(Vec::new);
        list.extend(parse_listeners(&text, port));
    }
    found
}

/// The LISTEN rows of one `/proc/net/tcp`-format table for `port`.
fn parse_listeners(text: &str, port: u16) -> Vec<u64> {
    let mut out = Vec::new();
    for line in text.lines().skip(1) {
        let f: Vec<&str> = line.split_whitespace().collect();
        if f.len() < 10 || f[3] != "0A" {
            continue;
        }
        let Some((_, p)) = f[1].rsplit_once(':') else {
            continue;
        };
        if u16::from_str_radix(p, 16).ok() != Some(port) {
            continue;
        }
        if let Ok(inode) = f[9].parse::<u64>() {
            if inode != 0 {
                out.push(inode);
            }
        }
    }
    out
}

/// `pid` and every live descendant, by parent pid.
fn tree(pid: u32) -> Vec<u32> {
    // treewalk-exempt: a walk of /proc's process table by parent pid, one
    // level of directory entries, never a source or build tree.
    let mut kids: std::collections::HashMap<u32, Vec<u32>> = Default::default();
    if let Ok(dir) = std::fs::read_dir("/proc") {
        for e in dir.flatten() {
            let Some(p) = e.file_name().to_str().and_then(|s| s.parse::<u32>().ok()) else {
                continue;
            };
            let Ok(stat) = std::fs::read_to_string(e.path().join("stat")) else {
                continue;
            };
            let Some(rest) = stat.rfind(')').map(|i| &stat[i + 2..]) else {
                continue;
            };
            if let Some(ppid) = rest.split(' ').nth(1).and_then(|s| s.parse::<u32>().ok()) {
                kids.entry(ppid).or_default().push(p);
            }
        }
    }
    let (mut out, mut todo) = (Vec::new(), vec![pid]);
    while let Some(p) = todo.pop() {
        out.push(p);
        if let Some(k) = kids.get(&p) {
            todo.extend(k);
        }
    }
    out
}

fn holds_socket(pid: u32, inodes: &[u64]) -> bool {
    let Ok(fds) = std::fs::read_dir(Path::new("/proc").join(pid.to_string()).join("fd")) else {
        return false;
    };
    fds.flatten().any(|fd| {
        std::fs::read_link(fd.path())
            .ok()
            .and_then(|l| l.to_str().map(str::to_owned))
            .and_then(|l| {
                l.strip_prefix("socket:[")?
                    .strip_suffix(']')?
                    .parse::<u64>()
                    .ok()
            })
            .is_some_and(|i| inodes.contains(&i))
    })
}

/// Who holds the listener on `port`, as seen from the daemon `pid`.
pub fn holder(pid: u32, port: u16) -> Holder {
    let Some(inodes) = listener_inodes(port) else {
        return Holder::Unknown;
    };
    if inodes.is_empty() {
        return Holder::Unknown;
    }
    if tree(pid).into_iter().any(|p| holds_socket(p, &inodes)) {
        Holder::Ours
    } else {
        Holder::Other
    }
}

/// Issue 2949 -- what `holder(pid, port)` saw, for a failure message: every
/// member of `pid`'s tree with its `comm` and `exe`, and each fd that links to
/// one of the port's LISTEN inodes. The negative control's one recorded red
/// read `Ours` from a `sleep` with no children, which the code cannot explain,
/// so the next red must carry the evidence rather than a bare `left: Ours`.
#[cfg(test)]
fn describe_holder(pid: u32, port: u16) -> String {
    let inodes = listener_inodes(port).unwrap_or_default();
    let proc_dir = |p: u32| Path::new("/proc").join(p.to_string());
    let mut out = format!("port {port} LISTEN inodes {inodes:?}; tree of {pid}:");
    for p in tree(pid) {
        let comm = std::fs::read_to_string(proc_dir(p).join("comm"))
            .map(|c| c.trim().to_string())
            .unwrap_or_else(|e| format!("<comm: {e}>"));
        let exe = std::fs::read_link(proc_dir(p).join("exe"))
            .map(|e| e.display().to_string())
            .unwrap_or_else(|e| format!("<exe: {e}>"));
        let ppid = std::fs::read_to_string(proc_dir(p).join("stat"))
            .ok()
            .and_then(|st| {
                let rest = &st[st.rfind(')')? + 2..];
                rest.split(' ').nth(1).map(str::to_string)
            })
            .unwrap_or_else(|| "?".into());
        let mut held = Vec::new();
        if let Ok(fds) = std::fs::read_dir(proc_dir(p).join("fd")) {
            for fd in fds.flatten() {
                if let Ok(link) = std::fs::read_link(fd.path()) {
                    let link = link.display().to_string();
                    let hit = link
                        .strip_prefix("socket:[")
                        .and_then(|l| l.strip_suffix(']'))
                        .and_then(|i| i.parse::<u64>().ok())
                        .is_some_and(|i| inodes.contains(&i));
                    if hit {
                        held.push(format!("fd {} -> {link}", fd.file_name().to_string_lossy()));
                    }
                }
            }
        }
        out.push_str(&format!(
            "\n  pid {p} (ppid {ppid}) comm {comm:?} exe {exe}: holds {held:?}"
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_only_listen_rows_on_the_port() {
        let t = "  sl  local_address rem_address   st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode\n\
                 0: 0100007F:880F 00000000:0000 0A 00000000:00000000 00:00000000 00000000  1000        0 111 1\n\
                 1: 0100007F:880F 0100007F:9C40 01 00000000:00000000 00:00000000 00000000  1000        0 222 1\n\
                 2: 00000000:1F90 00000000:0000 0A 00000000:00000000 00:00000000 00000000  1000        0 333 1\n";
        assert_eq!(parse_listeners(t, 0x880F), vec![111]);
        assert_eq!(parse_listeners(t, 0x1F90), vec![333]);
        assert!(parse_listeners(t, 1).is_empty());
    }

    #[test]
    fn a_listener_this_process_holds_is_ours_and_another_pid_does_not_hold_it() {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = l.local_addr().unwrap().port();
        if listener_inodes(port).is_none() {
            return; // no /proc/net/tcp on this host: `Unknown` is the answer
        }
        assert_eq!(holder(std::process::id(), port), Holder::Ours);
        // The diagnostic below must name a real holder when there is one, or a
        // red would print nothing useful (issue 2949).
        let own = describe_holder(std::process::id(), port);
        assert!(
            own.contains("-> socket:["),
            "the holder's fd is named: {own}"
        );
        // Negative control: the same listener, seen from a process tree that
        // does not hold it (a child that inherited no socket), is a stranger's.
        let mut child = std::process::Command::new("sleep")
            .arg("5")
            .spawn()
            .unwrap();
        let seen = holder(child.id(), port);
        // Issue 2949: read what `holder` saw before the child is reaped, so a
        // red names the process and fd that made it `Ours`.
        let evidence = (seen != Holder::Other).then(|| describe_holder(child.id(), port));
        let _ = child.kill();
        let _ = child.wait();
        assert_eq!(
            seen,
            Holder::Other,
            "a child that inherited no socket read as holding it: {}",
            evidence.unwrap_or_default()
        );
        drop(l);
        assert_eq!(holder(std::process::id(), port), Holder::Unknown);
    }
}
