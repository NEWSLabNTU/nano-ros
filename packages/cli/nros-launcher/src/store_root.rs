//! Where the store is — RFC-0095 D2's one answer.
//!
//! MOVED from `nros_cli_core::orchestration::store` (which re-exports it as
//! `store::root` / `store::root_origin`, its long-standing spelling). The
//! launcher constructs every path it touches under this root, so it cannot
//! depend on the toolchain crate to tell it where the root is — and a second
//! implementation is how a launcher comes to look in a different store from the
//! `nros toolchain` verbs that fill it.
//!
//! Never a literal `~/.nros`: `$NROS_STORE` is how a distrobox gets its own
//! store while sharing `$HOME` (issue 1248).

use std::path::PathBuf;

/// The store root: `$NROS_STORE`, else `$NROS_HOME`, else `$HOME/.nros`.
#[must_use]
pub fn root() -> PathBuf {
    if let Some(s) = std::env::var_os("NROS_STORE") {
        return PathBuf::from(s);
    }
    if let Some(h) = std::env::var_os("NROS_HOME") {
        return PathBuf::from(h);
    }
    if let Some(h) = std::env::var_os("HOME") {
        return PathBuf::from(h).join(".nros");
    }
    PathBuf::from(".nros")
}

/// Which environment variable answered [`root`] — for the header line, so a
/// reader never has to guess whose store they are about to shrink.
#[must_use]
pub fn root_origin() -> &'static str {
    if std::env::var_os("NROS_STORE").is_some() {
        "$NROS_STORE"
    } else if std::env::var_os("NROS_HOME").is_some() {
        "$NROS_HOME"
    } else if std::env::var_os("HOME").is_some() {
        "$HOME/.nros"
    } else {
        "./.nros (no $HOME)"
    }
}
