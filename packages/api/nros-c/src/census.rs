//! Issue 1556 item 2 — the census switch for a C application that owns its own
//! `main` and its own spin loop (`nros_app_main` + `rclc_executor_spin*`).
//!
//! The C++ funnel (`nros_board_native_run_components_named_ns`) and the Rust
//! one (`nros-board-linux`'s `boot_hosted`) answer `$NROS_CENSUS_OUT` where the
//! BOARD owns the boot. An rclc-style C application has no such funnel: it
//! opens its own support context, creates its own node and entities, and
//! calls its own spin. So the switch rides the two calls every such program
//! makes, in the order it makes them:
//!
//! 1. [`arm`] — at support init, BEFORE the session resolves its backend:
//!    register the recording backend and select it by name (`$NROS_RMW`), the
//!    same two halves `nros-cpp`'s `census_select_backend` performs. A census
//!    against a transporting backend would need a router and record nothing.
//! 2. [`finish_if_armed`] — at the head of every spin entry point: the program
//!    has created everything it is going to create (an rclc application
//!    declares, then spins), so write what the recorder saw — through
//!    `nros::metadata_mode::to_json`, the ONE emitter — and exit instead of
//!    spinning.
//!
//! The entities themselves reach the recorder through `nros::census_hooks`
//! (node / timer / guard condition / parameter, called by this crate's entry
//! points) and through the recording backend (publishers, subscriptions,
//! services, clients).
//!
//! Hosted only: `$NROS_CENSUS_OUT` is an environment variable and the census a
//! file, so the bodies need `std` — which no RTOS image has, and there both
//! calls are empty. An image with `std` but without `metadata-mode` has no
//! recorder; it REFUSES a census at support init rather than booting normally
//! with no file.

use crate::error::nros_ret_t;

/// Arm census mode at support init. `Ok(())` means "carry on": either no
/// census was asked for, or the recorder is now the selected backend.
pub(crate) fn arm() -> Result<(), nros_ret_t> {
    #[cfg(feature = "std")]
    {
        hosted::arm()
    }
    #[cfg(not(feature = "std"))]
    {
        Ok(())
    }
}

/// At the head of every spin entry point: if [`arm`] armed a census, write it
/// and exit the process instead of spinning. Returns only when no census was
/// armed (always, on a build without `std`).
pub(crate) fn finish_if_armed() {
    #[cfg(feature = "std")]
    hosted::finish_if_armed();
}

#[cfg(feature = "std")]
mod hosted {
    // ONE `std::` path for the module: `check-std-census` counts the text, and
    // every use here is the capability (an environment variable, a file, an
    // exit) rather than a convenience over the platform layer.
    // `fs` is used by the `metadata-mode` arm only.
    #[allow(unused_imports)]
    use std::{env, fs, process};

    #[allow(unused_imports)]
    use alloc::string::{String, ToString};
    use core::sync::atomic::{AtomicBool, Ordering};

    use crate::error::nros_ret_t;

    /// The census switch's variable name — the one the C++ and Rust funnels
    /// read.
    const CENSUS_OUT_ENV: &str = "NROS_CENSUS_OUT";

    /// Set by [`arm`]; read by [`finish_if_armed`].
    static ARMED: AtomicBool = AtomicBool::new(false);

    /// `$NROS_CENSUS_OUT`, with an empty value read as unset (the reading
    /// every other funnel gives it).
    fn census_out_path() -> Option<String> {
        let raw = env::var(CENSUS_OUT_ENV).ok()?;
        if raw.is_empty() { None } else { Some(raw) }
    }

    #[cfg(feature = "metadata-mode")]
    pub(super) fn arm() -> Result<(), nros_ret_t> {
        if census_out_path().is_none() {
            return Ok(());
        }
        // NOT `let _ =` (issue 1419): a full registry is a census that dies
        // one call later naming the selector rather than the cause.
        let ret = nros_rmw_metadata::nros_rmw_metadata_register();
        if ret != 0 {
            nros_log::log_error!(
                nros_log::get_logger("nros-c"),
                "nros census: registering the recording backend failed (rc={}); the RMW \
                 registry holds {} slot(s)",
                ret,
                nros_rmw_metadata::REGISTRY_SLOTS
            );
        }
        // SAFETY: support init runs before the application creates any
        // executor or thread of ours; a census process is single-threaded at
        // this point, which is the condition `set_var` asks for (the C++
        // funnel's argument).
        unsafe { env::set_var("NROS_RMW", nros::census_hooks::RECORDER_RMW) };
        ARMED.store(true, Ordering::Release);
        Ok(())
    }

    /// No recorder in this image: refuse the census by name at support init,
    /// so the caller's `nros_support_init` fails and the run exits non-zero
    /// with no file — never a normal boot that dials a router.
    #[cfg(not(feature = "metadata-mode"))]
    pub(super) fn arm() -> Result<(), nros_ret_t> {
        let Some(path) = census_out_path() else {
            return Ok(());
        };
        nros_log::log_error!(
            nros_log::get_logger("nros-c"),
            "nros census: $NROS_CENSUS_OUT=`{}` but this C image was built without \
             `nros-c/metadata-mode` -- there is no recorder to dump",
            path
        );
        Err(crate::error::NROS_RET_ERROR)
    }

    pub(super) fn finish_if_armed() {
        if !ARMED.load(Ordering::Acquire) {
            return;
        }
        let code = write_census();
        process::exit(code);
    }

    #[cfg(feature = "metadata-mode")]
    fn write_census() -> i32 {
        let Some(path) = census_out_path() else {
            return 1;
        };
        if nros::metadata_mode::entity_count() == 0 {
            nros_log::log_error!(
                nros_log::get_logger("nros-c"),
                "nros census: the application created no entity before its first spin -- \
                 refusing to write a census a check would read as \"declares nothing\""
            );
            return 1;
        }
        let exe = env::current_exe()
            .ok()
            .and_then(|p| p.file_name().map(|s| s.to_string_lossy().to_string()))
            .unwrap_or_default();
        let export = nros::node_metadata::SourceMetadataExport::new(&exe, &exe)
            .executable(&exe)
            .language("c");
        let Ok(json) = nros::metadata_mode::to_json(&export) else {
            return 1;
        };
        match fs::write(&path, json) {
            Ok(()) => 0,
            Err(_) => {
                nros_log::log_error!(
                    nros_log::get_logger("nros-c"),
                    "nros census: cannot write `{}`",
                    path
                );
                1
            }
        }
    }

    /// Unreachable in practice: [`arm`] never arms without the recorder.
    #[cfg(not(feature = "metadata-mode"))]
    fn write_census() -> i32 {
        1
    }
}
