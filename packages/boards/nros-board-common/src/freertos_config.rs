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

/// FreeRTOS's `configMAX_PRIORITIES` in the shared `FreeRTOSConfig.h`. Usable
/// task priorities are `0..=FREERTOS_MAX_PRIORITY - 1`; `xTaskCreate` asserts
/// on anything higher.
pub const FREERTOS_MAX_PRIORITY: u32 = 8;

/// THE normalized-0–31 → raw-FreeRTOS priority conversion (issue 0623).
///
/// This is the one place the mapping exists. It used to exist in two, and they
/// did not agree — which is the same silent-drift class as the numbers above,
/// one level up in the abstraction:
///
/// | path | normalized 16 became |
/// | --- | --- |
/// | `Config::to_freertos_priority` (Rust entry) | **4** — proportional |
/// | `clamp_prio` (C entry, `freertos_c_entry.c`) | **7** — saturating |
///
/// So one config produced two different schedules depending on which entry the
/// image used. Worse on the C side: every default was ≥ 8 (`app_priority` 12,
/// zenoh read/lease and poll 16), so all four SATURATED to 7 and the intended
/// ordering — app below transport — collapsed into "everything equal".
///
/// The conversion is proportional rather than saturating because the scale is a
/// *band*, not a range to be clipped: `31` means "most urgent available" on
/// whatever port, and clipping maps most of the band onto one value.
pub const fn to_freertos_priority(normalized: u8) -> u32 {
    let n = if normalized > 31 { 31 } else { normalized };
    // Round-to-nearest over the 0-31 → 0-7 span: (n * 7 / 31), doubled and
    // offset so integer division rounds instead of truncating.
    (n as u32 * (FREERTOS_MAX_PRIORITY - 1) * 2 + 31) / 62
}

/// Priorities are **RAW FreeRTOS** — `0..configMAX_PRIORITIES-1`, higher = more
/// urgent — the same units a `[tiers.<name>.freertos] priority` is written in
/// (issue 0623).
///
/// They were on a normalized 0–31 scale, and that was the defect: a tier and a
/// transport task both end up at `xTaskCreate` in ONE priority space, so an
/// author comparing `priority = 5` against a transport band that read "16"
/// concluded the tier was below it when it was above. Two numbers that are
/// compared must be in one unit.
///
/// The normalized band still exists as an INPUT spelling — see
/// [`to_freertos_priority`] and the `[node.rt]` arm of the board's TOML parser
/// — so configs written against the old scale keep working. What no longer
/// exists is a normalized value living in this struct, where the thing reading
/// it cannot tell which scale it is on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FreertosScheduling {
    /// Application task priority (RAW FreeRTOS).
    pub app_priority: u8,
    /// Application task stack size in bytes.
    pub app_stack_bytes: u32,
    /// zenoh-pico read task priority (RAW FreeRTOS).
    pub zenoh_read_priority: u8,
    /// zenoh-pico read task stack size in bytes.
    pub zenoh_read_stack_bytes: u32,
    /// zenoh-pico lease task priority (RAW FreeRTOS).
    pub zenoh_lease_priority: u8,
    /// zenoh-pico lease task stack size in bytes.
    pub zenoh_lease_stack_bytes: u32,
    /// Network poll task priority (RAW FreeRTOS).
    pub poll_priority: u8,
    /// Network poll interval in milliseconds.
    pub poll_interval_ms: u32,
}

/// The default application task stack, in bytes, when the build sets no
/// override. See [`app_stack_bytes`] for the measurement it comes from.
pub const DEFAULT_APP_STACK_BYTES: u32 = 131072;

impl Default for FreertosScheduling {
    fn default() -> Self {
        Self {
            // Issue 0623 — RAW FreeRTOS now, and these are EXACTLY what the
            // normalized values resolved to, so the schedule is unchanged:
            // `to_freertos_priority(12) == 3`, `(16) == 4`. The historical
            // chain was APP_TASK_PRIORITY=3 -> normalized 12 -> back to 3, and
            // POLL_TASK_PRIORITY=4 -> 16 -> back to 4; the round trip is gone
            // and the constants are the ones that reach `xTaskCreate`.
            //
            // CLAUDE.md's FreeRTOS pitfall entry requires the poll task at
            // >= 4, which is now literally what is written here rather than
            // something a reader has to compute.
            app_priority: 3,
            app_stack_bytes: DEFAULT_APP_STACK_BYTES,
            zenoh_read_priority: 4,
            zenoh_read_stack_bytes: 5120,
            zenoh_lease_priority: 4,
            zenoh_lease_stack_bytes: 5120,
            poll_priority: 4,
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
/// The default is **128 KiB, and it is MEASURED** (issue 1146). Every FreeRTOS
/// task stack here is drawn from the FreeRTOS heap (heap_4, `ucHeap[]` in
/// `.bss`, sized in `nros-board-freertos/build.rs`), and a spawned tier whose
/// `stack_bytes` is 0 gets this number too — so it is charged once per task,
/// not once per image.
///
/// What the measurement is: `uxTaskGetStackHighWaterMark` on the app task at
/// the end of the register pass, which is the deepest point bring-up reaches.
/// Eight images on qemu mps2-an385, each run to a live zenoh session:
///
/// | image | after boot bringup | after `Executor::open` | after register |
/// | --- | --- | --- | --- |
/// | rust/talker | 5 040 | 8 952 | 25 160 |
/// | rust/listener | 5 040 | 8 952 | 24 784 |
/// | rust/service-server | 5 040 | 8 952 | 25 936 |
/// | rust/service-client | 5 040 | 8 952 | 26 992 |
/// | rust/action-server | 5 040 | 8 952 | **36 152** |
/// | rust/action-client | 5 040 | 8 952 | 33 584 |
/// | workspaces/rust (2 nodes) | 3 208 | 10 752 | 23 296 |
/// | workspaces/realtime-rust (2 tiers) | 3 208 | — | 22 184 boot / 22 368 tier |
///
/// So the worst in-tree peak is **36 152 bytes** and this default is 3.6x it.
/// Reproduce it from any image's own boot output: `report_stack_peak` in
/// `nros-board-freertos/src/entry.rs` prints the line, so nobody has to patch
/// the board to learn the number again. The table above was taken with a
/// throwaway probe; the shipped line reads 8 bytes HIGHER (36 160 for
/// action-server) because the reporting call has a frame of its own — that
/// delta is the difference between the two, not drift.
///
/// The three claims this replaces were each false, and each had outlived its
/// subject by phases:
///
/// - *"the Rust zenoh executor can exceed 160 KiB opening a FreeRTOS session
///   with lwIP up"* — measured **8 952** bytes at exactly that point, on every
///   one of the eight images. Off by 18x.
/// - *"the phase-212 Entry / run-plan Executor open overflows the older
///   256 KiB (issue #46)"* — whatever issue #46 saw in phase 212, nothing on
///   this path now comes within 7x of 256 KiB.
/// - *"a 10-node macro entry's register pass overflows even 384 KiB, hence the
///   override"* — true, and about an OUT-OF-TREE consumer (the sentinel entry
///   of `a60b80da3`, which ships 896 KiB). It argued for the override, never
///   for the default: an entry that overflows 384 KiB is not served by a
///   384 KiB default either. Nothing in this tree sets the override.
///
/// A bigger entry raises it — `NROS_FREERTOS_APP_STACK_KB`, or a `[node.rt]
/// app_stack_bytes`. Getting it wrong is loud but MISATTRIBUTED, which is worth
/// knowing before you read the log. The bracketing run, same image, same
/// router:
///
/// - `NROS_FREERTOS_APP_STACK_KB=40` → boots, serves goals, and reports
///   `app task stack peak 36160 of 40960 bytes (4800 free)`. The measurement
///   predicts the boundary to within 5 KiB.
/// - `NROS_FREERTOS_APP_STACK_KB=32` → `*** MALLOC FAILED ***` and a hang.
///   **NOT** `*** STACK OVERFLOW: <task> ***`, even though the shared
///   `FreeRTOSConfig.h` sets `configCHECK_FOR_STACK_OVERFLOW 2` and the hook is
///   wired: heap_4 hands out the task stack, so the overflow lands in the
///   ADJACENT heap block header and the next `pvPortMalloc` fails before any
///   context switch can check the pattern. The kernel's own stack check cannot
///   win that race here.
///
/// So the way to know is the boot line, not the failure.
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

// =============================================================================
// The FreeRTOS heap default (issue 1197) — DERIVED, from terms that were
// measured on running images, not bisected.
// =============================================================================

/// App-sized task stacks the default heap budgets for.
///
/// Three: the boot task plus two spawned tiers. `app_stack_bytes` is charged
/// **per TASK** — a tier whose `[tiers.*] stack_bytes` is 0 takes the app
/// default too — so a tiered image multiplies this term rather than sharing it.
/// The deepest in-tree image (`examples/workspaces/realtime-rust`) declares two
/// tiers; three is one more than anything shipped, and a deeper image raises
/// `NROS_FREERTOS_HEAP_KB` with its own `heap peak` line as the evidence.
pub const DEFAULT_HEAP_APP_TASK_SLOTS: usize = 3;

/// Heap an executor costs when it is NOT the one that took the `.bss`
/// reservation — issue 1197's residue, and the only backing term left in this
/// budget.
///
/// phase-392 W6 put ONE executor's per-entry storage in the named `.bss` static
/// `nros_node::executor::backing::EXECUTOR_BACKING`, so the FIRST executor an
/// image opens costs the heap NOTHING and this budget has no term for it. That
/// is what makes the default independent of a size the board cannot see (issue
/// 1197's whole subject). A TIERED boot opens one executor per tier, and
/// `backing::take` is a latch — the second and later ones fall through to
/// `Box::leak` out of this heap, which is correct and deliberate.
///
/// MEASURED: `examples/workspaces/realtime-rust` (2 tiers) peaks at **389,064**
/// bytes of heap against **176,920** for the worst single-executor image
/// (`rust/action-client`), both at a 131,072-byte app stack. The difference
/// beyond the second task's stack is **93,216** bytes — a default-sized backing
/// (87,496 by `arm-none-eabi-nm -S` on the wake-latency images, which declare no
/// entities) plus heap_4 block overhead. 131,072 is 1.4x it.
///
/// This is the one term a backing change can re-stale, and it is the one term
/// every image PRINTS a check on: `nros: heap peak <used> of <total>`.
pub const DEFAULT_HEAP_SPARE_EXECUTOR_BYTES: usize = 131_072;

/// Everything in the heap that is neither an app-sized task stack nor a spare
/// executor: the zenoh-pico read + lease task stacks (5,120 each), the network
/// poll task (1 KiB), the heap-peak reporter (2 KiB), lwIP's tcpip thread and
/// per-socket allocations, and the RMW's per-entity working set.
///
/// MEASURED at **45,848** bytes worst — `rust/action-client`, 176,920 peak less
/// its one 131,072 app stack. The C/C++ carrier agrees to within 1 KiB
/// (`c/action-server` 569,064 less its 524,288 stack = **44,776**), which is the
/// cross-check that this term is a property of the transport and the netstack
/// rather than of a language lane. 65,536 is 1.43x the worse of the two.
pub const DEFAULT_HEAP_WORKING_SET_BYTES: usize = 65_536;

/// The FreeRTOS heap default for a zenoh image, in BYTES.
///
/// ```text
/// app_stack_bytes * SLOTS + SPARE_EXECUTOR * (SLOTS - 1) + WORKING_SET
/// ```
///
/// At the shipped 131,072-byte app stack that is **720,896** bytes (704 KiB),
/// replacing a bisected 2 MiB literal that predates both of the reductions it
/// was still sized for:
///
/// * issue 1146 lowered `app_stack_bytes` 393,216 -> 131,072, charged per task;
/// * issue 1197 / phase-392 W6 moved the first executor's backing to `.bss`,
///   so this budget stopped holding it a second time.
///
/// Doing those in two commits is how the subtraction ends up applied twice, and
/// a too-small heap on this port does not say `*** STACK OVERFLOW ***` — heap_4
/// hands out the task stacks, so it says `*** MALLOC FAILED ***` and hangs
/// (issue 1146 measured that too). Hence one derivation, and hence every image
/// prints its own `nros: heap peak` line so the next person reads the number
/// instead of re-bisecting it.
///
/// VERIFIED by running every in-tree FreeRTOS mps2-an385 image against a live
/// `rmw_zenohd`: the worst peak is `realtime-rust`'s **389,064**, so the default
/// carries 1.85x what the deepest image has ever asked for.
///
/// Not applied to the cyclone / XRCE lane, which keeps `FreeRTOSConfig.h`'s
/// 3 MiB: DDS discovery's working set is a different measurement and this PR did
/// not take it.
pub const fn default_heap_bytes(app_stack_bytes: u32) -> usize {
    (app_stack_bytes as usize) * DEFAULT_HEAP_APP_TASK_SLOTS
        + DEFAULT_HEAP_SPARE_EXECUTOR_BYTES * (DEFAULT_HEAP_APP_TASK_SLOTS - 1)
        + DEFAULT_HEAP_WORKING_SET_BYTES
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
    fn the_heap_default_tracks_the_app_stack_it_holds() {
        // issue 1197 — the point of the derivation: lower the stack (issue
        // 1146's half) and the heap follows, with no second edit to forget.
        assert_eq!(default_heap_bytes(DEFAULT_APP_STACK_BYTES), 720_896);
        assert_eq!(
            default_heap_bytes(DEFAULT_APP_STACK_BYTES) - default_heap_bytes(65_536),
            (DEFAULT_APP_STACK_BYTES as usize - 65_536) * DEFAULT_HEAP_APP_TASK_SLOTS
        );
    }

    #[test]
    fn the_heap_default_covers_the_worst_measured_image() {
        // `examples/workspaces/realtime-rust` on qemu mps2-an385 against a live
        // `rmw_zenohd`, 2026-09-12: `nros: heap peak 389064 of 2097152 bytes`.
        // A term that shrinks below this is a `*** MALLOC FAILED ***` at boot,
        // which no build-time check can see.
        const WORST_MEASURED_PEAK: usize = 389_064;
        assert!(
            default_heap_bytes(DEFAULT_APP_STACK_BYTES) >= WORST_MEASURED_PEAK,
            "the derived heap default no longer covers the deepest image this \
             tree has measured — re-run the FreeRTOS images and read their \
             `nros: heap peak` lines before moving a term"
        );
    }

    #[test]
    fn the_poll_task_outranks_the_app_task() {
        // CLAUDE.md's FreeRTOS pitfall: a poll task that does not outrank the
        // app task starves, and lwIP never drains the LAN9118 RX FIFO.
        let sched = FreertosScheduling::default();
        assert!(sched.poll_priority > sched.app_priority);
    }
}
