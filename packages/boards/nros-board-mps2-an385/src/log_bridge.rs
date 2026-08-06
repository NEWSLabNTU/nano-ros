//! phase-338 — the `log` facade sink, reachable from BOTH boot paths.
//!
//! This module exists because the bridge used to live in `entry.rs`, which sits
//! behind `feature = "board-entry"`. The RTIC boot path (`rtic.rs`) could not
//! reach it, so an RTIC node body written against `log::info!` would have
//! COMPILED AND PRINTED NOTHING — the silent failure phase-338 W7.b names.
//! Ungated and shared, so both `entry.rs` and `rtic.rs` install the same sink.

/// phase-338 W7 — route the `log` facade to the semihosting console.
///
/// Mirrors `nros-board-linux::install_stdout_log_bridge` and
/// `nros-board-nuttx::install_stdout_logger`. `set_logger` may only succeed
/// once, and the `is_ok()` guard makes this a NO-OP if something already
/// installed one — so it adds a sink where there was none and never fights one
/// that exists.
///
/// The message is emitted VERBATIM, with no level prefix: the examples bake the
/// full human line into the message (`Publishing: '...'` / `I heard: [...]`)
/// and the e2e harness greps those markers, so decorating them here would break
/// every assertion at once.
pub(crate) fn install_semihosting_log_bridge() {
    struct SemihostingLogger;

    impl ::log::Log for SemihostingLogger {
        fn enabled(&self, _: &::log::Metadata<'_>) -> bool {
            true
        }

        fn log(&self, record: &::log::Record<'_>) {
            crate::println!("{}", record.args());
        }

        fn flush(&self) {}
    }

    static LOGGER: SemihostingLogger = SemihostingLogger;

    // No `std::sync::Once` on bare metal; `set_logger` is itself idempotent —
    // the second call returns Err and we leave the level alone.
    if ::log::set_logger(&LOGGER).is_ok() {
        ::log::set_max_level(::log::LevelFilter::Trace);
    }
}
