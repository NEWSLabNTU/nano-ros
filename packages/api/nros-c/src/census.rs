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
//! file, so the bodies need a C library — read and written through
//! `nros::env`, which exists on every Linux target (phase-359 W10 stage C; it
//! used to be `std`). No RTOS image has one, and there both calls are empty. A
//! Linux image without `metadata-mode` has no recorder; it REFUSES a census at
//! support init rather than booting normally with no file.

use crate::error::rcl_ret_t;

/// Arm census mode at support init. `Ok(())` means "carry on": either no
/// census was asked for, or the recorder is now the selected backend.
pub(crate) fn arm() -> Result<(), rcl_ret_t> {
    #[cfg(target_os = "linux")]
    {
        hosted::arm()
    }
    #[cfg(not(target_os = "linux"))]
    {
        Ok(())
    }
}

/// At the head of every spin entry point: if [`arm`] armed a census, write it
/// and exit the process instead of spinning. Returns only when no census was
/// armed (always, off Linux).
pub(crate) fn finish_if_armed() {
    #[cfg(target_os = "linux")]
    hosted::finish_if_armed();
}

#[cfg(target_os = "linux")]
mod hosted {
    // Every process facility here — the variable, the file, the exit — goes
    // through `nros::env`, the one C-library edge (phase-359 W10 stage C).
    use nros::env;

    use core::sync::atomic::{AtomicBool, Ordering};

    use crate::error::rcl_ret_t;

    /// The census switch's variable name — the one the C++ and Rust funnels
    /// read.
    const CENSUS_OUT_ENV: &str = "NROS_CENSUS_OUT";

    /// Set by [`arm`]; read by [`finish_if_armed`].
    static ARMED: AtomicBool = AtomicBool::new(false);

    /// `$NROS_CENSUS_OUT`, with an empty value read as unset (the reading
    /// every other funnel gives it).
    fn census_out_path() -> Option<&'static str> {
        env::var(CENSUS_OUT_ENV).filter(|raw| !raw.is_empty())
    }

    #[cfg(feature = "metadata-mode")]
    pub(super) fn arm() -> Result<(), rcl_ret_t> {
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
    /// so the caller's `rclc_support_init` fails and the run exits non-zero
    /// with no file — never a normal boot that dials a router.
    #[cfg(not(feature = "metadata-mode"))]
    pub(super) fn arm() -> Result<(), rcl_ret_t> {
        let Some(path) = census_out_path() else {
            return Ok(());
        };
        nros_log::log_error!(
            nros_log::get_logger("nros-c"),
            "nros census: $NROS_CENSUS_OUT=`{}` but this C image was built without \
             `nros-c/metadata-mode` -- there is no recorder to dump",
            path
        );
        Err(crate::error::RCL_RET_ERROR)
    }

    pub(super) fn finish_if_armed() {
        if !ARMED.load(Ordering::Acquire) {
            return;
        }
        let code = write_census();
        env::exit(code);
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
        let exe = env::exe_name().unwrap_or_default();
        let export = nros::node_metadata::SourceMetadataExport::new(&exe, &exe)
            .executable(&exe)
            .language("c");
        let Ok(json) = nros::metadata_mode::to_json(&export) else {
            return 1;
        };
        match env::write_file(path, json.as_bytes()) {
            Ok(()) => 0,
            Err(why) => {
                nros_log::log_error!(
                    nros_log::get_logger("nros-c"),
                    "nros census: `{}` {}",
                    path,
                    why
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
