//! Contract monitors on target, and the `/diagnostics` reporter they publish
//! through — issues 1635 and 1676, ONE implementation for every language.
//!
//! RFC-0052 / phase-296 W3b: a contracted image bakes monitor tables
//! (`min_rate_hz` / `max_latency_ms` per publisher, `max_age_ms` per
//! subscriber), the executor checks them every spin and logs each violation at
//! detection (issue 0514's floor). Issue 1635 made a C or C++ image also
//! PUBLISH each one as a `DiagnosticArray` on `/diagnostics`, through a
//! reporter `nros-cpp` armed when the tables were installed. The Rust entry
//! (`nros::main!`) installed no table at all (issue 1676), so it neither
//! monitored nor reported.
//!
//! This module is that reporter, moved up to the umbrella so the three
//! languages share it: [`DiagSink`] (one `/diagnostics` publisher and its two
//! counters) and its violation sink, used by `nros-cpp`'s
//! `nros_cpp_install_monitors` and by [`install_contract_monitors`], which the
//! `nros::main!` expansion calls before the first node is created. The mapping
//! from a violation to a report is `nros-diagnostics`'
//! (`DiagnosticReporter::report_violation`, streamed by
//! `write_violation_report` so the report never sits on the spin thread's
//! stack as a value), so a violation reads the same from every road.

use core::{cell::UnsafeCell, ffi::c_void};

use nros_core::RosMessage;
use nros_diagnostics::DiagnosticArray;
use nros_node::executor::{
    Executor,
    monitor::{AgeMonitorSpec, MonitorSpec, Violation},
};
use nros_rmw::{Publisher as _, Session as _, TopicInfo, TransportError};

use crate::internals::RmwPublisher;

/// The topic ROS 2's diagnostics tooling reads.
pub const DIAG_TOPIC: &str = "/diagnostics";

/// One report's CDR: one `DiagnosticStatus` (name <= 64, message <= 128,
/// hardware id <= 96) with one key/value (<= 32 + 64) and the array's header.
/// 512 covers that with room; a report that does not fit is counted, not sent
/// truncated.
const REPORT_BUF: usize = 512;

/// One executor's `/diagnostics` reporter: the publisher and two counters.
///
/// No rate-limit state: the rules are windowed and fire on transitions, so the
/// reporter is built per call with no interval (and this struct's size, which
/// `nros-build-helpers` adds to `CPP_EXECUTOR_OPAQUE_U64S`, stays the
/// publisher plus two words).
pub struct DiagSink {
    publisher: RmwPublisher,
    /// Reports published.
    pub published: u32,
    /// Reports that could not be (serialise or publish failed).
    pub failed: u32,
}

impl DiagSink {
    /// Create a `/diagnostics` publisher on `executor`'s session, in
    /// `domain_id` (the executor's own for a Rust entry; the C++ context keeps
    /// its domain beside the executor).
    pub fn create(executor: &mut Executor<'_>, domain_id: u32) -> Result<Self, TransportError> {
        let info = TopicInfo::new(
            DIAG_TOPIC,
            <DiagnosticArray as RosMessage>::TYPE_NAME,
            <DiagnosticArray as RosMessage>::TYPE_HASH,
        )
        .with_domain(domain_id);
        let publisher = executor
            .session_mut()
            .create_publisher(&info, nros_rmw::QoSProfile::default())?;
        Ok(Self {
            publisher,
            published: 0,
            failed: 0,
        })
    }

    /// Point `executor`'s violation sink at this reporter.
    ///
    /// # Safety
    /// `self` must not move, and must outlive every spin of `executor`, until
    /// the sink is removed (`set_violation_sink(None)`) or the executor is
    /// dropped — the executor keeps its address.
    pub unsafe fn hook(&mut self, executor: &mut Executor<'_>) {
        let ctx = self as *mut DiagSink as *mut c_void;
        unsafe { executor.set_violation_sink(Some((publish_violation as _, ctx))) };
    }
}

/// The executor's violation sink: one violation, one `DiagnosticArray`.
///
/// # Safety
/// `ctx` is the [`DiagSink`] [`DiagSink::hook`] installed.
unsafe fn publish_violation(ctx: *mut c_void, v: &Violation) {
    let sink = unsafe { &mut *(ctx as *mut DiagSink) };
    // No rate limit: the rules themselves are windowed and fire on
    // transitions. phase-474 I7 -- the report is STREAMED into the buffer
    // (`write_violation_report`, byte-identical to serializing
    // `DiagnosticReporter::report_violation`'s value). The value is a 5 KB
    // `DiagnosticArray`, and building it here, on the spin thread, ran the
    // safety island's 16 KiB main stack into the idle thread's on its first
    // stored violation. What stays on this frame is the 512 B buffer.
    let mut buf = [0u8; REPORT_BUF];
    let Ok(mut w) = nros_core::CdrWriter::new_with_header(&mut buf) else {
        sink.failed = sink.failed.saturating_add(1);
        return;
    };
    if nros_diagnostics::write_violation_report(&mut w, v.rule, v.fqn, v.measured, v.declared)
        .is_err()
    {
        sink.failed = sink.failed.saturating_add(1);
        return;
    }
    let len = w.position();
    match sink.publisher.publish_raw(&buf[..len]) {
        Ok(()) => sink.published = sink.published.saturating_add(1),
        Err(_) => sink.failed = sink.failed.saturating_add(1),
    }
}

/// Static storage for one executor's [`DiagSink`] — what a Rust entry owns in
/// place of the C++ context's `diag` field.
///
/// `nros::main!` emits one per executor that installs a non-empty table, as a
/// `static`. Filled once, by [`install_contract_monitors`], on the thread that
/// registers that executor's nodes; read afterwards only through the
/// executor's sink, on the thread that spins it.
pub struct ContractReporter {
    sink: UnsafeCell<Option<DiagSink>>,
}

// SAFETY: one `ContractReporter` serves one executor. It is written once,
// before that executor spins, and afterwards reached only from that
// executor's spin (through the sink pointer) — never from two threads at once.
unsafe impl Sync for ContractReporter {}

impl ContractReporter {
    /// An empty reporter.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            sink: UnsafeCell::new(None),
        }
    }
}

impl Default for ContractReporter {
    fn default() -> Self {
        Self::new()
    }
}

/// Install a contracted Rust image's monitor tables on one executor, and arm
/// its `/diagnostics` reporter — issue 1676.
///
/// The Rust spelling of `nros_cpp_install_monitors`. Call BEFORE any node is
/// created on the executor: a publisher attaches its counter cell by exact
/// topic match at create time, so a table installed later monitors nothing.
/// `nros::main!` emits this ahead of the parameter services and the node
/// registrations on every executor it builds (the single one, and each
/// tier's).
///
/// The tables are refused, not truncated, when they exceed the executor's
/// `MAX_MONITORS` / `MAX_AGE_MONITORS` (the baked tables also carry a
/// compile-time assert naming the knob). Empty tables install nothing and arm
/// no reporter — an uncontracted image carries none of this. A reporter is
/// armed once per `reporter`; a second install keeps the first.
///
/// If the `/diagnostics` publisher cannot be created the tables stay
/// installed and violations reach the log only — reported, not fatal, which
/// is what `nros_cpp_install_monitors` does too (both through
/// [`arm_reporter`]; a census run arms none). Returns `Err` only for a
/// refused table.
///
/// # Safety
/// `executor` must be the live `*mut Executor<'static>` handle a
/// `RuntimeCtx` hands out (`runtime.executor_handle()`), or NULL, and nothing
/// else may hold a reference into that executor for the call.
pub unsafe fn install_contract_monitors(
    executor: *mut c_void,
    monitors: &'static [MonitorSpec],
    ages: &'static [AgeMonitorSpec],
    reporter: &'static ContractReporter,
) -> Result<(), &'static str> {
    if executor.is_null() {
        return Err("the runtime hands out no executor to install the contract monitors on");
    }
    let executor = unsafe { &mut *(executor as *mut Executor<'static>) };
    if let Err(full) = executor.try_set_monitor_tables(monitors, ages) {
        nros_log::log_error!(
            nros_log::get_logger("nros.contract"),
            "install_contract_monitors: {}",
            full
        );
        return Err("the contract's monitor tables exceed the executor's monitor capacity");
    }
    if monitors.is_empty() && ages.is_empty() {
        return Ok(());
    }
    // SAFETY: see `ContractReporter`'s `Sync` — this is the one write, made
    // before the executor spins.
    let slot = unsafe { &mut *reporter.sink.get() };
    let domain_id = executor.domain_id();
    // SAFETY: `slot` lives in a `'static` reporter and never moves.
    unsafe { arm_reporter(executor, domain_id, slot) };
    Ok(())
}

/// Arm one executor's `/diagnostics` reporter in `slot`, and point the
/// executor's violation sink at it — THE arming step, shared by
/// [`install_contract_monitors`] (Rust) and `nros-cpp`'s
/// `nros_cpp_install_monitors` (C and C++), issue 1693.
///
/// * Idempotent: a slot already holding a reporter keeps it.
/// * A CENSUS run arms nothing ([`crate::census_hooks::recording_run`]): it
///   never spins, so the reporter could never publish, and the recorder has no
///   node open yet to attribute it to — it refused the publisher, and the
///   C/C++ road turned that refusal into a failed setup that recorded nothing.
/// * A publisher that cannot be created is REPORTED, never fatal: the tables
///   stay installed and every violation still reaches the log (issue 0514's
///   floor). Both roads say so in the one message below; before this the C/C++
///   road logged "violations stay in the log only" and then failed the setup.
///
/// Returns whether a reporter is armed after the call.
///
/// # Safety
/// `slot` must not move, and must outlive every spin of `executor`, until the
/// sink is removed (`set_violation_sink(None)`) or the executor is dropped —
/// [`DiagSink::hook`]'s contract.
pub unsafe fn arm_reporter(
    executor: &mut Executor<'_>,
    domain_id: u32,
    slot: &mut Option<DiagSink>,
) -> bool {
    if slot.is_some() {
        return true;
    }
    if crate::census_hooks::recording_run() {
        return false;
    }
    match DiagSink::create(executor, domain_id) {
        Ok(sink) => {
            let sink = slot.insert(sink);
            // SAFETY: forwarded from this fn's contract.
            unsafe { sink.hook(executor) };
            true
        }
        Err(e) => {
            nros_log::log_error!(
                nros_log::get_logger("nros.contract"),
                "contract monitors installed, but the {} publisher could not be created \
                 ({:?}); violations stay in the log only (issues 1635/1676)",
                DIAG_TOPIC,
                e
            );
            false
        }
    }
}
