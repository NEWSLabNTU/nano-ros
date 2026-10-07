//! Phase 8 — the C entry point that installs the callback trace sink.
//!
//! Design: autoware-safety-island `docs/design/callback_tracing.rst`.
//!
//! `nros_node::executor::callback_trace::set_trace_sink` is a Rust `pub fn`,
//! so a C entry funnel cannot call it. The Zephyr entry path IS C
//! (`nros-board-zephyr`'s `c/zephyr_run_tiers.c` — the crate's Rust half is
//! not in every image's link graph, only that translation unit is), and it is
//! the one place that runs before the executor registers anything, which is
//! the deadline the registration events have to beat.
//!
//! It lives in `nros-c` rather than `nros-cpp` deliberately: `nros-c` is
//! linked in BOTH the C-API path and the C++-only path, so one export covers
//! both configurations. `wake_probe::set_cycle_reader` — the seam this
//! copies — has no C export at all (its only caller is a pure-Rust bin), so
//! there is no prior spelling to match here.
//!
//! ## Why the symbol is unconditional and the body is not
//!
//! The same idiom the Zephyr platform shims use
//! (cf. `nros_zephyr_epoch_acquire_configured`): an unconditional symbol with
//! a gated body. The C caller then needs no `#ifdef` and no Kconfig knob has
//! to reach the cargo lane — an image built without `trace-callbacks` still
//! links, and the call is simply a no-op.
//!
//! The `rmw-cffi` half of the gate is not optional: `callback_trace` lives
//! under `nros-node`'s `has_rmw` cfg (there is no executor to trace without an
//! RMW seam), and `rmw-cffi` is what puts it there.

/// Install the callback trace sink, or clear it with `NULL`.
///
/// `sink` is called as `sink(marker_id, arg)` — the signature of a
/// two-`uint32` platform trace event, chosen so a platform shim is
/// installable verbatim:
///
/// | `marker_id` | event    | `arg`                                     |
/// |-------------|----------|-------------------------------------------|
/// | 16          | register | `handle << 8 \| kind`                     |
/// | 17          | name     | next 4 name bytes, byte *i* in bits `8*i` |
/// | 18          | start    | `handle`                                  |
/// | 19          | end      | `handle`                                  |
///
/// and, with the later blocks, 20 (handle-tagged name chunk), 21-24 (a stored
/// contract violation, phase-474 I1) and 25-27 (a take and its source stamp,
/// phase-474 I3, see [`nros_trace_set_take`]); the full table is in
/// `nros_node::executor::callback_trace`.
///
/// Call it once at startup, BEFORE anything is registered on the executor: a
/// sink installed later misses the registration events, and the decoder then
/// has handles with no names.
///
/// No-op unless the crate was built with `trace-callbacks`.
///
/// # Safety
/// `sink` must be a valid function pointer with the C ABI above, or `NULL`.
/// It is called from the executor's dispatch path — including from an
/// OS-priority worker task, i.e. a different thread — so it must be
/// re-entrant and must not unwind. It must remain valid until it is replaced
/// or cleared.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_set_trace_sink(sink: Option<unsafe extern "C" fn(u32, u32)>) {
    #[cfg(all(feature = "trace-callbacks", feature = "rmw-cffi"))]
    nros_node::executor::callback_trace::set_trace_sink(sink);
    #[cfg(not(all(feature = "trace-callbacks", feature = "rmw-cffi")))]
    let _ = sink;
}

/// phase-474 I2 -- the application's start-up is over: arm the contract
/// monitors of every executor that waits for this
/// (`CONFIG_NROS_MONITOR_ARM_ON_CALL`). Call it where the application enters
/// its running state, for example an INIT/RUN state machine's RUN transition.
///
/// Safe from any thread and from inside a callback: it bumps one atomic, and
/// each executor arms at its next spin. Before arming, a monitor verdict is
/// counted (`suppressed_before_arm`) but not stored, logged or traced. On an
/// image whose monitors arm at the first spin (the default) it does nothing.
/// No-op in a build without an RMW, which has no monitors.
#[unsafe(no_mangle)]
pub extern "C" fn nros_monitors_arm() {
    #[cfg(feature = "rmw-cffi")]
    nros_node::executor::monitor::request_monitor_arming();
}

/// phase-474 I3 -- trace each sample TAKEN by the subscription in `handle`
/// (its slot index: the `handle` its register event carries), or stop.
///
/// A take is marker 25 (`handle << 24 | take seq`), emitted before the
/// callback's start (18). With `stamp_offset >= 0` it is followed by 26/27,
/// the sample's `stamp.sec` / `stamp.nanosec` read at that byte of the
/// serialized sample, encapsulation header included (4 for a type that
/// starts with a `Header` or a `builtin_interfaces/Time stamp`). A negative
/// `stamp_offset` emits no stamp on the C/C++ paths, which carry no type.
/// Overrides `CONFIG_NROS_TRACE_TAKES` for this slot.
///
/// No-op unless the crate was built with `trace-callbacks`.
#[unsafe(no_mangle)]
pub extern "C" fn nros_trace_set_take(handle: u8, on: bool, stamp_offset: i32) {
    #[cfg(all(feature = "trace-callbacks", feature = "rmw-cffi"))]
    nros_node::executor::callback_trace::set_take_trace(
        handle,
        on,
        u16::try_from(stamp_offset).ok(),
    );
    #[cfg(not(all(feature = "trace-callbacks", feature = "rmw-cffi")))]
    let _ = (handle, on, stamp_offset);
}

/// phase-474 I3 -- trace one tick in `every` of the timer in `handle` (its
/// slot index): 1 = every tick, 0 = none. Overrides
/// `CONFIG_NROS_TRACE_TIMER_EVERY` for this slot; the first tick after the
/// call is traced. A 30 Hz tick whose jitter is the measurement can stay
/// whole while the rest are thinned to fit a RAM trace window.
///
/// No-op unless the crate was built with `trace-callbacks`.
#[unsafe(no_mangle)]
pub extern "C" fn nros_trace_set_timer_every(handle: u8, every: u16) {
    #[cfg(all(feature = "trace-callbacks", feature = "rmw-cffi"))]
    nros_node::executor::callback_trace::set_timer_trace_every(handle, every);
    #[cfg(not(all(feature = "trace-callbacks", feature = "rmw-cffi")))]
    let _ = (handle, every);
}
