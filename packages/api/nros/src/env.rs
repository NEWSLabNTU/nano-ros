//! The hosted edge of configuration: the process environment.
//!
//! Every env var nano-ros honours is read HERE and
//! nowhere else. The core (`nros-node`) takes values — `ExecutorConfig::
//! resolve_with` accepts an [`EnvRung`](nros_node::EnvRung) of already-resolved fields — so it
//! needs no `std`, no `env` capability, and no stub on the five ports that have
//! no environment to read.
//!
//! That is the difference between this and the ABI ports W10 made for the
//! clock, sleep, tasks and the log sink: those name capabilities every RTOS
//! HAS. A process environment is not one. Modelling it as an ABI entry would
//! have put `return 0` in five ports and permanent surface in the header, for a
//! facility that exists on one platform family.
//!
//! **One reader, through the C library**. The
//! environment is read with `getenv`, not `std::env`. A process environment is
//! a property of the PROCESS, not of the Rust standard library: threadx-linux
//! and freertos-posix are `#![no_std]` images that are still Linux processes
//! with a C library, and a measurement showed what happens when they stop
//! honouring `$ROS_DOMAIN_ID` (parallel cells shared a domain and heard each
//! other). There used to be two readers — this one on `std::env` and
//! ~~`host_env`~~ on `getenv` — with the same variables and rules; they are one
//! now, and `nros` needs no `std` to be a hosted edge. The edge is "a C library
//! is linked", which on this tree is `target_os = "linux"`; a cross target has
//! no `getenv` and this module does not exist there.
//!
//! Strings are borrowed from the process environment, which outlives the boot,
//! so the resolved config's `&'static str` fields point straight into it. No
//! cache is needed (the old `std::env` reader kept a `OnceLock` only to turn an
//! owned `String` into a `&'static str`, and its staleness was a bug).
//! Nothing in nano-ros calls `setenv` after boot.
//!
//! What lives here:
//!
//! * [`ExecutorConfigEnvExt::from_env`] — the constructor that used to be an
//!   inherent method on `ExecutorConfig`. Bring the trait into scope (the
//!   prelude has it) and the call site spelling is unchanged. `env` feature.
//! * [`resolve_hosted`](crate::env::resolve_hosted) / [`try_resolve_hosted`](crate::env::try_resolve_hosted) — the boot-config precedence ladder
//!   with the environment rung on top. The C / C++ entries call these on every
//!   Linux build, `env` or not.
//! * [`rmw_selector`] — the one `$NROS_RMW` reader.
//! * [`var`](crate::env::var) / [`read_file`](crate::env::read_file) — the two C-library reads the hosted `init*`
//!   entries need (`$RMW_IMPLEMENTATION`, `--params-file`).
//! * [`write_file`](crate::env::write_file), [`rename`](crate::env::rename), [`set_var`](crate::env::set_var), [`exe_name`](crate::env::exe_name), [`thread_key`](crate::env::thread_key),
//!   [`exit`](crate::env::exit) — the rest of the hosted PROCESS edge, for the C / C++ surfaces.
//!   `nros-c` and `nros-cpp` write their census,
//!   metadata and profile files through these instead of `std::fs` /
//!   `std::process`, so the image root never links libstd: a census is a
//!   property of a process with a C library, not of the Rust standard library.
//!   One spelling of each C-library call, here, rather than one per crate.

use core::ffi::{CStr, c_char};

use crate::retired_env::RETIRED_ENV_VARS;
use nros_node::{BootConfig, BootConfigError, EnvRung, ExecutorConfig, RMW_SELECTOR_CAP};
use nros_rmw::SessionMode;

unsafe extern "C" {
    fn getenv(name: *const c_char) -> *const c_char;
}

/// `$NAME` as a C string, or `None` when unset — the one `getenv` call.
fn raw(name: &CStr) -> Option<&'static CStr> {
    // SAFETY: `name` is NUL-terminated; `getenv` returns NULL or a pointer to
    // a NUL-terminated string in the process environment, which lives for the
    // process, so a non-NULL result is a valid `'static` C string.
    unsafe {
        let p = getenv(name.as_ptr());
        (!p.is_null()).then(|| CStr::from_ptr(p))
    }
}

/// `$NAME` as UTF-8, or `None` when unset or not UTF-8 (`std::env::var`'s two
/// `Err` cases, which the old std reader also treated as unset).
///
/// `name` is a Rust string; one longer than 63 bytes, or one holding a NUL,
/// is no variable this tree reads and answers `None`.
pub fn var(name: &str) -> Option<&'static str> {
    let mut buf = [0u8; 64];
    if name.len() >= buf.len() || name.as_bytes().contains(&0) {
        return None;
    }
    buf[..name.len()].copy_from_slice(name.as_bytes());
    let cname = CStr::from_bytes_until_nul(&buf).ok()?;
    raw(cname)?.to_str().ok()
}

fn non_empty(v: Option<&'static str>) -> Option<&'static str> {
    v.filter(|s| !s.is_empty())
}

/// `path` as a NUL-terminated buffer for the C library, or `None` when it holds
/// a NUL byte (no file is named by such a path).
#[cfg(feature = "alloc")]
fn c_path(path: &str) -> Option<alloc::vec::Vec<u8>> {
    if path.as_bytes().contains(&0) {
        return None;
    }
    let mut cpath = alloc::vec::Vec::with_capacity(path.len() + 1);
    cpath.extend_from_slice(path.as_bytes());
    cpath.push(0);
    Some(cpath)
}

/// A whole file, through the C library — `std::fs::read_to_string` for a
/// crate with no `std`. `--params-file` reads through it, and so do the
/// C / C++ surfaces' own params-file entries.
///
/// The error is a short reason (`"cannot be opened"`, `"is not UTF-8"`, ...),
/// for the caller to put beside the path it already holds.
#[cfg(feature = "alloc")]
pub fn read_file(path: &str) -> Result<alloc::string::String, &'static str> {
    use core::ffi::c_void;
    unsafe extern "C" {
        fn fopen(path: *const c_char, mode: *const c_char) -> *mut c_void;
        fn fread(ptr: *mut c_void, size: usize, n: usize, f: *mut c_void) -> usize;
        fn ferror(f: *mut c_void) -> i32;
        fn fclose(f: *mut c_void) -> i32;
    }
    let Some(cpath) = c_path(path) else {
        return Err("names a path holding a NUL byte");
    };
    // SAFETY: both strings are NUL-terminated; `f` is checked before use and
    // closed exactly once; `fread` writes at most `chunk.len()` bytes.
    let bytes = unsafe {
        let f = fopen(cpath.as_ptr().cast(), c"rb".as_ptr());
        if f.is_null() {
            return Err("cannot be opened");
        }
        let mut out = alloc::vec::Vec::new();
        let mut chunk = [0u8; 4096];
        loop {
            let n = fread(chunk.as_mut_ptr().cast(), 1, chunk.len(), f);
            out.extend_from_slice(&chunk[..n]);
            if n < chunk.len() {
                break;
            }
        }
        let failed = ferror(f) != 0;
        fclose(f);
        if failed {
            return Err("cannot be read");
        }
        out
    };
    alloc::string::String::from_utf8(bytes).map_err(|_| "is not UTF-8")
}

/// Write `bytes` to `path`, creating or truncating it — `std::fs::write` for a
/// crate with no `std`. The error is a short reason, as for [`read_file`].
#[cfg(feature = "alloc")]
pub fn write_file(path: &str, bytes: &[u8]) -> Result<(), &'static str> {
    use core::ffi::c_void;
    unsafe extern "C" {
        fn fopen(path: *const c_char, mode: *const c_char) -> *mut c_void;
        fn fwrite(ptr: *const c_void, size: usize, n: usize, f: *mut c_void) -> usize;
        fn fclose(f: *mut c_void) -> i32;
    }
    let Some(cpath) = c_path(path) else {
        return Err("names a path holding a NUL byte");
    };
    // SAFETY: both strings are NUL-terminated; `f` is checked before use and
    // closed exactly once; `fwrite` reads at most `bytes.len()` bytes.
    unsafe {
        let f = fopen(cpath.as_ptr().cast(), c"wb".as_ptr());
        if f.is_null() {
            return Err("cannot be opened for writing");
        }
        let written = fwrite(bytes.as_ptr().cast(), 1, bytes.len(), f);
        // `fclose` flushes: a full disk shows up here, not in `fwrite`.
        let closed = fclose(f) == 0;
        if written != bytes.len() || !closed {
            return Err("cannot be written");
        }
    }
    Ok(())
}

/// Rename `from` to `to`, replacing `to` — `std::fs::rename`. Same filesystem
/// only, which is how its callers use it (write-then-rename beside the target).
#[cfg(feature = "alloc")]
pub fn rename(from: &str, to: &str) -> Result<(), &'static str> {
    unsafe extern "C" {
        fn rename(from: *const c_char, to: *const c_char) -> i32;
    }
    let (Some(f), Some(t)) = (c_path(from), c_path(to)) else {
        return Err("names a path holding a NUL byte");
    };
    // SAFETY: both strings are NUL-terminated and live for the call.
    match unsafe { rename(f.as_ptr().cast(), t.as_ptr().cast()) } {
        0 => Ok(()),
        _ => Err("cannot be renamed"),
    }
}

/// Set `$name` to `value` for this process — `std::env::set_var`.
///
/// # Safety
///
/// The process must be single-threaded, or no other thread may read or write
/// the environment during the call: `setenv` is not thread-safe, which is why
/// `std::env::set_var` is `unsafe` too. Its callers run in the boot funnel,
/// before any executor or sched task exists.
#[cfg(feature = "alloc")]
pub unsafe fn set_var(name: &str, value: &str) {
    unsafe extern "C" {
        fn setenv(name: *const c_char, value: *const c_char, overwrite: i32) -> i32;
    }
    let (Some(n), Some(v)) = (c_path(name), c_path(value)) else {
        return;
    };
    // SAFETY: both strings are NUL-terminated; the caller upholds the
    // single-threaded condition.
    unsafe { setenv(n.as_ptr().cast(), v.as_ptr().cast(), 1) };
}

/// The file name of this process's executable (`/proc/self/exe`'s last
/// component) — what `std::env::current_exe()` + `file_name()` gave. `None`
/// when the link cannot be read.
#[cfg(feature = "alloc")]
pub fn exe_name() -> Option<alloc::string::String> {
    unsafe extern "C" {
        fn readlink(path: *const c_char, buf: *mut c_char, len: usize) -> isize;
    }
    let mut buf = [0u8; 4096];
    // SAFETY: the path is NUL-terminated; `readlink` writes at most
    // `buf.len()` bytes and returns how many (it does not NUL-terminate).
    let n = unsafe {
        readlink(
            c"/proc/self/exe".as_ptr(),
            buf.as_mut_ptr().cast(),
            buf.len(),
        )
    };
    let full = core::str::from_utf8(buf.get(..usize::try_from(n).ok()?)?).ok()?;
    Some(alloc::string::String::from(
        full.rsplit('/').next().unwrap_or(full),
    ))
}

/// A key unique to the calling thread among live threads — the address a
/// `thread_local!` would give, without `std`. `pthread_t` on Linux is the
/// thread's descriptor address.
pub fn thread_key() -> usize {
    unsafe extern "C" {
        fn pthread_self() -> usize;
    }
    // SAFETY: `pthread_self` has no preconditions and cannot fail.
    unsafe { pthread_self() }
}

/// End the process with `code`, running `atexit` handlers and flushing stdio —
/// `std::process::exit`.
pub fn exit(code: i32) -> ! {
    unsafe extern "C" {
        fn exit(code: i32) -> !;
    }
    // SAFETY: `exit` has no preconditions; it does not return.
    unsafe { exit(code) }
}

/// **The** answer to "which RMW backend did the user select" — one reader, one
/// semantic, for every consumer.
///
/// This variable had FOUR readers with THREE
/// semantics: `Executor::open` read it as raw OS bytes, `nros`'s
/// `open_session` as a UTF-8 string filtered for empty, `nros-c`'s entry as a
/// string passed through EVEN WHEN EMPTY, and `nros::init` as a string with an
/// `RMW_IMPLEMENTATION` fallback the other three did not have. "Which backend
/// did the user ask for" had four answers in one process.
///
/// Two decisions are baked in, and both are deliberate:
///
/// * **`$NROS_RMW` only.** `$RMW_IMPLEMENTATION` is NOT folded in, though it
///   was tempting and one reader did it. The two carry different vocabularies:
///   this selector is matched against the cffi registry's canonical names
///   (`zenoh`, `dds`, `cyclonedds`), while `RMW_IMPLEMENTATION` holds ROS names
///   (`rmw_cyclonedds_cpp`). Feeding a ROS name to `resolve_backend` yields
///   `Unknown` — an ERROR — where today it is ignored and the unique-backend
///   path runs. Unifying them without a mapping would convert "ignored" into
///   "fails to start". The hosted `init` keeps its fallback for the
///   `Context.rmw` HINT, which is a different quantity.
/// * **Empty or non-UTF-8 means unset.** A name that is not UTF-8 cannot match
///   a registry entry, so treating it as absent is what the caller wants.
///
/// The return is `heapless::String` rather than `&str` because
/// `RMW_SELECTOR_CAP` is a real bound, not a guess: it is the capacity of
/// `Executor::primary_rmw_name`, so a longer value cannot name a registry slot
/// and is reported as unset rather than truncated into a different backend's
/// name.
pub fn rmw_selector() -> Option<nros_core::heapless::String<RMW_SELECTOR_CAP>> {
    let s = non_empty(var("NROS_RMW"))?;
    nros_core::heapless::String::try_from(s).ok()
}

/// The environment as an [`EnvRung`], for [`ExecutorConfig::resolve_with`].
fn env_rung() -> Result<EnvRung<'static>, BootConfigError> {
    // phase-484 W8c — a retired alias is REFUSED, never ignored: an ignored
    // `ZENOH_LOCATOR` dials the default router and reads as "no delivery".
    // Presence alone refuses (an empty value is still somebody's intent), and
    // the refusal is logged here so the C / C++ shims, which only get a
    // return code, still name the replacement. Only presence is tested, so a
    // non-UTF-8 value still refuses.
    for &(name, replacement) in RETIRED_ENV_VARS {
        let mut cname = [0u8; 32];
        cname[..name.len()].copy_from_slice(name.as_bytes());
        let cname = CStr::from_bytes_until_nul(&cname).expect("retired env name fits");
        if raw(cname).is_some() {
            let err = BootConfigError::RetiredEnvVar { name, replacement };
            nros_log::log_error!(nros_log::get_logger("nros"), "{}", err);
            return Err(err);
        }
    }
    // Issue 0330 — a SET locator is the env's answer even when empty; the
    // active backend substitutes its own default for an empty one.
    let locator = var("NROS_LOCATOR");
    let mode = match var("NROS_SESSION_MODE") {
        Some("peer") => SessionMode::Peer,
        _ => SessionMode::Client,
    };
    let domain_id = match non_empty(var("ROS_DOMAIN_ID")) {
        // #206 — malformed is an ERROR, not a silent skip. The parse lives
        // here rather than in the core because only this side has the text.
        Some(s) => Some(
            s.trim()
                .parse::<u32>()
                .map_err(|_| BootConfigError::DomainIdParse)?,
        ),
        None => None,
    };
    let rmw = non_empty(var("NROS_RMW")).filter(|s| s.len() <= RMW_SELECTOR_CAP);
    Ok(EnvRung {
        locator,
        domain_id,
        // Session mode has no baked rung to fall through to, so `Client` is
        // always the answer when the variable says nothing.
        mode: Some(mode),
        // Issue 0794 — an EMPTY name or namespace is not a declaration: `""`
        // is what an unset one already resolves to.
        node_name: non_empty(var("NROS_NODE_NAME")),
        namespace: non_empty(var("NROS_NODE_NAMESPACE")),
        rmw,
    })
}

/// Which rung answered one field, for the boot line.
fn source(env: bool, baked: bool) -> &'static str {
    if env {
        "env"
    } else if baked {
        "baked"
    } else {
        "default"
    }
}

/// The boot line: what each identity field resolved to, and FROM WHERE.
///
/// A misspelled variable does nothing, and without this line nothing says so:
/// the image runs on its baked domain and reads as "no delivery". One line,
/// short enough for `nros_log`'s 256-byte buffer (a body that does not fit is
/// dropped, not truncated), so the locator is the field that can be elided.
fn report(env: &EnvRung<'_>, baked: &BootConfig<'_>, cfg: &ExecutorConfig<'_>) {
    let locator = if cfg.locator.is_empty() {
        "(backend default)"
    } else if cfg.locator.len() > 96 {
        "(long)"
    } else {
        cfg.locator
    };
    nros_log::log_info!(
        nros_log::get_logger("nros"),
        "boot config: domain {} ({}), locator {} ({}), rmw {} ({})",
        cfg.domain_id,
        source(env.domain_id.is_some(), baked.domain_id.is_some()),
        locator,
        source(env.locator.is_some(), baked.locator.is_some()),
        cfg.rmw.unwrap_or("(unique)"),
        source(env.rmw.is_some(), baked.rmw.is_some()),
    );
}

/// The boot-config precedence ladder with the environment on top:
/// `env (var set) > baked > compiled default`.
///
/// Fails loud on invalid identity input (repo rule: a bad domain id at boot is
/// a configuration error, never a silent domain-0 node). FFI shims that need a
/// return code call [`try_resolve_hosted`].
pub fn resolve_hosted<'a>(baked: BootConfig<'a>) -> ExecutorConfig<'a> {
    match try_resolve_hosted(baked) {
        Ok(cfg) => cfg,
        Err(e) => panic!("nros boot-config resolution failed: {e}"),
    }
}

/// Fallible [`resolve_hosted`] — returns [`BootConfigError`] instead of
/// panicking, so the C / C++ FFI shims can surface a return code.
///
/// - `ROS_DOMAIN_ID` set but non-numeric → `DomainIdParse` (the older C++
///   header silently collapsed this to domain 0; the resolver silently ignored
///   it — both were wrong).
/// - any resolved domain id > `DOMAIN_ID_MAX` → `DomainIdRange`, INCLUDING a
///   baked one (the DDS backend would only fail later).
///
/// Logs one line naming each field's source (`report`).
pub fn try_resolve_hosted<'a>(
    baked: BootConfig<'a>,
) -> Result<ExecutorConfig<'a>, BootConfigError> {
    let env = env_rung()?;
    let cfg = ExecutorConfig::try_resolve_with(baked, Some(env))?;
    report(&env, &baked, &cfg);
    Ok(cfg)
}

/// Where the environment overrides a value the image's
/// `system.toml` STATED, each as `(env, stated)`. Only a value both sides
/// state and that differs is reported: an unstated one has nothing to
/// override, and an equal one changes nothing.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct StatementOverrides<'a> {
    /// `$ROS_DOMAIN_ID` against `domain_id`.
    pub domain_id: Option<(u32, u32)>,
    /// `$NROS_LOCATOR` against `locator`.
    pub locator: Option<(&'a str, &'a str)>,
}

/// The pure decision behind [`warn_env_overrides_statement`].
pub fn env_overrides_statement<'a>(
    stated: &BootConfig<'a>,
    env: &EnvRung<'a>,
) -> StatementOverrides<'a> {
    StatementOverrides {
        domain_id: match (env.domain_id, stated.domain_id) {
            (Some(e), Some(s)) if e != s => Some((e, s)),
            _ => None,
        },
        locator: match (env.locator, stated.locator) {
            (Some(e), Some(s)) if e != s => Some((e, s)),
            _ => None,
        },
    }
}

/// On a host the environment WINS over what the image's
/// `system.toml` states (a user ruling, for testing convenience), and says so
/// at boot, one warning per field, through `nros_log`.
///
/// `stated` is the image's statement and nothing else, which is why only the
/// `linux` board and the host C/C++ runners call this, where the baked rung
/// IS the statement. An RTOS simulator's baked rung is its board config
/// whether or not anything was stated, so it would report a default as a
/// statement. A malformed environment is not reported here: the resolver
/// refuses it, and says why, right after.
pub fn warn_env_overrides_statement(stated: &BootConfig<'_>) {
    let Ok(env) = env_rung() else {
        return;
    };
    let o = env_overrides_statement(stated, &env);
    if let Some((e, s)) = o.domain_id {
        nros_log::log_warn!(
            nros_log::get_logger("nros"),
            "ROS_DOMAIN_ID={} overrides system.toml's domain_id={}",
            e,
            s
        );
    }
    if let Some((e, s)) = o.locator {
        nros_log::log_warn!(
            nros_log::get_logger("nros"),
            "NROS_LOCATOR={} overrides system.toml's locator={}",
            e,
            s
        );
    }
}

/// `ExecutorConfig::from_env()`, as an extension trait.
///
/// This was an inherent method on `ExecutorConfig`, which is
/// defined in `nros-node`; an inherent method cannot be moved to another crate
/// and keep its spelling, and the spelling is what ~26 call sites (most of them
/// user-facing native examples) are written against. A trait keeps
/// `ExecutorConfig::from_env()` working wherever it is in scope — the
/// [`prelude`](crate::prelude) carries it, so a consumer that already writes
/// `use nros::prelude::*` changes nothing at all.
#[cfg(feature = "env")]
pub trait ExecutorConfigEnvExt {
    /// Create a configuration from environment variables.
    ///
    /// Reads:
    /// - `NROS_LOCATOR` — Middleware locator. Unset ⇒ empty (issue 0330: the
    ///   active RMW backend applies its own default; e.g. zenoh dials
    ///   `nros_rmw_zenoh::DEFAULT_LOCATOR`). The retired name `ZENOH_LOCATOR`
    ///   is refused (phase-484 W8c) — see [`BootConfigError::RetiredEnvVar`].
    /// - `ROS_DOMAIN_ID` — ROS 2 domain ID (default: `0`).
    /// - `NROS_SESSION_MODE` — `"client"` or `"peer"` (default: `"client"`).
    ///   The retired name `ZENOH_MODE` is refused (phase-484 W8c).
    /// - `NROS_RMW` — the backend selector, through [`rmw_selector`]. issue
    ///   0687: `Executor::open` used to read this itself; it now takes
    ///   `ExecutorConfig::rmw`, so a config built here still selects the
    ///   backend the user named.
    ///
    /// The returned `&'static str` fields point into the process environment,
    /// read live on every call.
    ///
    /// **Panics** on a malformed or out-of-range `$ROS_DOMAIN_ID` or a set
    /// retired alias (`ZENOH_LOCATOR` / `ZENOH_MODE`), like
    /// [`resolve_hosted`] and the C / C++ entries. A boot identity that cannot
    /// be resolved is a configuration error; the alternative is a node silently
    /// running on domain 0, which is what issue #206 removed everywhere else.
    fn from_env() -> ExecutorConfig<'static>;
}

#[cfg(feature = "env")]
impl ExecutorConfigEnvExt for ExecutorConfig<'static> {
    fn from_env() -> ExecutorConfig<'static> {
        // issue 0687 — `from_env` IS `resolve_hosted` with nothing baked. The
        // one field that does NOT come from the rung is `node_name`: `from_env`
        // has never honoured `$NROS_NODE_NAME` (its doc lists what it reads),
        // and callers chain `.node_name(..)` immediately.
        ExecutorConfig {
            node_name: "node",
            ..resolve_hosted(BootConfig::default())
        }
    }
}

/// Process-wide mutex that serialises every test that touches the process
/// environment — the ones that MUTATE it (`EnvGuard` below) and the ones that
/// only READ it through `init()` / `Context::from_env()` (`init.rs`).
///
/// `cargo test` runs `#[test]`s in parallel within a single binary by default,
/// and the reader reads the live environment on every call. So a reader that
/// does not hold this lock can
/// observe a sibling's `ROS_DOMAIN_ID=abc` halfway through: `init.rs`'s
/// `plain_args_pass_through_to_init` compared two reads and got `Ok(..)` vs
/// `Err(EnvParseFailed)` under `just ci gate`. It lived in `env.rs`'s private
/// test module, so the readers in `init.rs` could not take it. (`cargo
/// nextest` runs each test in its own process, so the lock is uncontended
/// there, but taking it is still correct.)
///
/// Poison-TOLERANT, deliberately: three tests assert that a bad
/// `$ROS_DOMAIN_ID` panics, and a panic while this guard is alive poisons the
/// mutex — which would then fail every LATER test for a reason that has
/// nothing to do with what it asserts (measured: 3 intended failures became
/// 8). The data behind the lock is `()`; there is no invariant a panic could
/// have broken.
#[cfg(all(test, feature = "env"))]
pub(crate) fn test_env_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::OnceLock<std::sync::Mutex<()>> = std::sync::OnceLock::new();
    LOCK.get_or_init(|| std::sync::Mutex::new(()))
        .lock()
        .unwrap_or_else(|e| e.into_inner())
}

#[cfg(all(test, feature = "env"))]
mod tests {
    use super::*;

    /// Issue 2714's user ruling, the whole grid: the environment overriding
    /// a value is reported exactly when BOTH state it and they differ — for
    /// the domain and the locator alike, each on its own.
    #[test]
    fn an_override_is_reported_exactly_when_both_state_it_and_differ() {
        let env = |d: Option<u32>, l: Option<&'static str>| EnvRung {
            domain_id: d,
            locator: l,
            ..Default::default()
        };
        let stated = |d: Option<u32>, l: Option<&'static str>| BootConfig {
            domain_id: d,
            locator: l,
            ..Default::default()
        };
        for (e, st, want) in [
            (None, None, None),
            (Some(5), None, None),
            (None, Some(7), None),
            (Some(7), Some(7), None),
            (Some(5), Some(7), Some((5, 7))),
            (Some(5), Some(0), Some((5, 0))),
        ] {
            assert_eq!(
                env_overrides_statement(&stated(st, None), &env(e, None)).domain_id,
                want,
                "domain: env {e:?}, stated {st:?}"
            );
        }
        for (e, st, want) in [
            (None, None, None),
            (Some("tcp/e:1"), None, None),
            (None, Some("tcp/s:2"), None),
            (Some("tcp/s:2"), Some("tcp/s:2"), None),
            (
                Some("tcp/e:1"),
                Some("tcp/s:2"),
                Some(("tcp/e:1", "tcp/s:2")),
            ),
        ] {
            assert_eq!(
                env_overrides_statement(&stated(None, st), &env(None, e)).locator,
                want,
                "locator: env {e:?}, stated {st:?}"
            );
        }
        // Each field on its own: a differing domain says nothing about an
        // agreeing locator.
        let both = env_overrides_statement(
            &stated(Some(7), Some("tcp/s:2")),
            &env(Some(5), Some("tcp/s:2")),
        );
        assert_eq!(
            both,
            StatementOverrides {
                domain_id: Some((5, 7)),
                locator: None
            }
        );
    }

    fn env_lock() -> std::sync::MutexGuard<'static, ()> {
        super::test_env_lock()
    }

    /// RAII guard that saves and restores a single env var.
    struct EnvGuard {
        key: &'static str,
        prev: Option<std::ffi::OsString>,
    }

    impl EnvGuard {
        fn set(key: &'static str, value: &str) -> Self {
            let prev = std::env::var_os(key);
            // SAFETY: serialised via env_lock().
            unsafe { std::env::set_var(key, value) };
            Self { key, prev }
        }

        fn unset(key: &'static str) -> Self {
            let prev = std::env::var_os(key);
            // SAFETY: serialised via env_lock().
            unsafe { std::env::remove_var(key) };
            Self { key, prev }
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            // SAFETY: serialised via env_lock().
            unsafe {
                match &self.prev {
                    Some(v) => std::env::set_var(self.key, v),
                    None => std::env::remove_var(self.key),
                }
            }
        }
    }

    /// `resolve_hosted(BootConfig::default())` must be field-for-field
    /// identical to `from_env()`, regardless of what env vars are set.
    #[test]
    fn noop_resolve_matches_from_env() {
        let _g = env_lock();

        let resolved = resolve_hosted(BootConfig::default());
        let env_cfg = ExecutorConfig::from_env();

        assert_eq!(resolved.locator, env_cfg.locator);
        assert_eq!(resolved.mode, env_cfg.mode);
        assert_eq!(resolved.domain_id, env_cfg.domain_id);
        assert_eq!(resolved.node_name, env_cfg.node_name);
        assert_eq!(resolved.namespace, env_cfg.namespace);
        assert_eq!(resolved.rmw, env_cfg.rmw);
    }

    #[test]
    fn env_overrides_baked_on_hosted() {
        let _g = env_lock();
        let _e = EnvGuard::set("NROS_LOCATOR", "tcp/env:7447");

        let baked = BootConfig {
            locator: Some("tcp/baked:9999"),
            ..Default::default()
        };
        let resolved = resolve_hosted(baked);

        // Issue 0607 — the exact env string is observable: the reader has no
        // process-global cache for an earlier test to have frozen.
        assert_eq!(
            resolved.locator, "tcp/env:7447",
            "env locator must win over baked, with its own value"
        );
    }

    #[test]
    fn baked_used_when_env_unset_on_hosted() {
        let _g = env_lock();
        let _e1 = EnvGuard::unset("NROS_LOCATOR");

        let baked = BootConfig {
            locator: Some("tcp/baked-only:8888"),
            ..Default::default()
        };
        let resolved = resolve_hosted(baked);

        assert_eq!(
            resolved.locator, "tcp/baked-only:8888",
            "baked locator must be used when env var is absent"
        );
    }

    #[test]
    fn per_field_independence_baked_name_env_locator() {
        let _g = env_lock();
        let _e = EnvGuard::set("NROS_LOCATOR", "tcp/env:7447");
        let _n = EnvGuard::unset("NROS_NODE_NAME");

        let baked = BootConfig {
            node_name: Some("my_talker"),
            ..Default::default()
        };
        let resolved = resolve_hosted(baked);

        assert_eq!(resolved.locator, "tcp/env:7447");
        assert_eq!(
            resolved.node_name, "my_talker",
            "baked node_name must apply even when locator comes from env"
        );
    }

    #[test]
    fn try_resolve_malformed_domain_env_errors() {
        let _l = env_lock();
        let _g = EnvGuard::set("ROS_DOMAIN_ID", "not-a-number");
        let err = match try_resolve_hosted(BootConfig::default()) {
            Err(e) => e,
            Ok(_) => panic!("expected DomainIdParse error"),
        };
        assert_eq!(err, BootConfigError::DomainIdParse);
    }

    /// The retired name of `replacement` in [`RETIRED_ENV_VARS`] (the table is
    /// sorted by the replacing row's name, so a position names nothing).
    fn alias_of(replacement: &str) -> &'static str {
        RETIRED_ENV_VARS
            .iter()
            .find(|(_, r)| *r == replacement)
            .map(|(a, _)| *a)
            .expect("a retired alias of the replacement")
    }

    /// RFC-0140 D4 (phase-581 W6) -- EVERY `run:` retirement refuses the boot,
    /// naming its replacement: generated from the registry's table, so a new
    /// retirement (`XRCE_SERIAL_BAUD`, the Cyclone test controls) is a new case.
    #[test]
    fn every_retired_run_time_name_is_refused_naming_its_replacement() {
        let _l = env_lock();
        let _guards: alloc::vec::Vec<_> = RETIRED_ENV_VARS
            .iter()
            .map(|&(a, _)| EnvGuard::unset(a))
            .collect();
        assert!(RETIRED_ENV_VARS.len() >= 5, "{RETIRED_ENV_VARS:?}");
        for &(name, replacement) in RETIRED_ENV_VARS {
            let _set = EnvGuard::set(name, "1");
            let err = match try_resolve_hosted(BootConfig::default()) {
                Err(e) => e,
                Ok(_) => panic!("{name} set: expected RetiredEnvVar"),
            };
            assert_eq!(err, BootConfigError::RetiredEnvVar { name, replacement });
            assert!(alloc::format!("{err}").contains(replacement), "{err}");
        }
    }

    /// phase-484 W8c — a retired alias REFUSES, naming its replacement, and
    /// does so even beside a canonical value (half-migrated env is still a
    /// mistake somebody should hear about).
    #[test]
    fn retired_zenoh_locator_is_refused_naming_nros_locator() {
        let _l = env_lock();
        let locator_alias = alias_of("NROS_LOCATOR");
        let mode_alias = alias_of("NROS_SESSION_MODE");
        let _a = EnvGuard::set(locator_alias, "tcp/legacy:7447");
        let _b = EnvGuard::unset("NROS_LOCATOR");
        let _c = EnvGuard::unset(mode_alias);
        let err = match try_resolve_hosted(BootConfig::default()) {
            Err(e) => e,
            Ok(cfg) => panic!("expected RetiredEnvVar, resolved {:?}", cfg.locator),
        };
        assert_eq!(
            err,
            BootConfigError::RetiredEnvVar {
                name: locator_alias,
                replacement: "NROS_LOCATOR"
            }
        );
        let msg = alloc::format!("{err}");
        assert!(msg.contains("NROS_LOCATOR"), "{msg}");

        let _d = EnvGuard::set("NROS_LOCATOR", "tcp/canonical:7447");
        assert!(try_resolve_hosted(BootConfig::default()).is_err());
    }

    #[test]
    fn retired_zenoh_mode_is_refused_naming_nros_session_mode() {
        let _l = env_lock();
        let locator_alias = alias_of("NROS_LOCATOR");
        let mode_alias = alias_of("NROS_SESSION_MODE");
        let _a = EnvGuard::unset(locator_alias);
        let _b = EnvGuard::set(mode_alias, "peer");
        let err = match try_resolve_hosted(BootConfig::default()) {
            Err(e) => e,
            Ok(_) => panic!("expected RetiredEnvVar"),
        };
        assert_eq!(
            err,
            BootConfigError::RetiredEnvVar {
                name: mode_alias,
                replacement: "NROS_SESSION_MODE"
            }
        );
    }

    /// Positive control for the two refusals above: the canonical name alone
    /// resolves, with its own value.
    #[test]
    fn canonical_nros_locator_resolves_when_no_retired_alias_set() {
        let _l = env_lock();
        let _a = EnvGuard::unset(alias_of("NROS_LOCATOR"));
        let _b = EnvGuard::unset(alias_of("NROS_SESSION_MODE"));
        let _c = EnvGuard::set("NROS_LOCATOR", "tcp/canonical:7447");
        let cfg = try_resolve_hosted(BootConfig::default()).expect("canonical resolves");
        assert_eq!(cfg.locator, "tcp/canonical:7447");
    }

    #[test]
    fn try_resolve_domain_env_over_max_errors() {
        let _l = env_lock();
        let _g = EnvGuard::set("ROS_DOMAIN_ID", "233");
        let err = match try_resolve_hosted(BootConfig::default()) {
            Err(e) => e,
            Ok(_) => panic!("expected DomainIdRange error"),
        };
        assert_eq!(err, BootConfigError::DomainIdRange);
    }

    #[test]
    fn try_resolve_node_name_env_rung() {
        let _l = env_lock();
        let _g = EnvGuard::set("NROS_NODE_NAME", "env_node");
        let baked = BootConfig {
            node_name: Some("baked_node"),
            ..BootConfig::default()
        };
        let cfg = match try_resolve_hosted(baked) {
            Ok(c) => c,
            Err(e) => panic!("resolve failed: {e}"),
        };
        assert_eq!(
            cfg.node_name, "env_node",
            "env rung must override baked with its own value"
        );
    }

    /// issue 0794 — the hosted edge fills the namespace rung.
    ///
    /// Before, `EnvRung` had no namespace field, so this variable could not
    /// exist: a hosted node's namespace came from the baked rung or from a
    /// hand-written `ExecutorConfig::namespace(..)` — the per-board lore
    /// RFC-0045 exists to abolish.
    #[test]
    fn try_resolve_namespace_env_rung() {
        let _l = env_lock();
        let baked = BootConfig {
            namespace: Some("/baked"),
            ..BootConfig::default()
        };

        let g = EnvGuard::set("NROS_NODE_NAMESPACE", "/robot1");
        let cfg = try_resolve_hosted(baked).expect("resolve ok");
        assert_eq!(
            cfg.namespace, "/robot1",
            "env rung must override the baked namespace with its own value"
        );
        drop(g);

        // Unset falls through to baked, which is the rung below it.
        let _g = EnvGuard::unset("NROS_NODE_NAMESPACE");
        let cfg = try_resolve_hosted(baked).expect("resolve ok");
        assert_eq!(cfg.namespace, "/baked");

        // EMPTY is unset, not "override back to root": `""` is already what an
        // unset namespace resolves to, so honouring it would give `NROS_NODE_
        // NAMESPACE=` a meaning no caller can distinguish from absence.
        let _g = EnvGuard::set("NROS_NODE_NAMESPACE", "");
        let cfg = try_resolve_hosted(baked).expect("resolve ok");
        assert_eq!(cfg.namespace, "/baked");
    }

    /// issue 0687 — the selector reaches the config, which is how
    /// `Executor::open` still honours `$NROS_RMW` now that it does not read the
    /// environment itself. Both halves are asserted: a set value arrives, and
    /// an EMPTY value means unset rather than "a backend named the empty
    /// string" (which is what `nros-c` used to pass through).
    #[test]
    fn selector_reaches_the_config() {
        let _l = env_lock();

        let _g = EnvGuard::set("NROS_RMW", "cyclonedds");
        assert_eq!(rmw_selector().as_deref(), Some("cyclonedds"));
        assert_eq!(
            ExecutorConfig::from_env().rmw,
            Some("cyclonedds"),
            "from_env must carry the selector"
        );
        assert_eq!(
            resolve_hosted(BootConfig::default()).rmw,
            Some("cyclonedds")
        );

        let _g = EnvGuard::set("NROS_RMW", "");
        assert_eq!(rmw_selector(), None, "empty means unset");
        assert_eq!(ExecutorConfig::from_env().rmw, None);
    }

    /// issue 0687 — `from_env` and `resolve_hosted(default)` are the same
    /// resolution, and this asserts the fields the type does not force.
    #[test]
    fn from_env_is_resolve_hosted_with_nothing_baked() {
        let _l = env_lock();
        let _g = EnvGuard::set("NROS_LOCATOR", "tcp/agree:7447");
        let _d = EnvGuard::set("ROS_DOMAIN_ID", "42");

        let a = ExecutorConfig::from_env();
        let b = resolve_hosted(BootConfig::default());
        assert_eq!(a.locator, b.locator);
        assert_eq!(a.domain_id, b.domain_id);
        assert_eq!(a.mode, b.mode);
        assert_eq!(a.rmw, b.rmw);
        assert_eq!(a.namespace, b.namespace);
        // The one deliberate divergence: `from_env` does not take the node name
        // from the environment, and never has.
        assert_eq!(a.node_name, "node");
    }

    /// A malformed `$ROS_DOMAIN_ID` must fail LOUD here, as it does for
    /// `resolve_hosted` and the C / C++ entries. It used to resolve to domain 0
    /// silently — the #206 defect, surviving on this one path.
    #[test]
    #[should_panic(expected = "boot-config resolution failed")]
    fn from_env_rejects_a_malformed_domain() {
        let _l = env_lock();
        let _g = EnvGuard::set("ROS_DOMAIN_ID", "not-a-number");
        let _ = ExecutorConfig::from_env();
    }

    /// …and an out-of-range one, which the same path used to coerce to 0.
    #[test]
    #[should_panic(expected = "boot-config resolution failed")]
    fn from_env_rejects_an_out_of_range_domain() {
        let _l = env_lock();
        let _g = EnvGuard::set("ROS_DOMAIN_ID", "300");
        let _ = ExecutorConfig::from_env();
    }

    /// A selector longer than the executor's identity capacity cannot name a
    /// registry slot, so it is unset rather than truncated into some other
    /// backend's name.
    #[test]
    fn overlong_selector_is_unset_not_truncated() {
        let _l = env_lock();
        let long = "x".repeat(RMW_SELECTOR_CAP + 1);
        let _g = EnvGuard::set("NROS_RMW", &long);
        assert_eq!(rmw_selector(), None);
    }
}
