//! Where the store is — RFC-0095 D2, one variable since RFC-0103 D6.
//!
//! The implementation is `nros_build_paths::store` (the one Rust spelling,
//! shared with every build script); this module keeps the names the launcher
//! and `nros_cli_core::orchestration::store` have always called.
//!
//! Never a literal `~/.nros`: `$NROS_HOME` is how a distrobox gets its own
//! store while sharing `$HOME` (issue 1248).

use std::path::PathBuf;

pub use nros_build_paths::store::retired_in_env;

/// The store root: `$NROS_HOME`, else `$HOME/.nros`. A retired variable
/// (`NROS_STORE`, `NROS_SDK_STORE`) is refused by [`retired_in_env`], which
/// both binaries' `main` checks before anything resolves the store.
#[must_use]
pub fn root() -> PathBuf {
    nros_build_paths::store::root_unchecked()
}

/// Which environment variable answered [`root`] — for the header line, so a
/// reader never has to guess whose store they are about to shrink.
#[must_use]
pub fn root_origin() -> &'static str {
    nros_build_paths::store::root_origin()
}
