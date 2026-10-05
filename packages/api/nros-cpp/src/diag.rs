//! Issue 1635 — a contracted C or C++ image publishes its contract violations
//! on `/diagnostics`.
//!
//! The executor already detects them (phase-462 W1 installs the tables, the
//! rules run every spin) and logs each at detection (issue 0514's floor). What
//! no generated entry did was drain the ring into a `DiagnosticArray`, so the
//! RFC-0050 rule vocabulary never reached the observer play_launch and
//! `ros2 topic echo /diagnostics` read on the Linux side. The reporter is one
//! `/diagnostics` publisher per executor, armed by `nros_cpp_install_monitors`
//! and fed by the executor's violation sink from `spin_once`, so every board's
//! spin loop reaches it without a line in any entry template.
//!
//! Issue 1676 — the reporter itself is `nros::contract::DiagSink`, shared with
//! the Rust entry (`nros::main!` -> `install_contract_monitors`). This module
//! only places it in the C++ context, which is caller storage that does not
//! move for the executor's life.

use crate::{CppContext, NROS_CPP_RET_OK, nros_cpp_ret_t};

/// The per-executor reporter, as the C++ context holds it.
pub(crate) use nros::contract::DiagSink;

/// Create this executor's `/diagnostics` publisher and point its violation
/// sink at it. Idempotent: a second install (another tier's table on the same
/// executor) keeps the first reporter.
pub(crate) fn arm(ctx: &mut CppContext) -> nros_cpp_ret_t {
    if ctx.diag.is_some() {
        return NROS_CPP_RET_OK;
    }
    let sink = match DiagSink::create(&mut ctx.executor, ctx.domain_id) {
        Ok(s) => s,
        Err(e) => {
            crate::cpp_diag!(
                "contract monitors installed, but the {} publisher could not be \
                 created ({e:?}); violations stay in the log only (issue 1635)",
                nros::contract::DIAG_TOPIC
            );
            return crate::transport_error_to_cpp_ret(e);
        }
    };
    let sink = ctx.diag.insert(sink);
    // The reporter lives inside the context, which is caller storage that does
    // not move for the executor's life; `nros_cpp_fini` unhooks it before
    // dropping it.
    unsafe { sink.hook(&mut ctx.executor) };
    NROS_CPP_RET_OK
}
