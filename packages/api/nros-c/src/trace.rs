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
