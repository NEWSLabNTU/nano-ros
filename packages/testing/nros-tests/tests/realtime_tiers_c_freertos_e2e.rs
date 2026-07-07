//! phase-281 W2 — runtime E2E for embedded **C** **RFC-0015 Model 1**
//! (`run_tiers`) on FreeRTOS/mps2-an385 (QEMU). The `ws-realtime-c-mps2`
//! workspace — the C sibling of `ws-realtime-cpp-mps2` — deploys two C nodes on
//! two priority tiers over ONE shared zenoh session, each an `Executor` on its
//! own FreeRTOS task:
//!   - `ctrl`  — high tier, 10 ms period (boot task)
//!   - `telem` — low tier, 100 ms period (spawned task)
//!
//! `FreertosBoard::run_tiers` (→ the SHARED C `nros_board_freertos_run_tiers`
//! glue) opens the session on the boot task, then spawns the non-boot tier.
//! This is the same C `run_tiers` impl the C++ freertos e2e
//! (`realtime_tiers_cpp_freertos_e2e`) exercises — here it drives C *nodes*,
//! closing the `C × freertos` cell of the execution-model convergence matrix
//! (the codegen routes embedded-C through the C++ emitter, instantiating each C
//! node via its `NROS_C_COMPONENT` `extern "C"` seam). Each node's `on_tick`
//! calls `nros_cpp_publish_raw(...)` and prints `[<tier>] tick=N` **only when
//! the publish succeeds** — so observing BOTH `[ctrl]` AND `[telem]` ticks
//! proves (a) the shared session connects to the host zenohd, (b) the boot
//! (high) tier publishes, and (c) the spawned (low) tier's borrowed executor
//! over the SAME session schedules + publishes at its period.
//!
//! The firmware uses a static 192.0.3.x lwIP config, so `start_mps2_an385_
//! freertos_slirp` runs a matching slirp net (host 192.0.3.1) and the entry
//! dials `tcp/192.0.3.1:17871` (baked). No TAP / bridge / root.
//!
//! Run with: `cargo nextest run -p nros-tests --test realtime_tiers_c_freertos_e2e`

use nros_tests::fixtures::{
    QemuProcess, ZenohRouter, build_freertos_workspace_c_realtime_entry, freertos,
    is_qemu_available, require_zenohd,
};
use std::time::Duration;

/// Router port baked into the realtime entry's locator (see the
/// `workspace-c-freertos-realtime` fixture's `NROS_ENTRY_LOCATOR`).
const REALTIME_ENTRY_PORT: u16 = 17871;

#[test]
fn c_freertos_run_tiers_both_tiers_publish() {
    if !require_zenohd() {
        nros_tests::skip!("zenohd not found");
    }
    if !freertos::is_freertos_available() {
        nros_tests::skip!("FREERTOS_DIR not set or invalid");
    }
    if !freertos::is_lwip_available() {
        nros_tests::skip!("LWIP_DIR not set or invalid");
    }
    if !freertos::is_arm_gcc_available() {
        nros_tests::skip!("arm-none-eabi-gcc not found");
    }
    if !is_qemu_available() {
        nros_tests::skip!("qemu-system-arm not found");
    }

    let entry = build_freertos_workspace_c_realtime_entry()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|e| nros_tests::skip!("realtime C tiers fixture not built: {e}"));

    // Host zenohd on 0.0.0.0:<port>; slirp maps the board's gateway 192.0.3.1 to
    // the host, so the guest reaches it. The nodes' `publish_raw` (and thus the
    // `[tier] tick` prints) only succeed once this session is up.
    let _router = ZenohRouter::start_on("0.0.0.0", REALTIME_ENTRY_PORT).unwrap_or_else(|e| {
        nros_tests::skip!("zenohd failed to start on {REALTIME_ENTRY_PORT}: {e}")
    });

    let mut qemu = QemuProcess::start_mps2_an385_freertos_slirp(&entry)
        .unwrap_or_else(|e| panic!("boot realtime freertos QEMU: {e}"));

    // High tier (boot task) connects + publishes first — a `[ctrl] tick` proves
    // the run_tiers boot session reached the host zenohd.
    let out_ctrl = qemu
        .wait_for_output_pattern("[ctrl] tick=", Duration::from_secs(90))
        .unwrap_or_else(|e| {
            qemu.kill();
            panic!(
                "high tier (ctrl) never published over the shared session — the \
                 run_tiers boot executor did not connect.\nerr: {e:?}"
            )
        });
    assert!(out_ctrl.contains("[ctrl] tick="));

    // Low tier (spawned task, borrowed executor over the SAME session) must also
    // publish — proving the shared C `nros_board_freertos_run_tiers` glue spawned
    // the per-tier task and gated it on the one shared session for a C *node*.
    let out_telem = qemu
        .wait_for_output_pattern("[telem] tick=", Duration::from_secs(30))
        .unwrap_or_else(|e| {
            qemu.kill();
            panic!(
                "low tier (telem) never published — the spawned tier task's \
                 borrowed executor over the shared session did not run.\nerr: {e:?}"
            )
        });
    qemu.kill();

    assert!(
        out_telem.contains("[telem] tick="),
        "expected both tiers to publish (ctrl high 10 ms + telem low 100 ms)"
    );
}
