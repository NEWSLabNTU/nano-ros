//! One GNU make jobserver for a whole `nros build` (RFC-0117 D4).
//!
//! `--parallel-workers N` runs up to N packages at once, and each package's
//! own tool — cargo, `cmake --build` over make or ninja, west over cmake and
//! ninja — is parallel on its own. Left alone that is N tools at N jobs each,
//! N² processes. So `nros build` owns ONE token pool of N and hands it to
//! every child: a package starts only with a token in hand, and every tool
//! below it draws its extra jobs from the same pool. Total jobs stay at N
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
//! form is chosen unless a CMake package would be driven by a make older than
//! 4.4, which it would kill; then the pipe form is used and the ninja gap is
//! said out loud. Ninja also IGNORES the pool when given an explicit `-j`,
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
        static SEQ: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let seq = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("nros-jobserver-{}-{seq}", std::process::id()));
        std::fs::DirBuilder::new().mode(0o700).create(&dir)?;
        let fifo = dir.join("fifo");
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

/// Which form to create, given what drives CMake packages: the FIFO unless
/// the `CMAKE_GENERATOR` in use is make's and that make is older than 4.4.
/// `make` is `None` when no make runs.
#[must_use]
pub fn choose_style(cmake_generator: Option<&str>, make: Option<(u32, u32)>) -> Style {
    let makefiles = cmake_generator.is_none_or(|g| g.contains("Makefiles"));
    match make {
        Some(v) if makefiles && v < MAKE_FIFO => Style::Pipe,
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
    fn the_pipe_form_is_chosen_only_for_an_old_make_that_will_run() {
        assert_eq!(choose_style(None, Some((4, 3))), Style::Pipe);
        assert_eq!(
            choose_style(Some("Unix Makefiles"), Some((4, 3))),
            Style::Pipe
        );
        assert_eq!(choose_style(Some("Ninja"), Some((4, 3))), Style::Fifo);
        assert_eq!(choose_style(None, Some((4, 4))), Style::Fifo);
        assert_eq!(choose_style(None, None), Style::Fifo);
    }
}
