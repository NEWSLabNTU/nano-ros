//! The FreeRTOS task-scheduling defaults, in one place.
//!
//! phase-337 W5.d — `nros_board_freertos::Config::default()`, the MPS2 board's
//! `build.rs::emit_nros_app_config` and `cmake/templates/freertos_app_config.c.in`
//! each carried their own copy of these eight numbers, and they had **already
//! drifted**: `app_stack_bytes` read 393216 in Rust, 262144 in the board's
//! `build.rs` C-string mirror and 524288 in the CMake template. That is exactly
//! the silent-drift class this phase exists to remove, so the numbers live here
//! and the emitters read them.
//!
//! This module is `no_std` and dependency-free so the runtime `Config` and the
//! `build.rs` emitter can share it — the emitter itself is in
//! [`crate::freertos_build`], behind `build-helpers`.

/// Priorities are on a normalized 0–31 scale (higher = more important); the
/// board maps them onto FreeRTOS's `0..configMAX_PRIORITIES` range.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FreertosScheduling {
    /// Application task priority (normalized 0–31).
    pub app_priority: u8,
    /// Application task stack size in bytes.
    pub app_stack_bytes: u32,
    /// zenoh-pico read task priority (normalized 0–31).
    pub zenoh_read_priority: u8,
    /// zenoh-pico read task stack size in bytes.
    pub zenoh_read_stack_bytes: u32,
    /// zenoh-pico lease task priority (normalized 0–31).
    pub zenoh_lease_priority: u8,
    /// zenoh-pico lease task stack size in bytes.
    pub zenoh_lease_stack_bytes: u32,
    /// Network poll task priority (normalized 0–31).
    pub poll_priority: u8,
    /// Network poll interval in milliseconds.
    pub poll_interval_ms: u32,
}

/// The default application task stack, in bytes, when the build sets no
/// override. See [`app_stack_bytes`] for why it is this large.
pub const DEFAULT_APP_STACK_BYTES: u32 = 393216;

impl Default for FreertosScheduling {
    fn default() -> Self {
        Self {
            // The old hardcoded constants, normalized: APP_TASK_PRIORITY=3 →
            // 12 (12*7/31 ≈ 2.7 → 3); POLL_TASK_PRIORITY=4 → 16 (16*7/31 ≈ 3.6
            // → 4). zenoh read/lease default to 4 in zenoh-pico
            // (configMAX_PRIORITIES/2). CLAUDE.md's FreeRTOS pitfall entry
            // requires the poll task to land at >= 4.
            app_priority: 12,
            app_stack_bytes: DEFAULT_APP_STACK_BYTES,
            zenoh_read_priority: 16,
            zenoh_read_stack_bytes: 5120,
            zenoh_lease_priority: 16,
            zenoh_lease_stack_bytes: 5120,
            poll_priority: 16,
            poll_interval_ms: 5,
        }
    }
}

/// Resolve the app-task stack size from the `NROS_FREERTOS_APP_STACK_KB` build
/// override, in the one spelling both readers share.
///
/// The Rust `Config` passes `option_env!("NROS_FREERTOS_APP_STACK_KB")`, a
/// `build.rs` passes `env::var(..).ok().as_deref()`. Both land here so the
/// parser cannot drift.
///
/// The default is 384 KiB: the Rust zenoh executor can exceed 160 KiB opening a
/// FreeRTOS session with lwIP up, and the phase-212 Entry / run-plan Executor
/// open overflows the older 256 KiB (issue #46). The stack is drawn from the
/// FreeRTOS heap (heap_4), sized to match in `nros-board-freertos/build.rs`.
/// A 10-node macro entry's register pass overflows even 384 KiB, hence the
/// override (issue-0274 follow-up).
///
/// # Panics
/// On a non-decimal value — a typo'd stack size must not silently become the
/// default and stack-overflow at runtime.
pub const fn app_stack_bytes(kb: Option<&str>) -> u32 {
    match kb {
        Some(s) => parse_kb(s) * 1024,
        None => DEFAULT_APP_STACK_BYTES,
    }
}

/// Const decimal parser for the `NROS_FREERTOS_APP_STACK_KB` build env.
const fn parse_kb(s: &str) -> u32 {
    let bytes = s.as_bytes();
    let mut i = 0;
    let mut acc: u32 = 0;
    while i < bytes.len() {
        let d = bytes[i];
        if !d.is_ascii_digit() {
            panic!("NROS_FREERTOS_APP_STACK_KB must be a decimal integer");
        }
        acc = acc * 10 + (d - b'0') as u32;
        i += 1;
    }
    acc
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_stack_override_is_read_in_kib() {
        assert_eq!(app_stack_bytes(Some("256")), 262144);
        assert_eq!(app_stack_bytes(None), DEFAULT_APP_STACK_BYTES);
    }

    #[test]
    fn the_poll_task_outranks_the_app_task() {
        // CLAUDE.md's FreeRTOS pitfall: a poll task that does not outrank the
        // app task starves, and lwIP never drains the LAN9118 RX FIFO.
        let sched = FreertosScheduling::default();
        assert!(sched.poll_priority > sched.app_priority);
    }
}
