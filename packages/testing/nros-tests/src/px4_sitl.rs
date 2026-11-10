//! A running PX4 SITL daemon (phase-521 W8 (e), RFC-0113 D9).
//!
//! PX4 is a Tier 1 framework, and its runtime cells are `nros-tests` consumers
//! like every other platform's. So the SITL harness lives here rather than in
//! the out-of-workspace `nros-px4-sitl-test` crate, which needs the `px4-rs`
//! submodule only for its own `Px4Sitl`. This one needs nothing but a built
//! SITL tree.
//!
//! The SITL build is the BUILD stage's (`just px4 build-sitl-example`); this
//! module only boots it. [`prebuilt_sitl_dir`] fails loudly when the tree is
//! missing or was built from a different external-modules root.

use std::{
    fs::{File, OpenOptions},
    io::{BufRead, BufReader, Read},
    os::unix::{io::AsRawFd, net::UnixStream},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{Arc, Condvar, Mutex},
    thread,
    time::{Duration, Instant},
};

use crate::{fixtures::require::RequireFixture, process::ManagedProcess};

/// How long `rcS` may take to print its success line.
const BOOT_TIMEOUT: Duration = Duration::from_secs(30);

/// The line `etc/init.d-posix/rcS` prints when it has started everything.
const BOOT_DONE: &str = "Startup script returned successfully";

/// The PX4 instance every [`Px4Sitl`] boots as. PX4's posix daemon keys its
/// server lock (`/tmp/px4_lock-<n>`) and its client socket (`/tmp/px4-sock-<n>`)
/// on the instance, under a hardcoded `/tmp`, so instance 0 is ONE resource per
/// HOST, not per build dir: a second server unlinks the first's socket, and a
/// `px4-<module>` client reaches whichever server bound it last (issue 2243).
const INSTANCE: u32 = 0;

/// How long [`Px4Sitl::boot_in`] waits for another holder of [`INSTANCE`].
/// Longer than any one PX4 test; a holder past it is wedged, not busy.
const INSTANCE_WAIT: Duration = Duration::from_secs(300);

/// The PX4-Autopilot tree: `PX4_AUTOPILOT_DIR`, else the checkout's
/// `third-party/px4/PX4-Autopilot` (`nros setup --source px4-autopilot`).
pub fn px4_autopilot_dir() -> PathBuf {
    std::env::var_os("PX4_AUTOPILOT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| crate::project_root().join("third-party/px4/PX4-Autopilot"))
}

/// The prebuilt `px4_sitl_default` tree, asserted to contain `module`.
///
/// PX4 accepts one `EXTERNAL_MODULES_LOCATION` per build and every build
/// writes the same `build/px4_sitl_default`, so whichever root was built last
/// wins. A module's directory and its `bin/px4-<name>` shim both survive a
/// rebuild from another root, so neither proves the module is linked (issue
/// 1046). The proof is the command name inside `bin/px4` itself.
pub fn prebuilt_sitl_dir(module: &str, build_recipe: &str) -> PathBuf {
    let build_dir = px4_autopilot_dir().join("build/px4_sitl_default");
    let bin = build_dir.join("bin/px4");
    crate::fixtures::require_prebuilt_artifact(&bin, build_recipe).require("PX4 SITL binary");
    let linked = std::fs::read(&bin)
        .map(|bytes| bytes.windows(module.len()).any(|w| w == module.as_bytes()))
        .unwrap_or(false);
    assert!(
        linked,
        "SITL tree at {} was built WITHOUT `{module}`: {} does not contain it.\n\
         PX4 takes one EXTERNAL_MODULES_LOCATION per build and they share this \
         build dir, so another root built last. Its module directory and \
         `bin/px4-{module}` shim survive that, so neither is evidence the module \
         is linked (issue 1046).\nRebuild with:\n    {build_recipe}",
        build_dir.display(),
        bin.display(),
    );
    build_dir
}

#[derive(Default)]
struct LogBuf {
    text: Mutex<String>,
    grew: Condvar,
}

/// Exclusive hold on PX4 instance [`INSTANCE`] for this host (issue 2243).
///
/// PX4's own lock cannot serve: `main.cpp` checks it (`F_GETLK`) and takes it
/// (`F_SETLK`) in two steps, so two daemons started together can both pass the
/// check, and the loser prints nothing a test can wait on. The harness takes
/// its own lock BEFORE spawning, held until the daemon is dead, so a second
/// [`Px4Sitl`] (another test, process or nextest slot) waits instead of racing.
/// A separate file: an `fcntl` lock on PX4's own file would read to PX4 as a
/// running server.
struct InstanceLock {
    _file: File,
}

impl InstanceLock {
    fn acquire() -> Self {
        let path = PathBuf::from(format!("/tmp/nros-px4-sitl-instance-{INSTANCE}.lock"));
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(&path)
            .unwrap_or_else(|e| panic!("open {}: {e}", path.display()));
        let deadline = Instant::now() + INSTANCE_WAIT;
        // `flock`, not `File::try_lock`: the crate's MSRV predates it. The
        // lock goes with the descriptor, so a test that dies releases it.
        loop {
            // SAFETY: `file` owns the descriptor for the whole call.
            if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } == 0 {
                break;
            }
            let err = std::io::Error::last_os_error();
            if err.kind() != std::io::ErrorKind::WouldBlock {
                panic!("lock {}: {err}", path.display());
            }
            assert!(
                Instant::now() < deadline,
                "PX4 instance {INSTANCE} still held via {} after {INSTANCE_WAIT:?}: \
                 another PX4 SITL test owns it (issue 2243)",
                path.display()
            );
            thread::sleep(Duration::from_millis(200));
        }
        // Holding our lock rules out every harness daemon, not one started by
        // hand. A server that ACCEPTS on the instance socket is live; booting
        // over it would unlink its socket and race it, so refuse by name.
        let sock = format!("/tmp/px4-sock-{INSTANCE}");
        if UnixStream::connect(&sock).is_ok() {
            panic!(
                "a PX4 daemon not started by this harness is serving instance \
                 {INSTANCE} ({sock} accepts). Stop it first: two servers on one \
                 instance race for that socket (issue 2243)"
            );
        }
        Self { _file: file }
    }
}

/// A live `px4 -d etc/init.d-posix/rcS` daemon. Dropping it kills the
/// process group ([`ManagedProcess`]), so no orphan keeps PX4's UDP ports.
///
/// At most one exists per host at a time ([`InstanceLock`], issue 2243).
pub struct Px4Sitl {
    // Field order is drop order: the daemon dies BEFORE the instance is
    // released, so the next holder never meets a dying server.
    _daemon: ManagedProcess,
    build_dir: PathBuf,
    log: Arc<LogBuf>,
    _instance: InstanceLock,
}

impl Px4Sitl {
    /// Boot the daemon in `build_dir` and wait for `rcS` to finish.
    ///
    /// Waits first for any other [`Px4Sitl`] on this host to drop; the boot
    /// timeout starts after that.
    pub fn boot_in(build_dir: &Path) -> Self {
        let instance = InstanceLock::acquire();
        let mut cmd = Command::new(build_dir.join("bin/px4"));
        cmd.arg("-d")
            .arg(Path::new("etc").join("init.d-posix").join("rcS"))
            .current_dir(build_dir);
        let mut daemon = ManagedProcess::spawn_command(cmd, "px4-sitl")
            .unwrap_or_else(|e| panic!("spawn PX4 SITL in {}: {e}", build_dir.display()));
        let log = Arc::new(LogBuf::default());
        let child = daemon.handle_mut();
        if let Some(out) = child.stdout.take() {
            drain(out, Arc::clone(&log));
        }
        if let Some(err) = child.stderr.take() {
            drain(err, Arc::clone(&log));
        }
        let sitl = Self {
            _daemon: daemon,
            build_dir: build_dir.to_path_buf(),
            log,
            _instance: instance,
        };
        if let Err(e) = sitl.wait_for_log(BOOT_DONE, BOOT_TIMEOUT) {
            panic!(
                "PX4 SITL did not boot: {e}\n=== SITL log ===\n{}",
                sitl.log_snapshot()
            );
        }
        sitl
    }

    /// Run `bin/px4-<first word> <rest>` against the daemon and return its
    /// stdout. A client's output does NOT pass through the daemon's log.
    pub fn shell(&self, cmd: &str) -> String {
        let mut parts = cmd.split_whitespace();
        let module = parts.next().expect("shell: empty command");
        let out = Command::new(self.build_dir.join("bin").join(format!("px4-{module}")))
            .args(parts)
            .current_dir(&self.build_dir)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .unwrap_or_else(|e| panic!("run px4-{module}: {e}"));
        assert!(
            out.status.success(),
            "`{cmd}` exited {:?}\nstdout:\n{}\nstderr:\n{}\n=== SITL log ===\n{}",
            out.status.code(),
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr),
            self.log_snapshot()
        );
        String::from_utf8_lossy(&out.stdout).into_owned()
    }

    /// Wait until a daemon log line contains `pattern`; returns that line.
    pub fn wait_for_log(&self, pattern: &str, timeout: Duration) -> Result<String, String> {
        self.wait_for_log_count(pattern, 1, timeout)
            .map(|mut lines| lines.pop().unwrap_or_default())
    }

    /// Wait until `count` daemon log lines contain `pattern`; returns them.
    pub fn wait_for_log_count(
        &self,
        pattern: &str,
        count: usize,
        timeout: Duration,
    ) -> Result<Vec<String>, String> {
        let deadline = Instant::now() + timeout;
        let mut text = self.log.text.lock().unwrap();
        loop {
            let hits: Vec<String> = text
                .lines()
                .filter(|l| l.contains(pattern))
                .map(str::to_string)
                .collect();
            if hits.len() >= count {
                return Ok(hits);
            }
            let now = Instant::now();
            if now >= deadline {
                return Err(format!(
                    "{} of {count} line(s) containing `{pattern}` within {timeout:?}",
                    hits.len()
                ));
            }
            text = self.log.grew.wait_timeout(text, deadline - now).unwrap().0;
        }
    }

    /// Everything the daemon has printed so far.
    pub fn log_snapshot(&self) -> String {
        self.log.text.lock().unwrap().clone()
    }
}

fn drain<R: Read + Send + 'static>(reader: R, log: Arc<LogBuf>) {
    thread::spawn(move || {
        for line in BufReader::new(reader).lines() {
            let Ok(line) = line else { break };
            let mut text = log.text.lock().unwrap();
            text.push_str(&line);
            text.push('\n');
            log.grew.notify_all();
        }
    });
}
