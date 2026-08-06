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
/// The message body is emitted VERBATIM — the examples bake the full human line
/// into it (`Publishing: '...'` / `I heard: [...]`) and the e2e harness greps
/// those markers with `contains`, which the `[LEVEL]` prefix leaves intact.
///
/// That prefix is not decoration: `nros_log`'s sinks write one, and issue 0309
/// made `workspace_features_e2e` assert the tag on the same line as the marker
/// precisely so a bare `printf` cannot satisfy a logging proof. A bridge that
/// dropped it would make every `log::` record look like that bypass. Same
/// labels as `nros-board-linux` — `log::Level`'s `Display` is already
/// `TRACE`/`DEBUG`/`INFO`/`WARN`/`ERROR`.
pub(crate) fn install_semihosting_log_bridge() {
    struct SemihostingLogger;

    impl ::log::Log for SemihostingLogger {
        fn enabled(&self, _: &::log::Metadata<'_>) -> bool {
            true
        }

        fn log(&self, record: &::log::Record<'_>) {
            crate::println!("[{}] {}", record.level(), record.args());
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
