//! What a runner starts dies with the runner (phase-544 W3, issue 2307).
//!
//! `nros run` starts its runner with `PR_SET_PDEATHSIG`, so a verb killed with
//! SIGKILL, which no handler sees, still takes the runner down. The image is
//! the RUNNER's child, though, and nothing tied it to anyone: a SIGKILL to the
//! verb left the native image or the QEMU guest running, reparented to init.
//! `nros-extension` cannot set the flag, because it forbids `unsafe`; this
//! crate is the one place a runner's spawn may use it. Every first-party
//! runner that starts something goes through one of its two calls:
//!
//! * [`die_with_runner`]: one hop. The child gets `PR_SET_PDEATHSIG(SIGKILL)`,
//!   set between fork and exec. For an image the runner starts directly and
//!   that starts nothing of its own (a host image, QEMU).
//! * [`Chain`]: any depth. For a TOOL that starts the image further down
//!   (`west build -t run` is west, then ninja, then QEMU or `zephyr.exe`; a
//!   `prefix` such as `gdb --args`; a user's command template). PDEATHSIG
//!   reaches one hop only, so the tool and everything under it join a process
//!   group led by a small guard (`/bin/sh`) that holds the read end of a pipe
//!   whose write end only the runner holds. When the runner goes, by exit or
//!   by any signal, SIGKILL included, the kernel closes that end, the guard
//!   reads end of file and stops its group: SIGTERM, one second, SIGKILL.
//!   The guard needs nothing from Linux, so it is also what covers a runner
//!   on another Unix, where PDEATHSIG does not exist.
//!
//! # PDEATHSIG and threads
//!
//! The "parent" PDEATHSIG watches is the THREAD that forked, not the process:
//! the signal fires when that thread exits, even while the runner lives on.
//! So [`die_with_runner`] is only correct when the spawn happens on a thread
//! that lives as long as the runner. Every first-party runner spawns from its
//! main thread (the threads it starts afterwards only copy console bytes). A
//! runner that spawns from a worker thread must keep that thread alive until
//! the child is reaped, or use [`Chain`], whose guard keys on the pipe and not
//! on a thread.
//!
//! # The race at fork
//!
//! If the runner dies after `fork` but before the child's `prctl`, the child
//! is already reparented and no signal will ever come. The hook therefore
//! reads `getppid()` after `prctl` and refuses to exec when the parent is no
//! longer the runner.
//!
//! Off Linux, [`die_with_runner`] is a no-op (no other supported host has
//! PDEATHSIG), and [`Chain`] still works on any Unix. Off Unix both are
//! no-ops.

#![warn(missing_docs)]

use std::process::Command;

/// Tie `cmd`'s child to this runner: on Linux it gets SIGKILL when the runner
/// (strictly, the spawning thread; see the crate docs) dies. A child spawned
/// after the runner has already gone does not exec at all.
///
/// One hop only: whatever the child starts is not covered. Use [`Chain`] for a
/// tool that starts the image itself.
pub fn die_with_runner(cmd: &mut Command) -> &mut Command {
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::process::CommandExt;
        // Read before fork: in the child, this is the pid getppid() must
        // still answer after prctl.
        let runner = std::process::id() as libc::pid_t;
        // SAFETY: the hook runs between fork and exec, and calls only prctl
        // and getppid, both async-signal-safe; it touches no memory the
        // parent's other threads could hold a lock on.
        unsafe {
            cmd.pre_exec(move || {
                if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL) != 0 {
                    return Err(std::io::Error::last_os_error());
                }
                if libc::getppid() != runner {
                    // The runner died between fork and prctl: the signal it
                    // would have sent is already gone.
                    return Err(std::io::Error::from_raw_os_error(libc::ESRCH));
                }
                Ok(())
            });
        }
    }
    cmd
}

/// The guard's script: ignore the signals a teardown sends, wait for the
/// runner, then stop the group. A line on stdin is the runner finishing
/// normally (the tool has exited, so only leftovers remain: SIGKILL them);
/// end of file is the runner gone without a word.
#[cfg(unix)]
const GUARD: &str = "trap '' INT TERM HUP
if read -r _; then kill -KILL 0; fi
kill -TERM 0
sleep 1
kill -KILL 0
";

/// A process group that dies when the runner does, however deep the processes
/// in it are (see the crate docs). Arm one, [`join`](Chain::join) the tool's
/// command to it, run the tool, and drop the chain once the tool has been
/// waited for: the drop stops anything the tool left behind.
pub struct Chain {
    #[cfg(unix)]
    guard: Option<Guard>,
}

#[cfg(unix)]
struct Guard {
    child: std::process::Child,
    tie: Option<std::process::ChildStdin>,
}

impl Chain {
    /// Start the guard. Off Unix, a chain that does nothing.
    ///
    /// # Errors
    ///
    /// `/bin/sh` could not be started.
    pub fn arm() -> std::io::Result<Self> {
        #[cfg(unix)]
        {
            use std::{os::unix::process::CommandExt, process::Stdio};
            let mut child = Command::new("/bin/sh")
                .args(["-c", GUARD])
                // Its own group, which it leads, so `kill 0` names exactly
                // the chain, and the group id cannot be reused while the
                // guard lives.
                .process_group(0)
                // The pipe's write end is close-on-exec in the runner, so
                // nothing the chain starts holds it open.
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()?;
            let tie = child.stdin.take();
            Ok(Self {
                guard: Some(Guard { child, tie }),
            })
        }
        #[cfg(not(unix))]
        Ok(Self {})
    }

    /// Put `cmd`'s child in the chain's group, and tie that first hop with
    /// [`die_with_runner`] too, so on Linux it goes at once rather than when
    /// the guard gets to it.
    pub fn join<'c>(&self, cmd: &'c mut Command) -> &'c mut Command {
        #[cfg(unix)]
        if let Some(pgid) = self.pgid() {
            use std::os::unix::process::CommandExt;
            cmd.process_group(pgid as i32);
        }
        die_with_runner(cmd)
    }

    /// The chain's process group (the guard's pid), where there is one.
    #[must_use]
    pub fn pgid(&self) -> Option<u32> {
        #[cfg(unix)]
        return self.guard.as_ref().map(|g| g.child.id());
        #[cfg(not(unix))]
        None
    }
}

/// Run `cmd` as a [`Chain`] through `exec` (`runner::exec` or
/// `runner::exec_tool`), and stop whatever it left behind once it has
/// exited. A guard that cannot start is the runner failing.
pub fn exec_chain(
    cmd: &mut Command,
    exec: impl FnOnce(&mut Command) -> nros_extension::exit::RunnerExit,
) -> nros_extension::exit::RunnerExit {
    match Chain::arm() {
        Ok(chain) => exec(chain.join(cmd)),
        Err(e) => nros_extension::runner::fail(format!("starting the process guard: {e}")),
    }
}

impl Drop for Chain {
    fn drop(&mut self) {
        #[cfg(unix)]
        if let Some(mut g) = self.guard.take() {
            use std::io::Write;
            if let Some(mut tie) = g.tie.take() {
                let _ = tie.write_all(b"done\n");
            }
            let _ = g.child.wait();
        }
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    fn alive(pid: u32) -> bool {
        // A zombie has exited; only its entry is left for a parent to reap.
        std::fs::read_to_string(format!("/proc/{pid}/stat"))
            .ok()
            .and_then(|s| s.rsplit_once(") ").map(|(_, rest)| !rest.starts_with('Z')))
            .unwrap_or(false)
    }

    fn gone_within(pid: u32, within: Duration) -> bool {
        let deadline = Instant::now() + within;
        while Instant::now() < deadline {
            if !alive(pid) {
                return true;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        !alive(pid)
    }

    #[test]
    fn a_dropped_chain_stops_what_its_tool_left_behind() {
        let chain = Chain::arm().unwrap();
        let pgid = chain.pgid().unwrap();
        // The tool exits at once and leaves a grandchild running, as ninja
        // would leave QEMU if it were killed.
        let out = chain
            .join(Command::new("/bin/sh").args(["-c", "sleep 30 >/dev/null 2>&1 & echo $!"]))
            .output()
            .unwrap();
        let orphan: u32 = String::from_utf8_lossy(&out.stdout).trim().parse().unwrap();
        assert!(alive(orphan));
        let stat = std::fs::read_to_string(format!("/proc/{orphan}/stat")).unwrap();
        let fields: Vec<&str> = stat.rsplit_once(") ").unwrap().1.split(' ').collect();
        assert_eq!(
            fields[2],
            pgid.to_string(),
            "the grandchild is in the chain"
        );
        drop(chain);
        assert!(gone_within(orphan, Duration::from_secs(5)));
    }

    #[test]
    fn the_guard_stops_the_chain_when_its_runner_is_killed() {
        // This test process plays the runner. A runner killed with SIGKILL
        // does exactly one thing the guard can see: its end of the pipe
        // closes. Closing it here, without the "done" line, is that.
        let mut chain = Chain::arm().unwrap();
        let mut tool = chain
            .join(&mut Command::new("/bin/sh"))
            .args(["-c", "sleep 30 >/dev/null 2>&1 & echo $!; wait"])
            .stdout(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        let mut line = String::new();
        std::io::BufRead::read_line(
            &mut std::io::BufReader::new(tool.stdout.take().unwrap()),
            &mut line,
        )
        .unwrap();
        let grandchild: u32 = line.trim().parse().unwrap();
        // The runner dying without a word: its end of the pipe closes.
        let g = chain.guard.as_mut().unwrap();
        drop(g.tie.take());
        assert!(gone_within(grandchild, Duration::from_secs(5)));
        let _ = tool.wait();
    }

    /// Set in the copy of this test binary that plays the runner.
    const ROLE: &str = "NROS_RUNNER_LIFETIME_TEST_RUNNER";

    #[test]
    fn a_tied_child_dies_with_its_runner_on_sigkill() {
        if std::env::var_os(ROLE).is_some() {
            // The runner: start a tied image, name it, and wait forever.
            let mut child = die_with_runner(&mut Command::new("sleep"))
                .arg("30")
                .spawn()
                .unwrap();
            println!("image {}", child.id());
            // Killed long before this returns; the wait is never reached.
            let _ = child.wait();
            return;
        }
        let mut runner = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "tests::a_tied_child_dies_with_its_runner_on_sigkill",
                "--nocapture",
                "--test-threads=1",
            ])
            .env(ROLE, "1")
            .stdout(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        let reader = std::io::BufReader::new(runner.stdout.take().unwrap());
        let image: u32 = std::io::BufRead::lines(reader)
            .map_while(Result::ok)
            // libtest may print `test <name> ... ` on the same line first.
            .find_map(|l| {
                l.split_once("image ")
                    .map(|(_, p)| p.trim().parse().unwrap())
            })
            .expect("the runner named its image");
        assert!(alive(image));
        runner.kill().unwrap(); // SIGKILL: no handler, no cleanup
        let _ = runner.wait();
        let gone = gone_within(image, Duration::from_secs(5));
        if !gone {
            // Never leave the survivor behind a red test.
            let _ = Command::new("kill")
                .args(["-KILL", &image.to_string()])
                .status();
        }
        assert!(gone, "image {image} survived");
    }
}
