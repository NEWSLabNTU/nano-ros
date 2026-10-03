//! Issue 1635 — a contracted C or C++ image publishes its contract violations
//! on `/diagnostics`.
//!
//! The executor already detects them (phase-462 W1 installs the tables, the
//! rules run every spin) and logs each at detection (issue 0514's floor). What
//! no generated entry did was drain the ring into a `DiagnosticArray`, so the
//! RFC-0050 rule vocabulary never reached the observer play_launch and
//! `ros2 topic echo /diagnostics` read on the Linux side. This module is the
//! reporter: one `/diagnostics` publisher per executor, armed by
//! `nros_cpp_install_monitors` and fed by the executor's violation sink from
//! `spin_once`, so every board's spin loop reaches it without a line in any
//! entry template.
//!
//! The mapping from a violation to a report is `nros-diagnostics`'
//! (`DiagnosticReporter::report_violation`), the same one the parity fixture
//! uses, so a violation reads the same from a fixture and from an image.

use core::ffi::c_void;

use nros::{RosMessage, Serialize};
use nros_diagnostics::{DiagnosticArray, DiagnosticReporter};
use nros_rmw::{Publisher as _, Session as _, TopicInfo};

use crate::{CppContext, NROS_CPP_RET_OK, nros_cpp_ret_t};

/// The topic ROS 2's diagnostics tooling reads.
pub(crate) const DIAG_TOPIC: &str = "/diagnostics";

/// One report's CDR: one `DiagnosticStatus` (name <= 64, message <= 128,
/// hardware id <= 96) with one key/value (<= 32 + 64) and the array's header.
/// 512 covers that with room; a report that does not fit is counted, not sent
/// truncated.
const REPORT_BUF: usize = 512;

/// The per-executor reporter: the publisher and two counters. No rate-limit
/// state: the rules are windowed and fire on transitions, so the reporter is
/// built per call with no interval (and this struct's size, which
/// `nros-build-helpers` adds to `CPP_EXECUTOR_OPAQUE_U64S`, stays the
/// publisher plus two words).
pub(crate) struct DiagSink {
    publisher: nros::internals::RmwPublisher,
    /// Reports published / reports that could not be (serialise or publish).
    pub(crate) published: u32,
    pub(crate) failed: u32,
}

/// Create this executor's `/diagnostics` publisher and point its violation
/// sink at it. Idempotent: a second install (another tier's table on the same
/// executor) keeps the first reporter.
pub(crate) fn arm(ctx: &mut CppContext) -> nros_cpp_ret_t {
    if ctx.diag.is_some() {
        return NROS_CPP_RET_OK;
    }
    let info = TopicInfo::new(
        DIAG_TOPIC,
        <DiagnosticArray as RosMessage>::TYPE_NAME,
        crate::normalize_type_hash(<DiagnosticArray as RosMessage>::TYPE_HASH),
    )
    .with_domain(ctx.domain_id);
    let publisher = match ctx
        .executor
        .session_mut()
        .create_publisher(&info, nros_rmw::QoSProfile::default())
    {
        Ok(p) => p,
        Err(e) => {
            crate::cpp_diag!(
                "contract monitors installed, but the {DIAG_TOPIC} publisher could not be \
                 created ({e:?}); violations stay in the log only (issue 1635)"
            );
            return crate::transport_error_to_cpp_ret(e);
        }
    };
    ctx.diag = Some(DiagSink {
        publisher,
        published: 0,
        failed: 0,
    });
    // The sink's context is the reporter's own address inside the context,
    // which is caller storage that does not move for the executor's life;
    // `nros_cpp_fini` unhooks it before dropping it.
    let sink = ctx.diag.as_mut().map(|d| d as *mut DiagSink as *mut c_void);
    unsafe {
        ctx.executor
            .set_violation_sink(sink.map(|p| (publish_violation as _, p)))
    };
    NROS_CPP_RET_OK
}

/// The executor's violation sink: one violation, one `DiagnosticArray`.
///
/// # Safety
/// `ctx` is the `DiagSink` [`arm`] installed.
unsafe fn publish_violation(ctx: *mut c_void, v: &nros::monitor::Violation) {
    let sink = unsafe { &mut *(ctx as *mut DiagSink) };
    // `now_us` 0 with a 0 interval: the rules themselves are windowed and
    // fire on transitions, so the reporter adds no second rate limit.
    let Some(report) =
        DiagnosticReporter::new(0).report_violation(0, v.rule, v.fqn, v.measured, v.declared)
    else {
        return;
    };
    let mut buf = [0u8; REPORT_BUF];
    let Ok(mut w) = nros::CdrWriter::new_with_header(&mut buf) else {
        sink.failed = sink.failed.saturating_add(1);
        return;
    };
    if report.serialize(&mut w).is_err() {
        sink.failed = sink.failed.saturating_add(1);
        return;
    }
    let len = w.position();
    match sink.publisher.publish_raw(&buf[..len]) {
        Ok(()) => sink.published = sink.published.saturating_add(1),
        Err(_) => sink.failed = sink.failed.saturating_add(1),
    }
}
