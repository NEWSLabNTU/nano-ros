//! One GNU make jobserver for a whole `nros build` (RFC-0117 D4).
//!
//! `nros build -j N` runs independent packages side by side, and each
//! package's own tool — cargo, `cmake --build` over make or ninja, west over
//! cmake and ninja — is parallel on its own. Left alone that is N tools at N
//! jobs each, N² processes. So `nros build` owns ONE token pool of N and hands
//! it to every child: a package starts only with a token in hand, and every
//! tool below it draws its extra jobs from the same pool. Total jobs stay at N
//! however the work is shaped. This is the same mechanism the repository's
//! `just` fan-outs use (`scripts/build/jobserver-pool.sh`, make 4.4's FIFO
//! jobserver); `nros build` simply plays the part make plays there.
//!
//! ## Which form
//!
//! GNU make defines two ways to name a pool in `MAKEFLAGS`:
//! `--jobserver-auth=fifo:PATH` (make 4.4) and `--jobserver-auth=R,W`, two
//! inherited pipe descriptors (make 4.3 and older). They do not overlap:
//!
//! | client | `fifo:PATH` | `R,W` |
//! | --- | --- | --- |
//! | cargo, rustc | joins | joins |
//! | make 4.4 | joins | joins |
//! | make 4.3 | **fatal** (`invalid --jobserver-auth string`) | joins |
//! | ninja 1.13 | joins | ignored: runs at its own default width |
//! | ninja < 1.13 | ignored | ignored |
//!
//! (Measured on the pinned ninja 1.13.2 and on make 4.3 / 4.4.1.) So the FIFO
//! form is chosen unless the `make` a child would run is older than 4.4, which
//! it would kill; then the pipe form is used and the ninja gap is said out
//! loud. That is asked of the make on PATH whatever drives the package, never
//! of the CMake generator: a Ninja build runs make too (Zephyr's native_sim
//! links `zephyr.exe` through the native simulator's Makefile, an
//! a CMake ExternalProject may use make, a cargo build script may), and inheriting
//! a FIFO pool killed exactly that link under west (issue 2219). Ninja also IGNORES the pool when given an explicit `-j`,
//! which `cmake --build` passes when `CMAKE_BUILD_PARALLEL_LEVEL` is set, so
//! that variable is removed from every child that gets the pool.
//!
//! ## Joining
//!
//! When `nros build` itself runs under a jobserver (`MAKEFLAGS` carries
//! `--jobserver-auth`, e.g. a `make -jN` recipe), it JOINS that pool rather
//! than creating a second one, and its children inherit the outer `MAKEFLAGS`
//! unchanged. A pipe-form pool whose descriptors were not passed down (the
//! parent rule lacked make's `+`) cannot be joined; `nros build` then makes
//! its own, exactly as make itself falls back.

use std::{
    ffi::OsString,
    fs::File,
    io::{self, Read, Write},
    path::PathBuf,
};

/// What a child process needs set (`Some`) or removed (`None`) to share the
/// pool. Applied by [`super::handoff::Handoff`] after its own environment.
pub type ChildEnv = Vec<(OsString, Option<OsString>)>;

/// The variable `cmake --build` turns into an explicit `-j`, which makes both
/// make and ninja stop reading the pool.
pub const CMAKE_PARALLEL_LEVEL: &str = "CMAKE_BUILD_PARALLEL_LEVEL";

/// How the pool is named to children.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Style {
    /// `--jobserver-auth=fifo:PATH`: make 4.4, ninja ≥ 1.13, cargo.
    Fifo,
    /// `--jobserver-auth=R,W`: every make, cargo; never ninja.
    Pipe,
}

#[derive(Debug)]
enum Auth {
    /// A pool this process created; removed on drop.
    OwnedFifo {
        dir: PathBuf,
        fifo: PathBuf,
    },
    OwnedPipe {
        read: i32,
        write: i32,
    },
    /// A pool from `MAKEFLAGS`; the children inherit it as-is.
    Joined,
}

/// A GNU make jobserver this process either owns or has joined.
#[derive(Debug)]
pub struct Jobserver {
    read: File,
    write: File,
    auth: Auth,
    /// The pool's size, counting this process's own implicit token. `None`
    /// when joined: only the outer make knows.
    tokens: Option<usize>,
    /// How the pool is named, which decides whether ninja can read it.
    style: Style,
}

/// One token drawn from the pool, returned when dropped.
#[derive(Debug)]
pub struct Token<'a> {
    pool: &'a Jobserver,
    byte: u8,
}

impl Drop for Token<'_> {
    fn drop(&mut self) {
        // A token that is not returned shrinks the pool for every other
        // client for the rest of the run, so retry an interrupted write.
        loop {
            match (&self.pool.write).write(&[self.byte]) {
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                _ => break,
            }
        }
    }
}

impl Jobserver {
    /// A new pool of `tokens` jobs. This process holds one of them implicitly,
    /// as make's top level does, so `tokens - 1` are put in the pool.
    pub fn create(tokens: usize, style: Style) -> io::Result<Self> {
        let tokens = tokens.max(1);
        let js = match style {
            Style::Fifo => Self::create_fifo(tokens)?,
            Style::Pipe => Self::create_pipe(tokens)?,
        };
        let fill = vec![b'+'; tokens - 1];
        (&js.write).write_all(&fill)?;
        Ok(js)
    }

    #[cfg(unix)]
    fn create_fifo(tokens: usize) -> io::Result<Self> {
        use std::os::unix::{ffi::OsStrExt, fs::DirBuilderExt};
        // A private directory, so the path is not guessable and nobody else's
        // make can drain the pool.
        //
        // The name records the owner's pid AND start time, so a later pool can
        // tell this one is orphaned once the process is gone, even after the
        // pid is reused (issue 2254). Without `/proc` only the pid is written,
        // and only the pid is checked.
        static SEQ: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let seq = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let root = std::env::temp_dir();
        retire_orphan_fifo_dirs(&root);
        let pid = std::process::id();
        let name = match process_start(pid) {
            Some(start) => format!("{FIFO_DIR_PREFIX}{pid}-{start}-{seq}"),
            None => format!("{FIFO_DIR_PREFIX}{pid}-{seq}"),
        };
        let dir = root.join(name);
        std::fs::DirBuilder::new().mode(0o700).create(&dir)?;
        let fifo = dir.join(FIFO_NAME);
        let c = std::ffi::CString::new(fifo.as_os_str().as_bytes())
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;
        // SAFETY: `c` is a valid NUL-terminated path.
        if unsafe { libc::mkfifo(c.as_ptr(), 0o600) } != 0 {
            let e = io::Error::last_os_error();
            let _ = std::fs::remove_dir(&dir);
            return Err(e);
        }
        // Read+write: opening a FIFO for one direction alone blocks until
        // someone opens the other, and holding both keeps the buffered tokens
        // alive between clients.
        let file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&fifo)?;
        Ok(Jobserver {
            write: file.try_clone()?,
            read: file,
            auth: Auth::OwnedFifo { dir, fifo },
            tokens: Some(tokens),
            style: Style::Fifo,
        })
    }

    #[cfg(unix)]
    fn create_pipe(tokens: usize) -> io::Result<Self> {
        use std::os::fd::FromRawFd;
        let mut fds = [0i32; 2];
        // SAFETY: `fds` has room for two descriptors. They are created
        // WITHOUT close-on-exec on purpose: a pipe-form pool reaches the
        // children only by inheritance.
        if unsafe { libc::pipe(fds.as_mut_ptr()) } != 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: both descriptors were just created and are owned here.
        let (read, write) = unsafe { (File::from_raw_fd(fds[0]), File::from_raw_fd(fds[1])) };
        Ok(Jobserver {
            read,
            write,
            auth: Auth::OwnedPipe {
                read: fds[0],
                write: fds[1],
            },
            tokens: Some(tokens),
            style: Style::Pipe,
        })
    }

    #[cfg(not(unix))]
    fn create_fifo(_: usize) -> io::Result<Self> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "a GNU make jobserver needs a Unix host",
        ))
    }

    #[cfg(not(unix))]
    fn create_pipe(_: usize) -> io::Result<Self> {
        Self::create_fifo(0)
    }

    /// Join the pool `makeflags` names, if any.
    ///
    /// `Ok(None)` when it names none. `Err` when it names one this process
    /// cannot reach (a pipe whose descriptors were not inherited, a FIFO that
    /// is gone), with the reason; the caller then makes its own pool.
    pub fn join(makeflags: &str) -> Result<Option<Self>, String> {
        // make reads the LAST `--jobserver-auth` when there are several.
        let Some(auth) = makeflags
            .split_whitespace()
            .filter_map(|w| w.strip_prefix("--jobserver-auth="))
            .next_back()
        else {
            return Ok(None);
        };
        if let Some(path) = auth.strip_prefix("fifo:") {
            let file = std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .open(path)
                .map_err(|e| format!("cannot open the jobserver FIFO {path}: {e}"))?;
            let write = file.try_clone().map_err(|e| e.to_string())?;
            return Ok(Some(Jobserver {
                read: file,
                write,
                auth: Auth::Joined,
                tokens: None,
                style: Style::Fifo,
            }));
        }
        Self::join_pipe(auth).map(Some)
    }

    #[cfg(unix)]
    fn join_pipe(auth: &str) -> Result<Self, String> {
        use std::os::fd::FromRawFd;
        let parse = |s: Option<&str>| s.and_then(|s| s.parse::<i32>().ok());
        let mut it = auth.splitn(2, ',');
        let (Some(r), Some(w)) = (parse(it.next()), parse(it.next())) else {
            return Err(format!("unrecognised --jobserver-auth `{auth}`"));
        };
        for fd in [r, w] {
            // SAFETY: F_GETFD only reads the descriptor's flags.
            if fd < 0 || unsafe { libc::fcntl(fd, libc::F_GETFD) } == -1 {
                return Err(format!(
                    "MAKEFLAGS names jobserver descriptors {auth}, but they were not \
                     passed to this process (the calling make rule needs a `+` prefix)"
                ));
            }
        }
        // SAFETY: both descriptors are open (checked above). Duplicated, so
        // dropping these files never closes the descriptors the children
        // inherit through MAKEFLAGS.
        let (read, write) = unsafe {
            let (dr, dw) = (libc::dup(r), libc::dup(w));
            if dr < 0 || dw < 0 {
                return Err(io::Error::last_os_error().to_string());
            }
            (File::from_raw_fd(dr), File::from_raw_fd(dw))
        };
        Ok(Jobserver {
            read,
            write,
            auth: Auth::Joined,
            tokens: None,
            style: Style::Pipe,
        })
    }

    #[cfg(not(unix))]
    fn join_pipe(auth: &str) -> Result<Self, String> {
        Err(format!("cannot join jobserver `{auth}` on this host"))
    }

    /// Whether the pool came from an outer make rather than this process.
    #[must_use]
    pub fn is_joined(&self) -> bool {
        matches!(self.auth, Auth::Joined)
    }

    /// How the pool is named to children.
    #[must_use]
    pub fn style(&self) -> Style {
        self.style
    }

    /// The pool's size, counting the implicit token; `None` when joined.
    #[must_use]
    pub fn tokens(&self) -> Option<usize> {
        self.tokens
    }

    /// The `MAKEFLAGS` value naming this pool, or `None` when joined (the
    /// children inherit the outer one).
    #[must_use]
    pub fn makeflags(&self) -> Option<String> {
        let n = self.tokens?;
        let auth = match &self.auth {
            Auth::OwnedFifo { fifo, .. } => format!("fifo:{}", fifo.display()),
            Auth::OwnedPipe { read, write } => format!("{read},{write}"),
            Auth::Joined => return None,
        };
        Some(format!("-j{n} --jobserver-auth={auth}"))
    }

    /// What every child of this build needs to share the pool.
    #[must_use]
    pub fn child_env(&self) -> ChildEnv {
        let mut env: ChildEnv = Vec::new();
        if let Some(flags) = self.makeflags() {
            // cargo reads CARGO_MAKEFLAGS first and MAKEFLAGS after; make and
            // ninja read MAKEFLAGS. Both are set, so an inherited stale value
            // in either cannot win.
            env.push(("MAKEFLAGS".into(), Some(flags.clone().into())));
            env.push(("CARGO_MAKEFLAGS".into(), Some(flags.into())));
            env.push(("MFLAGS".into(), None));
        }
        env.push((CMAKE_PARALLEL_LEVEL.into(), None));
        env
    }

    /// Block until a token is free, and take it.
    pub fn acquire(&self) -> io::Result<Token<'_>> {
        let mut b = [0u8; 1];
        loop {
            match (&self.read).read(&mut b) {
                Ok(1) => {
                    return Ok(Token {
                        pool: self,
                        byte: b[0],
                    });
                }
                Ok(_) => {
                    return Err(io::Error::new(
                        io::ErrorKind::UnexpectedEof,
                        "the jobserver closed",
                    ));
                }
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(e) => return Err(e),
            }
        }
    }

    /// Tokens currently in the pool (not counting any held). For tests and
    /// diagnostics.
    #[cfg(unix)]
    pub fn available(&self) -> io::Result<usize> {
        use std::os::fd::AsRawFd;
        let mut n: libc::c_int = 0;
        // SAFETY: FIONREAD writes one c_int.
        if unsafe { libc::ioctl(self.read.as_raw_fd(), libc::FIONREAD, &mut n) } != 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(usize::try_from(n).unwrap_or(0))
    }
}

impl Drop for Jobserver {
    fn drop(&mut self) {
        if let Auth::OwnedFifo { dir, fifo } = &self.auth {
            let _ = std::fs::remove_file(fifo);
            let _ = std::fs::remove_dir(dir);
        }
    }
}

/// The prefix of every pool directory `create_fifo` makes in the temp dir.
const FIFO_DIR_PREFIX: &str = "nros-jobserver-";

/// The FIFO's name inside its pool directory.
const FIFO_NAME: &str = "fifo";

/// A process's start time, so a recycled pid is not mistaken for the owner:
/// field 22 of `/proc/<pid>/stat`, read after the `)` that ends the command
/// name (which may itself hold spaces or parentheses). `None` where `/proc`
/// is absent. The same rule as `scripts/build/gate-scratch.sh`.
#[cfg(unix)]
fn process_start(pid: u32) -> Option<u64> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let rest = &stat[stat.rfind(')')? + 1..];
    // After the `)`: field 3 (state) is the first word, so 22 is the 20th.
    rest.split_whitespace().nth(19)?.parse().ok()
}

/// Whether the process that made a pool may still be running. `start` is
/// the start time the directory recorded (`None` for a directory from before
/// that was recorded). Answers `true` whenever death cannot be PROVED.
#[cfg(unix)]
fn owner_alive(pid: u32, start: Option<u64>) -> bool {
    let Ok(raw) = libc::pid_t::try_from(pid) else {
        return true;
    };
    if raw <= 0 {
        return true;
    }
    // SAFETY: signal 0 performs only the existence and permission check.
    if unsafe { libc::kill(raw, 0) } != 0 {
        // EPERM: the pid exists and belongs to someone else.
        return io::Error::last_os_error().raw_os_error() != Some(libc::ESRCH);
    }
    match (start, process_start(pid)) {
        (Some(want), Some(now)) => want == now,
        // No recorded start, or no `/proc`: the pid alone, which is alive.
        _ => true,
    }
}

/// `(pid, start)` from a pool directory's name: `nros-jobserver-<pid>-<start>-<seq>`,
/// or `nros-jobserver-<pid>-<seq>` as written before the start time was
/// recorded (issue 2254). `None` for any other name.
fn parse_fifo_dir(name: &str) -> Option<(u32, Option<u64>)> {
    let parts: Vec<&str> = name.strip_prefix(FIFO_DIR_PREFIX)?.split('-').collect();
    if parts
        .iter()
        .any(|p| p.is_empty() || !p.bytes().all(|b| b.is_ascii_digit()))
    {
        return None;
    }
    match parts.as_slice() {
        [pid, _seq] => Some((pid.parse().ok()?, None)),
        [pid, start, _seq] => Some((pid.parse().ok()?, Some(start.parse().ok()?))),
        _ => None,
    }
}

/// Retire the pool directories under `root` whose owner is gone (issue
/// 2254). `Drop` removes a pool, but a signal that ends the process skips
/// `Drop`, and the FIFO cannot be unlinked early because fifo-form clients
/// open it by path. So each new pool sweeps the ones nothing can use any
/// more: a directory whose name parses, whose owner is provably dead (pid
/// gone, or reused by a process with another start time), and which holds
/// nothing but the FIFO. It is removed with `remove_file` + `remove_dir`,
/// never recursively, so a wrong match cannot take a tree with it. Every
/// failure is ignored: a directory that cannot be retired is left as it was.
/// Returns how many were retired.
#[cfg(unix)]
fn retire_orphan_fifo_dirs(root: &std::path::Path) -> usize {
    use std::os::unix::fs::FileTypeExt;
    let Ok(entries) = std::fs::read_dir(root) else {
        return 0;
    };
    let mut retired = 0;
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some((pid, start)) = name.to_str().and_then(parse_fifo_dir) else {
            continue;
        };
        // A real directory, not a symlink to one.
        if !entry.file_type().is_ok_and(|t| t.is_dir()) || owner_alive(pid, start) {
            continue;
        }
        let dir = entry.path();
        let Ok(inner) = std::fs::read_dir(&dir) else {
            continue;
        };
        let inner: Vec<_> = inner.collect();
        let only_the_fifo = inner.iter().all(|e| {
            e.as_ref().is_ok_and(|e| {
                e.file_name() == FIFO_NAME && e.file_type().is_ok_and(|t| t.is_fifo())
            })
        });
        if !only_the_fifo {
            continue;
        }
        if !inner.is_empty() && std::fs::remove_file(dir.join(FIFO_NAME)).is_err() {
            continue;
        }
        if std::fs::remove_dir(&dir).is_ok() {
            retired += 1;
        }
    }
    retired
}

/// `(major, minor)` from the first `X.Y` in a `--version` line, e.g.
/// `GNU Make 4.3` or `1.13.2`.
#[must_use]
pub fn parse_version(text: &str) -> Option<(u32, u32)> {
    text.lines().next()?.split_whitespace().find_map(|w| {
        let mut it = w.split('.');
        let major = it.next()?.parse().ok()?;
        let minor = it.next()?.parse().ok()?;
        Some((major, minor))
    })
}

/// The first line of `<program> --version`, or `None` when it does not run.
#[must_use]
pub fn tool_version(program: &str) -> Option<(u32, u32)> {
    let out = std::process::Command::new(program)
        .arg("--version")
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    parse_version(&String::from_utf8_lossy(&out.stdout))
}

/// Make older than 4.4 cannot read a FIFO pool.
pub const MAKE_FIFO: (u32, u32) = (4, 4);
/// Ninja older than 1.13 reads no pool at all.
pub const NINJA_CLIENT: (u32, u32) = (1, 13);

/// Which form to create, given the version of the `make` on PATH (`None`
/// when there is none): the FIFO unless that make is older than 4.4.
///
/// Deliberately NOT a function of the CMake generator or of which driver
/// runs. Any child may reach make — a Ninja build's custom command (Zephyr
/// native_sim's runner), a CMake ExternalProject, a build script — and every one
/// of them inherits `MAKEFLAGS`, so the pool must be one the reachable make
/// can read (issue 2219).
#[must_use]
pub fn choose_style(make: Option<(u32, u32)>) -> Style {
    match make {
        Some(v) if v < MAKE_FIFO => Style::Pipe,
        _ => Style::Fifo,
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn a_created_fifo_pool_holds_one_token_fewer_than_its_size() {
        let js = Jobserver::create(3, Style::Fifo).expect("pool");
        // The implicit token is this process's; two are in the FIFO.
        assert_eq!(js.available().unwrap(), 2);
        let a = js.acquire().unwrap();
        let b = js.acquire().unwrap();
        assert_eq!(js.available().unwrap(), 0);
        drop(a);
        assert_eq!(js.available().unwrap(), 1);
        drop(b);
        assert_eq!(js.available().unwrap(), 2);
        let flags = js.makeflags().unwrap();
        assert!(flags.starts_with("-j3 --jobserver-auth=fifo:"), "{flags}");
    }

    #[test]
    fn an_owned_fifo_is_removed_with_the_pool() {
        let js = Jobserver::create(2, Style::Fifo).expect("pool");
        let Auth::OwnedFifo { dir, fifo } = &js.auth else {
            panic!("fifo style")
        };
        let (dir, fifo) = (dir.clone(), fifo.clone());
        assert!(fifo.exists());
        drop(js);
        assert!(!fifo.exists() && !dir.exists());
    }

    /// A pid that is certainly not running: a child that has been reaped.
    fn dead_pid() -> u32 {
        let mut child = std::process::Command::new("true")
            .spawn()
            .expect("spawn true");
        let pid = child.id();
        child.wait().expect("reap");
        pid
    }

    /// A pool directory as `create_fifo` leaves one, holding a real FIFO.
    fn plant(root: &std::path::Path, name: &str) -> PathBuf {
        let dir = root.join(name);
        std::fs::create_dir(&dir).unwrap();
        let made = std::process::Command::new("mkfifo")
            .arg(dir.join(FIFO_NAME))
            .status()
            .expect("run mkfifo");
        assert!(made.success(), "mkfifo failed");
        dir
    }

    /// Issue 2254: a new pool retires the directories a signal (or a failed
    /// build's `exit`) left behind, and nothing else.
    #[test]
    fn orphaned_fifo_dirs_are_retired_and_nothing_else_is() {
        let root = tempfile::tempdir().unwrap();
        let root = root.path();
        let me = std::process::id();
        let start = process_start(me).expect("/proc on the test host");
        let dead = dead_pid();

        let orphan = plant(root, &format!("{FIFO_DIR_PREFIX}{dead}-{start}-0"));
        let legacy_orphan = plant(root, &format!("{FIFO_DIR_PREFIX}{dead}-0"));
        let live = plant(root, &format!("{FIFO_DIR_PREFIX}{me}-{start}-0"));
        // A live pid that started at another time: the pid was reused, so the
        // pool's owner is gone.
        let recycled = plant(root, &format!("{FIFO_DIR_PREFIX}{me}-{}-0", start + 1));
        // A pre-2254 name names no start time, so a live pid keeps it.
        let legacy_live = plant(root, &format!("{FIFO_DIR_PREFIX}{me}-0"));
        // Not the pattern: never touched, owner or not.
        let other = plant(root, &format!("nros-jobserverx-{dead}-0"));
        let bad_tail = plant(root, &format!("{FIFO_DIR_PREFIX}{dead}-0-0-0"));
        // The pattern, a dead owner, but something besides the FIFO inside:
        // left whole, since it is not provably a pool.
        let crowded = plant(root, &format!("{FIFO_DIR_PREFIX}{dead}-{start}-1"));
        std::fs::write(crowded.join("keep"), b"x").unwrap();
        // The pattern, a dead owner, but `fifo` is a regular file.
        let not_fifo = root.join(format!("{FIFO_DIR_PREFIX}{dead}-{start}-2"));
        std::fs::create_dir(&not_fifo).unwrap();
        std::fs::write(not_fifo.join(FIFO_NAME), b"").unwrap();
        // A dead owner's directory with no FIFO yet (mkfifo failed): empty, so
        // retiring it is safe.
        let empty = root.join(format!("{FIFO_DIR_PREFIX}{dead}-{start}-3"));
        std::fs::create_dir(&empty).unwrap();

        assert_eq!(retire_orphan_fifo_dirs(root), 4);
        for gone in [&orphan, &legacy_orphan, &recycled, &empty] {
            assert!(!gone.exists(), "{} should be retired", gone.display());
        }
        for kept in [&live, &legacy_live, &other, &bad_tail, &crowded, &not_fifo] {
            assert!(kept.exists(), "{} should be kept", kept.display());
        }
        assert!(crowded.join("keep").exists());
    }

    #[test]
    fn pool_dir_names_parse_only_in_the_two_written_shapes() {
        assert_eq!(
            parse_fifo_dir("nros-jobserver-12-34-0"),
            Some((12, Some(34)))
        );
        assert_eq!(parse_fifo_dir("nros-jobserver-12-0"), Some((12, None)));
        for bad in [
            "nros-jobserver-12",
            "nros-jobserver-12-",
            "nros-jobserver--0",
            "nros-jobserver-12-x-0",
            "nros-jobserver-12-34-0-1",
            "nros-gate-12-0",
        ] {
            assert_eq!(parse_fifo_dir(bad), None, "{bad}");
        }
    }

    /// The start time is field 22 of `/proc/<pid>/stat`, and it is stable
    /// for a running process.
    #[test]
    fn a_process_start_time_is_read_and_stable() {
        let me = std::process::id();
        let a = process_start(me).expect("/proc on the test host");
        assert_eq!(process_start(me), Some(a));
        assert!(owner_alive(me, Some(a)));
        assert!(!owner_alive(me, Some(a + 1)));
        assert!(!owner_alive(dead_pid(), None));
    }

    /// A pool made by this process is named with its pid and start time, and
    /// a second pool does not retire the first while both are live.
    #[test]
    fn a_new_pool_keeps_a_live_one() {
        let a = Jobserver::create(2, Style::Fifo).expect("pool a");
        let b = Jobserver::create(2, Style::Fifo).expect("pool b");
        let Auth::OwnedFifo { dir, fifo } = &a.auth else {
            panic!("fifo style")
        };
        let name = dir.file_name().unwrap().to_str().unwrap();
        let (pid, start) = parse_fifo_dir(name).expect("parses");
        assert_eq!(pid, std::process::id());
        assert_eq!(start, process_start(pid));
        assert!(fifo.exists(), "a live pool survived the second create");
        drop((a, b));
    }

    #[test]
    fn a_pipe_pool_names_its_descriptors() {
        let js = Jobserver::create(4, Style::Pipe).expect("pool");
        assert_eq!(js.available().unwrap(), 3);
        let flags = js.makeflags().unwrap();
        assert!(
            flags.starts_with("-j4 --jobserver-auth=") && !flags.contains("fifo:"),
            "{flags}"
        );
    }

    /// An outer FIFO pool is joined, and its tokens are the ones drawn.
    #[test]
    fn makeflags_with_a_fifo_is_joined() {
        let outer = Jobserver::create(2, Style::Fifo).expect("outer");
        let flags = outer.makeflags().unwrap();
        let inner = Jobserver::join(&format!(" -j2 {flags}"))
            .expect("joins")
            .expect("names a pool");
        assert!(inner.is_joined());
        assert_eq!(inner.tokens(), None);
        // Joined: the children inherit the outer MAKEFLAGS untouched.
        assert!(inner.makeflags().is_none());
        let t = inner.acquire().unwrap();
        assert_eq!(outer.available().unwrap(), 0, "drew the outer pool's token");
        drop(t);
        assert_eq!(outer.available().unwrap(), 1);
    }

    #[test]
    fn makeflags_without_a_pool_joins_nothing() {
        assert!(Jobserver::join("").unwrap().is_none());
        assert!(Jobserver::join("-j8 -k").unwrap().is_none());
    }

    #[test]
    fn a_pipe_pool_whose_descriptors_were_not_passed_is_refused() {
        let e = Jobserver::join("-j4 --jobserver-auth=900,901").expect_err("not open here");
        assert!(e.contains("`+`"), "{e}");
    }

    /// Every child gets the pool and loses the variable that would make
    /// `cmake --build` pass an explicit `-j` (which ninja and make take over
    /// the pool).
    #[test]
    fn child_env_names_the_pool_and_drops_the_explicit_parallel_level() {
        let js = Jobserver::create(2, Style::Fifo).expect("pool");
        let env = js.child_env();
        let get = |k: &str| env.iter().find(|(n, _)| n == k).map(|(_, v)| v.clone());
        let flags = js.makeflags().map(OsString::from);
        assert_eq!(get("MAKEFLAGS"), Some(flags.clone()));
        assert_eq!(get("CARGO_MAKEFLAGS"), Some(flags));
        assert_eq!(get(CMAKE_PARALLEL_LEVEL), Some(None));
    }

    #[test]
    fn versions_parse_from_both_tools_banners() {
        assert_eq!(parse_version("GNU Make 4.3\nBuilt for x"), Some((4, 3)));
        assert_eq!(parse_version("1.13.2\n"), Some((1, 13)));
        assert_eq!(parse_version("nothing here"), None);
    }

    #[test]
    fn the_pipe_form_is_chosen_whenever_the_reachable_make_is_old() {
        // An old make on PATH gets the pipe form whatever drives the build:
        // under Ninja/west it is still reached (issue 2219).
        assert_eq!(choose_style(Some((4, 3))), Style::Pipe);
        assert_eq!(choose_style(Some((3, 81))), Style::Pipe);
        assert_eq!(choose_style(Some((4, 4))), Style::Fifo);
        assert_eq!(choose_style(Some((4, 5))), Style::Fifo);
        assert_eq!(choose_style(None), Style::Fifo);
    }
}
