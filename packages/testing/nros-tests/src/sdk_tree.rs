//! Where a vendored SDK tree is, for a test that needs one (issue 1833).
//!
//! Phase-484 W3c stopped exporting `FREERTOS_DIR`, `LWIP_DIR`, `THREADX_DIR`,
//! `NETX_DIR` and `NUTTX_DIR`; the build answers them through
//! `nros_build_paths::locate`, whose ladder still honours each variable first.
//! The test preconditions kept reading the variables, so from a fresh shell
//! every RTOS test reported "not set" while the store held every tree. This is
//! the same ladder the build asks, so a test looks where its fixture was
//! built from.

use std::path::PathBuf;

/// The tree `[source.<source>]` in `nros-sdk-index.toml` names, if one is
/// provisioned (environment, then this checkout, then the store).
#[must_use]
pub fn sdk_tree(source: &str) -> Option<PathBuf> {
    nros_build_paths::locate::try_source(source)
        .ok()
        .filter(|p| p.is_dir())
}
