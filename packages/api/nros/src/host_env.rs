//! Issue 1763 — the boot-config ENV rung for a `#![no_std]` image that still
//! runs inside a host process, read through the C library's `getenv`.
//!
//! [`crate::env`] reads the same variables through `std::env`, so it needs
//! `std`. Until issue 1763 every image that had an environment also had `std`,
//! so that was one fact. It stopped being one when the host simulators dropped
//! libstd: threadx-linux's C/C++ runtime staticlib (issue 1763) and
//! freertos-posix's (issue 1778) are `alloc`, but they are still Linux
//! processes with a C library and an environment. Without this module those
//! images silently stopped honouring `$ROS_DOMAIN_ID` and `$NROS_LOCATOR`.
//! That was measured on freertos-posix: the C and C++ cells, run in parallel
//! with distinct domains, both fell back to the baked domain and heard each
//! other, so the C++ listener printed a `Received:` before its own talker
//! published.
//!
//! The variables, their precedence and their errors are the SAME as
//! [`crate::env`]'s. Only the reader differs. Issue 0687's rule still holds:
//! the edge that has an environment reads it, and the core takes values. The
//! edge is "a C library is linked", which on this tree is `target_os =
//! "linux"`. A cross target has no `getenv` to call, and this module does not
//! exist there.
//!
//! Strings are borrowed from the process environment, which outlives the boot.
//! Nothing in nano-ros calls `setenv` after boot, and the std reader made the
//! same assumption by caching its first read.

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

/// `$NAME` as UTF-8, or `None` when unset or not UTF-8 (`std::env::var`'s
/// `Err` cases, which the std reader also treats as unset).
fn var(name: &CStr) -> Option<&'static str> {
    raw(name)?.to_str().ok()
}

fn non_empty(v: Option<&'static str>) -> Option<&'static str> {
    v.filter(|s| !s.is_empty())
}

/// The env rung, read with `getenv`. Mirrors `crate::env::env_rung`.
fn env_rung() -> Result<EnvRung<'static>, BootConfigError> {
    // phase-484 W8c — mirrors `crate::env::env_rung`: a set retired alias is
    // refused (and logged, for the C / C++ shims that only see a return code).
    for &(name, replacement) in RETIRED_ENV_VARS {
        let mut cname = [0u8; 32];
        cname[..name.len()].copy_from_slice(name.as_bytes());
        let cname = CStr::from_bytes_until_nul(&cname).expect("retired env name fits");
        // Only presence is tested, so a non-UTF-8 value still refuses.
        if raw(cname).is_some() {
            let err = BootConfigError::RetiredEnvVar { name, replacement };
            nros_log::log_error!(nros_log::get_logger("nros"), "{}", err);
            return Err(err);
        }
    }
    let locator = var(c"NROS_LOCATOR");
    let mode_str = var(c"NROS_SESSION_MODE");
    let mode = match mode_str {
        Some("peer") => SessionMode::Peer,
        _ => SessionMode::Client,
    };
    let domain_id = match non_empty(var(c"ROS_DOMAIN_ID")) {
        Some(s) => Some(
            s.trim()
                .parse::<u32>()
                .map_err(|_| BootConfigError::DomainIdParse)?,
        ),
        None => None,
    };
    let rmw = non_empty(var(c"NROS_RMW")).filter(|s| s.len() <= RMW_SELECTOR_CAP);
    Ok(EnvRung {
        locator,
        domain_id,
        mode: Some(mode),
        node_name: non_empty(var(c"NROS_NODE_NAME")),
        namespace: non_empty(var(c"NROS_NODE_NAMESPACE")),
        rmw,
    })
}

/// [`ExecutorConfig::try_resolve_with`] with the `getenv` rung — the
/// `#![no_std]` hosted twin of [`crate::env::try_resolve_hosted`].
pub fn try_resolve_hosted<'a>(
    baked: BootConfig<'a>,
) -> Result<ExecutorConfig<'a>, BootConfigError> {
    ExecutorConfig::try_resolve_with(baked, Some(env_rung()?))
}
