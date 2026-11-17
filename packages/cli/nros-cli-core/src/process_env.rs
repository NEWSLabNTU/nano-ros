//! Environment the `nros` process sets for itself, so every child inherits it.

use std::ffi::OsString;

/// The cargo switch issue 2671 needs: fetch git dependencies with the git CLI.
pub const CARGO_GIT_FETCH_WITH_CLI: &str = "CARGO_NET_GIT_FETCH_WITH_CLI";

/// The `(name, value)` to set at startup, or `None` when the caller already
/// set the variable (their value wins, whatever it is). `get` reads the
/// current environment, so the rule is testable without touching it.
///
/// Cargo's built-in libgit2 failed the `ros-launch-manifest` git-tag fetch
/// with an `unknown error` TLS failure on host 118, twice, where `git ls-remote`
/// of the same URL passed every time. `net.git-fetch-with-cli` is cargo's own
/// transport switch, not a retry. Set in the process rather than in a
/// rendered cargo config, because a config reaches only the cargo command it
/// is passed to, and the cargo CMake, Corrosion, west and the metadata harness
/// run is a child of this process too.
pub fn cargo_git_fetch_default(
    get: impl Fn(&str) -> Option<OsString>,
) -> Option<(&'static str, &'static str)> {
    get(CARGO_GIT_FETCH_WITH_CLI)
        .is_none()
        .then_some((CARGO_GIT_FETCH_WITH_CLI, "true"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unset_is_set_to_true() {
        assert_eq!(
            cargo_git_fetch_default(|_| None),
            Some((CARGO_GIT_FETCH_WITH_CLI, "true"))
        );
    }

    #[test]
    fn a_callers_value_wins() {
        for v in ["false", "true", ""] {
            assert_eq!(
                cargo_git_fetch_default(|_| Some(OsString::from(v))),
                None,
                "{v:?}"
            );
        }
    }
}
