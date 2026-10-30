//! Which `make` a build script's nested CMake build runs — issue 2058.
//!
//! The fixture drivers run the PINNED make 4.4 with `--jobserver-style=fifo`,
//! so every descendant inherits a jobserver named `fifo:<path>`; cargo hands
//! it on to a build script as `CARGO_MAKEFLAGS`, and the `cmake` crate passes
//! it to `cmake --build` as `MAKEFLAGS`. A "Unix Makefiles" configure caches
//! whichever `gmake`/`make` it finds on `PATH` as `CMAKE_MAKE_PROGRAM`, and a
//! system make 4.3 cannot parse a fifo auth string: it stops with
//! `internal error: invalid --jobserver-auth string 'fifo:…'`.
//!
//! Issue 2119 fixed the drivers by putting the pinned make's directory first
//! on `PATH`, which keeps the answer correct only for as long as every road
//! into the build script prepends it (the gate `check-pinned-make-path-exported`
//! covers the in-tree scripts, nothing else). This is the build script's own
//! half: it names the program itself, CONSTRUCTED from the index pin the way
//! `nros_pinned_make` (scripts/build/jobserver-pool.sh) constructs it — never
//! searched on `PATH`, because a 4.3 first on `PATH` is the failure.
//!
//! The nested build JOINS the pool rather than leaving it: stripping
//! `MAKEFLAGS` would also build, but with `-j` and no jobserver inside a
//! 32-way pool.

use std::path::{Path, PathBuf};

/// The pinned GNU make the store provisions (`[tool.make]` in
/// `nros-sdk-index.toml`), if it is there: `<store>/sdk/make/<pin>/bin/make`.
///
/// `None` out of tree (no index, hence no pin) and when the pin is not
/// provisioned. Never lists the store for another version (issue 1546).
#[must_use]
pub fn pinned_make() -> Option<PathBuf> {
    let pin = crate::sdk_pinned_version("make")?;
    let make = crate::store::try_root()
        .ok()?
        .join("sdk")
        .join("make")
        .join(pin)
        .join("bin")
        .join("make");
    make.is_file().then_some(make)
}

/// Does a `MAKEFLAGS` value name a FIFO jobserver (make 4.4's
/// `--jobserver-style=fifo`)? `--jobserver-fds` is the pre-4.2 spelling make
/// still emits beside `--jobserver-auth`; either naming `fifo:` counts.
#[must_use]
pub fn names_fifo_jobserver(makeflags: &str) -> bool {
    makeflags.split_whitespace().any(|w| {
        w.strip_prefix("--jobserver-auth=")
            .or_else(|| w.strip_prefix("--jobserver-fds="))
            .is_some_and(|v| v.starts_with("fifo:"))
    })
}

/// What a nested CMake build should be told about its make.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MakeProgram {
    /// Set `CMAKE_MAKE_PROGRAM` to this make.
    Pinned(PathBuf),
    /// Leave CMake's own search alone.
    Ambient,
    /// A FIFO jobserver is inherited and no pinned make is provisioned: the
    /// `PATH` make answers, and if it is older than 4.4 the build stops. The
    /// string is the warning to print.
    AmbientUnderFifo(String),
}

/// The decision, over its inputs so it is testable without a store.
///
/// The pinned make is used whenever it is provisioned, not only under a fifo
/// jobserver: `CMAKE_MAKE_PROGRAM` is CACHED, so a build dir configured once
/// with the pinned make keeps it, and a run that switched back to `PATH`'s
/// make whenever no jobserver was inherited would make the cache depend on
/// which road last re-ran this build script.
#[must_use]
pub fn choose(pinned: Option<&Path>, makeflags: Option<&str>) -> MakeProgram {
    if let Some(p) = pinned {
        return MakeProgram::Pinned(p.to_path_buf());
    }
    match makeflags {
        Some(f) if names_fifo_jobserver(f) => MakeProgram::AmbientUnderFifo(
            "a FIFO jobserver is inherited (`--jobserver-auth=fifo:`) and the pinned make \
             is not provisioned, so CMake uses the make on PATH; a make older than 4.4 stops \
             with `invalid --jobserver-auth string`. Provision it: `nros setup --tool make` \
             (issue 2058)"
                .to_string(),
        ),
        _ => MakeProgram::Ambient,
    }
}

/// [`choose`] over this build script's environment: the store's pinned make
/// and the jobserver cargo hands the script (`CARGO_MAKEFLAGS`, which is what
/// the `cmake` crate forwards as `MAKEFLAGS`).
#[must_use]
pub fn make_program() -> MakeProgram {
    let flags = std::env::var("CARGO_MAKEFLAGS").ok();
    choose(pinned_make().as_deref(), flags.as_deref())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fifo_auth_is_recognised_in_both_spellings() {
        assert!(names_fifo_jobserver(
            "-j --jobserver-fds=fifo:/tmp/GMfifo1 --jobserver-auth=fifo:/tmp/GMfifo1"
        ));
        assert!(names_fifo_jobserver(
            "-j32 --jobserver-auth=fifo:/tmp/GMfifo2"
        ));
        assert!(names_fifo_jobserver("--jobserver-fds=fifo:/x"));
    }

    #[test]
    fn a_pipe_jobserver_or_none_is_not_fifo() {
        assert!(!names_fifo_jobserver(
            "-j --jobserver-fds=3,4 --jobserver-auth=3,4"
        ));
        assert!(!names_fifo_jobserver("-j8"));
        assert!(!names_fifo_jobserver(""));
        // A path that merely CONTAINS "fifo:" is not a fifo auth.
        assert!(!names_fifo_jobserver("--jobserver-auth=3,4 VAR=fifo:/x"));
    }

    #[test]
    fn a_provisioned_pin_is_used_with_or_without_a_jobserver() {
        let make = Path::new("/store/sdk/make/4.4.1/bin/make");
        for flags in [None, Some("-j8"), Some("--jobserver-auth=fifo:/f")] {
            assert_eq!(
                choose(Some(make), flags),
                MakeProgram::Pinned(make.to_path_buf())
            );
        }
    }

    #[test]
    fn no_pin_under_a_fifo_jobserver_warns_and_otherwise_stays_quiet() {
        assert!(matches!(
            choose(None, Some("-j --jobserver-auth=fifo:/tmp/GMfifo3")),
            MakeProgram::AmbientUnderFifo(_)
        ));
        assert_eq!(
            choose(None, Some("--jobserver-auth=3,4")),
            MakeProgram::Ambient
        );
        assert_eq!(choose(None, None), MakeProgram::Ambient);
    }
}
