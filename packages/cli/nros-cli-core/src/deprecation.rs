//! RFC-0052 / phase-296 R3 — deprecation notices for the legacy (pre-model)
//! bake paths.
//!
//! The canonical config path is the play_launch-resolved SystemModel
//! (`play_launch resolve … -o system_model.yaml`, then `--model` /
//! `MODEL` / `nros::main!(model = …)`). The transitional paths that parse
//! launch XML + `system.toml` at build time are deprecated; they warn once
//! per process and are removed in R4.
//!
//! Set `NROS_ALLOW_LEGACY_BAKE=1` to silence the warning (for consumers not
//! yet migrated) — the flag is the single opt-out both this crate and the
//! proc-macro honor.

use std::sync::atomic::{AtomicBool, Ordering};

/// The env flag that silences every legacy-bake deprecation warning.
pub const ALLOW_LEGACY_ENV: &str = "NROS_ALLOW_LEGACY_BAKE";

static WARNED: AtomicBool = AtomicBool::new(false);

/// Is the legacy path explicitly acknowledged (warning suppressed)?
pub fn legacy_allowed() -> bool {
    std::env::var_os(ALLOW_LEGACY_ENV).is_some_and(|v| v != "0")
}

/// Emit the one-shot legacy-bake deprecation warning naming `what`, unless
/// suppressed by [`ALLOW_LEGACY_ENV`]. Warns at most once per process so a
/// multi-target bake does not spam.
pub fn warn_legacy_bake(what: &str) {
    if legacy_allowed() {
        return;
    }
    if WARNED.swap(true, Ordering::Relaxed) {
        return;
    }
    eprintln!(
        "warning[deprecated]: {what} is deprecated (phase-296 R3).\n  \
         The canonical path is the play_launch-resolved SystemModel — pass \
         `--model <system_model.yaml>` (CLI), `MODEL <…>` (nano_ros_add_executable), \
         or `nros::main!(model = \"<bringup>\")` (Rust).\n  \
         Resolve one with: `play_launch resolve <launch> --system <system.toml> \
         -o <bringup>/config/system_model.yaml`.\n  \
         The launch-XML / system.toml bake path is removed in phase-296 R4.\n  \
         Set `{ALLOW_LEGACY_ENV}=1` to silence this warning meanwhile."
    );
}
